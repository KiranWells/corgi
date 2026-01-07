use std::fs::read_to_string;
use std::path::PathBuf;

use color_eyre::eyre::{Result, eyre};
use eframe::egui::Vec2;
use eframe::wgpu::Extent3d;
use little_exif::exif_tag::ExifTag;
use little_exif::metadata::Metadata;
use rug::Float;
use rug::ops::{CompleteRound, PowAssign};
use serde::{Deserialize, Serialize};

use super::{Coloring, Transform, get_precision};
use crate::image_gen::is_metadata_supported;
use crate::types::{Layer, LayerKind, next_layer_id};

// We use a custom implementation for serde
// of Float to get a radix of 10. This increases
// the space it takes on disk, but that is a smaller
// concern for this app.
#[derive(Deserialize, Serialize)]
#[serde(remote = "Float")]
struct FloatParser {
    #[serde(getter = "Float::value")]
    value: String,
    #[serde(getter = "Float::prec")]
    precision: u32,
}

trait Translate {
    fn value(&self) -> String;
}

impl Translate for Float {
    fn value(&self) -> String {
        self.to_string_radix(10, None)
    }
}

impl From<FloatParser> for Float {
    fn from(value: FloatParser) -> Self {
        Float::parse(value.value.clone())
            .map(|val| val.complete(value.precision))
            .unwrap_or(Float::new(53))
    }
}

#[derive(Debug, Clone)]
pub struct View {
    pub center: ComplexPoint,
    pub zoom: f32,
    pub width: u32,
    pub height: u32,
}

/// A representation of the current viewed portion of the fractal
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Parameters {
    // canvas
    pub width: u32,
    pub height: u32,
    pub samples: u8,
    // fractal parameters
    pub fractal_kind: FractalKind,
    // viewport
    pub center: ComplexPoint,
    pub zoom: f32,
    pub max_iter: u32,
    // internal rendering details
    pub probe_location: ComplexPoint,
}

/// A representation of the current viewed portion of the fractal
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct Viewport {
    // canvas
    pub width: u32,
    pub height: u32,
    pub samples: u8,
    // viewport
    pub center: ComplexPoint,
    pub zoom: f32,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ComplexPoint {
    #[serde(with = "FloatParser")]
    pub x: Float,
    #[serde(with = "FloatParser")]
    pub y: Float,
}

/// A representation of the current image being rendered, including
/// the viewport, coloring, and other parameters
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct Image {
    pub parameters: Parameters,
    pub external_coloring: Coloring,
    pub internal_coloring: Coloring,
    #[serde(skip)]
    pub optimization_level: OptLevel,
}

impl Image {
    pub fn view(&self) -> View {
        View {
            center: self.parameters.center.clone(),
            zoom: self.parameters.zoom,
            width: self.parameters.width,
            height: self.parameters.height,
        }
    }
}

/// A representation of the current image being rendered, including
/// the viewport, coloring, and other parameters
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct _Image {
    // fractal parameters
    pub fractal_kind: FractalKind,
    pub viewport: Viewport,
    pub max_iter: u32,
    // internal rendering details
    pub probe_location: ComplexPoint,
    pub external_coloring: Coloring,
    pub internal_coloring: Coloring,
    #[serde(skip)]
    pub optimization_level: OptLevel,
}

impl Default for _Image {
    fn default() -> Self {
        Self {
            viewport: Viewport {
                width: 512,
                height: 512,
                samples: 1,
                zoom: -1.0,
                center: ComplexPoint {
                    x: Float::with_val(53, -0.5),
                    y: Float::with_val(53, 0.0),
                },
            },
            fractal_kind: FractalKind::Mandelbrot,
            max_iter: 10000,
            probe_location: ComplexPoint {
                x: Float::with_val(53, -0.5),
                y: Float::with_val(53, 0.0),
            },
            external_coloring: Coloring::default(),
            internal_coloring: Coloring::internal_default(),
            optimization_level: OptLevel::AccuracyOptimized,
        }
    }
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            width: 512,
            height: 512,
            samples: 1,
            zoom: -1.0,
            center: ComplexPoint {
                x: Float::with_val(53, -0.5),
                y: Float::with_val(53, 0.0),
            },
        }
    }
}

impl From<_Image> for Image {
    fn from(value: _Image) -> Self {
        Self {
            parameters: Parameters {
                width: value.viewport.width,
                height: value.viewport.height,
                samples: 1,
                fractal_kind: value.fractal_kind,
                center: value.viewport.center,
                zoom: value.viewport.zoom,
                max_iter: value.max_iter,
                probe_location: value.probe_location,
            },
            external_coloring: value.external_coloring,
            internal_coloring: value.internal_coloring,
            optimization_level: value.optimization_level,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Algorithm {
    Directf32,
    Perturbedf32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum OptLevel {
    #[default]
    CacheOptimized,
    AccuracyOptimized,
    PerformanceOptimized,
}

#[derive(Debug, Default, Clone, PartialEq, Deserialize, Serialize)]
pub enum FractalKind {
    #[default]
    Mandelbrot,
    Julia(ComplexPoint),
}

#[derive(Clone, Copy, Debug)]
pub struct ImageDiff {
    pub reprobe: bool,
    pub recompute: bool,
    pub recolor: bool,
    pub rebuild: bool,
}

impl Default for Image {
    fn default() -> Self {
        Self {
            parameters: Parameters::default(),
            external_coloring: Coloring::default(),
            internal_coloring: Coloring::internal_default(),
            optimization_level: OptLevel::AccuracyOptimized,
        }
    }
}

impl Default for Parameters {
    fn default() -> Self {
        Parameters {
            fractal_kind: FractalKind::Mandelbrot,
            width: 512,
            height: 512,
            samples: 1,
            zoom: -1.0,
            max_iter: 10000,
            center: ComplexPoint {
                x: Float::with_val(53, -0.5),
                y: Float::with_val(53, 0.0),
            },
            probe_location: ComplexPoint {
                x: Float::with_val(53, -0.5),
                y: Float::with_val(53, 0.0),
            },
        }
    }
}

impl Default for ComplexPoint {
    fn default() -> Self {
        Self {
            x: Float::new(53),
            y: Float::new(53),
        }
    }
}

impl Image {
    pub fn algorithm(&self) -> Algorithm {
        self.view().algorithm()
    }

    pub fn comp(&self, other: &Self) -> ImageDiff {
        // determine if we need to reallocate buffers or recompile shaders
        // (due to changing compile-time parameters)
        let rebuild = self.parameters.width != other.parameters.width
            || self.parameters.height != other.parameters.height
            // if there are more bits set, then there are more enabled features
            || (self.get_flags() & 0xFF).count_ones() > (other.get_flags() & 0xFF).count_ones()
            || self.get_flags() & 0xFF00_0000 != other.get_flags() & 0xFF00_0000
            || self.parameters.max_iter != other.parameters.max_iter;
        // if the max iteration or probe location has changed, re-run the probe
        let reprobe = self.parameters.max_iter != other.parameters.max_iter
            || self.parameters.probe_location != other.parameters.probe_location
            || self.algorithm() == Algorithm::Perturbedf32
                && other.algorithm() == Algorithm::Directf32
            || self.parameters.fractal_kind != other.parameters.fractal_kind
            || rebuild;
        // if the probe location has changed or the image viewport has changed, re-generate the delta grid
        // if the image generation parameters have changed, re-run the compute shader
        let recompute = self.parameters != other.parameters || reprobe;
        // if the image coloring parameters have changed, re-run the image render
        let recolor = self.external_coloring != other.external_coloring
            || self.internal_coloring != other.internal_coloring
            || recompute;
        ImageDiff {
            reprobe,
            recompute,
            recolor,
            rebuild,
        }
    }

    pub fn load_from_file(path: &PathBuf) -> Result<Self> {
        let mut image: Image = if is_metadata_supported(path) {
            let meta = Metadata::new_from_path(path)?;
            let tag = meta
                .get_tag(&ExifTag::ImageDescription(String::new()))
                .next()
                .ok_or(eyre!("No Description tag"))?;
            let ExifTag::ImageDescription(desc) = tag else {
                return Err(eyre!("Tag is not a Description"));
            };
            let img: _Image = serde_json::from_str(desc)?;
            img.into()
        } else {
            let img: _Image = read_to_string(path)
                .map_err(color_eyre::Report::from)
                .and_then(|s| serde_json::from_str(&s).map_err(color_eyre::Report::from))?;
            img.into()
        };
        fn update_ids(layers: &mut [Layer]) {
            for layer in layers {
                layer.id = next_layer_id();
            }
        }
        update_ids(&mut image.internal_coloring.color_layers);
        update_ids(&mut image.internal_coloring.light_layers);
        update_ids(&mut image.external_coloring.color_layers);
        update_ids(&mut image.external_coloring.light_layers);
        Ok(image)
    }

    pub fn update_probe(&mut self) {
        let mut relative_pos = self
            .view()
            .coords_to_px_offset(&self.parameters.probe_location);
        relative_pos = (
            relative_pos.0 / self.parameters.width as f64,
            relative_pos.1 / self.parameters.height as f64,
        );
        if relative_pos.0.abs() > 10.0 || relative_pos.1.abs() > 10.0 {
            // reset probe
            self.parameters.probe_location = self.parameters.center.clone();
        }
    }

    pub fn get_flags(&self) -> u32 {
        const STRIPES_ENABLED: u32 = 0x1;
        const TOTAL_ANGLE_ENABLED: u32 = 0x2;
        const ORBIT_ENABLED: u32 = 0x4;
        const DERIVATIVE_ENABLED: u32 = 0x8;
        const JULIA: u32 = 0x1000_0000;
        let kind_flags = match &self.parameters.fractal_kind {
            FractalKind::Mandelbrot => 0,
            FractalKind::Julia(_) => JULIA,
        };
        match self.optimization_level {
            OptLevel::CacheOptimized => {
                STRIPES_ENABLED
                    | TOTAL_ANGLE_ENABLED
                    | ORBIT_ENABLED
                    | DERIVATIVE_ENABLED
                    | kind_flags
            }
            OptLevel::AccuracyOptimized | OptLevel::PerformanceOptimized => {
                let mut flags = 0;
                if self.contains_kind(LayerKind::Stripe) {
                    flags |= STRIPES_ENABLED;
                }
                if self.internal_coloring.contains_kind(LayerKind::Step)
                    || self.internal_coloring.contains_kind(LayerKind::SmoothStep)
                {
                    flags |= TOTAL_ANGLE_ENABLED;
                }
                if self.contains_kind(LayerKind::OrbitTrap) {
                    flags |= ORBIT_ENABLED;
                }
                if self.external_coloring.contains_kind(LayerKind::Distance)
                    || self.external_coloring.overlays.set_outline.is_some()
                {
                    flags |= DERIVATIVE_ENABLED;
                }
                flags | kind_flags
            }
        }
    }
    pub fn contains_kind(&self, kind: LayerKind) -> bool {
        self.external_coloring.contains_kind(kind) || self.internal_coloring.contains_kind(kind)
    }
}

impl View {
    /// Derives the transforms from another viewport to this one
    pub fn transforms_from(&self, other: &Self) -> Transform {
        let scale = f32::powf(2.0, -(self.zoom - other.zoom));
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

    pub fn zoom_offset_from(&self, other: &Self) -> f32 {
        let aspect = self.aspect_ratio() as f32;
        let other_aspect = other.aspect_ratio() as f32;
        if aspect < 1.0 {
            if other_aspect < 1.0 {
                (other_aspect / aspect).max(1.0)
            } else {
                1.0 / aspect
            }
        } else if other_aspect > 1.0 {
            (aspect / other_aspect).max(1.0)
        } else {
            aspect
        }
        .log2()
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
    pub fn get_real_coords(&self, x: f64, y: f64, scaling: f64) -> (Float, Float) {
        let precision = get_precision(self.zoom);
        let mut scale = Float::with_val(precision, 2.0);
        scale.pow_assign(-self.zoom);
        let aspect_scale = self.aspect_scale();

        let r = ((x / self.width as f64 / scaling) * 2.0 - 1.0) * scale.clone() * aspect_scale.x
            + Float::with_val(precision, &self.center.x);
        let i = ((y / self.height as f64 / scaling) * 2.0 - 1.0) * scale.clone() * aspect_scale.y
            + Float::with_val(precision, &self.center.y);
        (r, i)
    }

    /// Returns the offset in pixels from the center of this viewport to
    /// the given location in fractal coordinates
    pub fn coords_to_px_offset(&self, point: &ComplexPoint) -> (f64, f64) {
        let precision = get_precision(self.zoom);
        let mut scale = Float::with_val(precision, 2.0);
        scale.pow_assign(-self.zoom);
        let aspect_scale = self.aspect_scale();

        let x = ((point.x.clone() - self.center.x.clone()) / scale.clone()).to_f64()
            / aspect_scale.x as f64;
        let y =
            ((point.y.clone() - self.center.y.clone()) / scale).to_f64() / aspect_scale.y as f64;
        (x * 0.5 * self.width as f64, y * 0.5 * self.height as f64)
    }

    pub fn algorithm(&self) -> Algorithm {
        match self.zoom {
            x if x < 13.0 => Algorithm::Directf32,
            _ => Algorithm::Perturbedf32,
        }
    }

    pub fn buffer_size(&self) -> usize {
        (self.width as f64) as usize * (self.height as f64) as usize
    }
}
impl Parameters {
    pub fn update_prec(&mut self) {
        let prec = get_precision(self.zoom);
        self.center.x = Float::with_val(prec, self.center.x.clone());
        self.center.y = Float::with_val(prec, self.center.y.clone());
    }
}

impl View {
    pub fn extents(&self) -> Extent3d {
        Extent3d {
            width: (self.width as f64) as u32,
            height: (self.height as f64) as u32,
            depth_or_array_layers: 1,
        }
    }
}
impl Image {
    pub fn extents(&self) -> Extent3d {
        Extent3d {
            width: (self.parameters.width as f64) as u32,
            height: (self.parameters.height as f64) as u32,
            depth_or_array_layers: 1,
        }
    }
}

impl ImageDiff {
    pub fn full() -> Self {
        ImageDiff {
            rebuild: true,
            reprobe: true,
            recompute: true,
            recolor: true,
        }
    }
}
