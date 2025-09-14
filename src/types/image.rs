use std::{
    fs::{OpenOptions, read_to_string},
    io::Write,
    path::PathBuf,
};

use color_eyre::eyre::{Result, eyre};
use eframe::{egui::Vec2, wgpu::Extent3d};
use little_exif::{exif_tag::ExifTag, metadata::Metadata};
use nanoserde::{DeJson, SerJson};
use rug::{
    Float,
    ops::{CompleteRound, PowAssign},
};

use crate::image_gen::is_metadata_supported;

use super::{Coloring, Transform, get_precision};

#[derive(DeJson, SerJson)]
struct FloatParser {
    value: String,
    precision: u32,
}

impl From<&FloatParser> for Float {
    fn from(value: &FloatParser) -> Self {
        Float::parse(value.value.clone())
            .map(|val| val.complete(value.precision))
            .unwrap_or(Float::new(53))
    }
}

impl From<&Float> for FloatParser {
    fn from(val: &Float) -> Self {
        FloatParser {
            value: val.to_string_radix(10, None),
            precision: val.prec(),
        }
    }
}

/// A representation of the current viewed portion of the fractal
#[derive(Debug, Clone, PartialEq, DeJson, SerJson)]
pub struct Viewport {
    pub width: usize,
    pub height: usize,
    pub scaling: f64,
    pub zoom: f64,
    pub center: ComplexPoint,
}

#[derive(Debug, Clone, PartialEq, DeJson, SerJson)]
pub struct ComplexPoint {
    #[nserde(proxy = "FloatParser")]
    pub x: Float,
    #[nserde(proxy = "FloatParser")]
    pub y: Float,
}

/// A representation of the current image being rendered, including
/// the viewport, coloring, and other parameters
#[derive(Debug, Clone, PartialEq, DeJson, SerJson)]
pub struct Image {
    pub viewport: Viewport,
    pub max_iter: u64,
    pub probe_location: ComplexPoint,
    pub external_coloring: Coloring,
    pub internal_coloring: Coloring,
    pub misc: f32,
    pub debug_shutter: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Algorithm {
    Directf32,
    Perturbedf32,
}

#[derive(Clone, Copy, Debug)]
pub struct ImageDiff {
    pub reprobe: bool,
    pub recompute: bool,
    pub recolor: bool,
    pub resize: bool,
}

impl Default for Image {
    fn default() -> Self {
        Self {
            viewport: Viewport::default(),
            probe_location: ComplexPoint {
                x: Float::with_val(53, -0.5),
                y: Float::with_val(53, 0.0),
            },
            max_iter: 10000,
            external_coloring: Coloring::default(),
            internal_coloring: Coloring::internal_default(),
            misc: 1.0,
            debug_shutter: 0.0,
        }
    }
}

impl Default for Viewport {
    fn default() -> Self {
        Viewport {
            width: 512,
            height: 512,
            scaling: 1.0,
            zoom: -1.0,
            center: ComplexPoint {
                x: Float::with_val(53, -0.5),
                y: Float::with_val(53, 0.0),
            },
        }
    }
}

impl Image {
    pub fn algorithm(&self) -> Algorithm {
        self.viewport.algorithm()
    }

    pub fn comp(&self, other: &Self) -> ImageDiff {
        // if the viewport has changed, resize the GPU data
        let resize = self.viewport.width != other.viewport.width
            || self.viewport.height != other.viewport.height
            || self.viewport.scaling != other.viewport.scaling
            || self.max_iter != other.max_iter;
        // if the max iteration or probe location has changed, re-run the probe
        let reprobe = self.max_iter != other.max_iter
            || self.probe_location.x != other.probe_location.x
            || self.probe_location.y != other.probe_location.y
            || self.viewport.algorithm() == Algorithm::Perturbedf32
                && other.viewport.algorithm() == Algorithm::Directf32
            || resize;
        // if the probe location has changed or the image viewport has changed, re-generate the delta grid
        // if the image generation parameters have changed, re-run the compute shader
        let recompute =
            self.max_iter != other.max_iter || self.viewport != other.viewport || reprobe;
        // if the image coloring parameters have changed, re-run the image render
        let recolor = self.external_coloring != other.external_coloring
            || self.internal_coloring != other.internal_coloring
            || recompute
            || self.misc != other.misc
            || self.debug_shutter != other.debug_shutter;
        ImageDiff {
            reprobe,
            recompute,
            recolor,
            resize,
        }
    }

    pub fn load_from_file(path: &PathBuf) -> Result<Self> {
        let image = if is_metadata_supported(path) {
            let meta = Metadata::new_from_path(path)?;
            let tag = meta
                .get_tag(&ExifTag::ImageDescription(String::new()))
                .next()
                .ok_or(eyre!("No Description tag"))?;
            let ExifTag::ImageDescription(desc) = tag else {
                return Err(eyre!("Tag is not a Description"));
            };
            Image::deserialize_json(desc)?
        } else {
            read_to_string(path)
                .map_err(color_eyre::Report::from)
                .and_then(|s| Image::deserialize_json(&s).map_err(color_eyre::Report::from))?
        };
        Ok(image)
    }

    pub fn save_to_file(&self, path: &PathBuf) -> Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(path)?;
        let serialized = self.serialize_json();
        let written_amt = file.write(serialized.as_bytes())?;
        if written_amt < serialized.len() {
            Err(eyre!("Failed to write all of the image!"))
        } else {
            Ok(())
        }
    }

    pub fn update_probe(&mut self) {
        let mut relative_pos = self
            .viewport
            .coords_to_px_offset(&self.probe_location.x, &self.probe_location.y);
        relative_pos = (
            relative_pos.0 / self.viewport.width as f64,
            relative_pos.1 / self.viewport.height as f64,
        );
        if relative_pos.0.abs() > 10.0 || relative_pos.1.abs() > 10.0 {
            // reset probe
            self.probe_location = self.viewport.center.clone();
        }
    }
}

impl Viewport {
    /// Derives the transforms from another viewport to this one
    pub fn transforms_from(&self, other: &Self) -> Transform {
        let scale = f32::powf(2.0, -(self.zoom - other.zoom) as f32);
        let mut this_scale = Float::with_val(get_precision(self.zoom), 2.0);
        this_scale.pow_assign(-self.zoom);
        let self_aspect = self.aspect_scale();
        let aspect_scale = self_aspect / other.aspect_scale();
        let offset: [Float; 2] = [
            (self.center.x.clone() - other.center.x.clone()) / this_scale.clone() / self_aspect.x,
            (self.center.y.clone() - other.center.y.clone()) / this_scale / self_aspect.y,
        ];
        Transform {
            angle: 0.0,
            _padding: 0.0,
            scale: [scale * aspect_scale.x, scale * aspect_scale.y],
            offset: [offset[0].to_f32(), offset[1].to_f32()],
        }
    }

    /// The aspect ratio of the viewport
    pub fn aspect_ratio(&self) -> f64 {
        self.width as f64 / self.height as f64
    }

    pub fn aspect_scale(&self) -> Vec2 {
        let aspect = self.aspect_ratio() as f32;
        if aspect < 1.0 {
            Vec2::new(aspect, 1.0)
        } else {
            Vec2::new(1.0, 1.0 / aspect)
        }
    }

    /// Gets the fractal coordinates of a pixel from viewport coordinates
    pub fn get_real_coords(&self, x: f64, y: f64) -> (Float, Float) {
        let precision = get_precision(self.zoom);
        let mut scale = Float::with_val(precision, 2.0);
        scale.pow_assign(-self.zoom);
        let aspect_scale = self.aspect_scale();

        let r = ((x / self.width as f64) * 2.0 - 1.0) * scale.clone() * aspect_scale.x
            + Float::with_val(precision, &self.center.x);
        let i = ((y / self.height as f64) * 2.0 - 1.0) * scale.clone() * aspect_scale.y
            + Float::with_val(precision, &self.center.y);
        (r, i)
    }

    /// Returns the offset in pixels from the center of this viewport to
    /// the given location in fractal coordinates
    pub fn coords_to_px_offset(&self, r: &Float, i: &Float) -> (f64, f64) {
        let precision = get_precision(self.zoom);
        let mut scale = Float::with_val(precision, 2.0);
        scale.pow_assign(-self.zoom);
        let aspect_scale = self.aspect_scale();

        let x =
            ((r.clone() - self.center.x.clone()) / scale.clone()).to_f64() / aspect_scale.x as f64;
        let y = ((i.clone() - self.center.y.clone()) / scale).to_f64() / aspect_scale.y as f64;
        (x * 0.5 * self.width as f64, y * 0.5 * self.height as f64)
    }

    pub fn algorithm(&self) -> Algorithm {
        match self.zoom {
            x if x < 13.0 => Algorithm::Directf32,
            _ => Algorithm::Perturbedf32,
        }
    }

    pub fn buffer_size(&self) -> usize {
        (self.width as f64 * self.scaling) as usize * (self.height as f64 * self.scaling) as usize
    }

    pub fn update_prec(&mut self) {
        let prec = get_precision(self.zoom);
        self.center.x = Float::with_val(prec, self.center.x.clone());
        self.center.y = Float::with_val(prec, self.center.y.clone());
    }
}

impl From<&Viewport> for Extent3d {
    fn from(viewport: &Viewport) -> Self {
        Self {
            width: (viewport.width as f64 * viewport.scaling) as u32,
            height: (viewport.height as f64 * viewport.scaling) as u32,
            depth_or_array_layers: 1,
        }
    }
}

impl ImageDiff {
    pub fn full() -> Self {
        ImageDiff {
            resize: true,
            reprobe: true,
            recompute: true,
            recolor: true,
        }
    }
}
