use eframe::{egui::Vec2, wgpu::Extent3d};
use nanoserde::{DeJson, SerJson};
use rug::{
    Float,
    ops::{CompleteRound, PowAssign},
};

use super::get_precision;

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
    pub zoom: f64,
    #[nserde(proxy = "FloatParser")]
    pub x: Float,
    #[nserde(proxy = "FloatParser")]
    pub y: Float,
}

#[derive(Debug, Clone, PartialEq, DeJson, SerJson)]
pub struct ProbeLocation {
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
    pub max_iter: usize,
    pub probe_location: ProbeLocation,
    pub coloring: Coloring,
    pub misc: f32,
    pub debug_shutter: f32,
}

/// The coloring parameters for the image
#[derive(Debug, Clone, Copy, PartialEq, DeJson, SerJson)]
pub struct Coloring {
    pub saturation: f32,
    pub color_frequency: f32,
    pub color_offset: f32,
    pub glow_spread: f32,
    pub glow_intensity: f32,
    pub brightness: f32,
    pub internal_brightness: f32,
}

/// The parameters for the compute shader. This is sent as a uniform
/// to the compute shader.
#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ComputeParams {
    pub width: u32,
    pub height: u32,
    pub max_iter: u32,
    pub probe_len: u32,
    pub iter_offset: u32,
}

/// The parameters for the render shader. This is sent as a uniform
/// to the render shader.
#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct RenderParams {
    pub image_width: u32,
    pub max_step: u32,
    pub zoom: f32,
    pub saturation: f32,
    pub color_frequency: f32,
    pub color_offset: f32,
    pub glow_spread: f32,
    pub glow_intensity: f32,
    pub brightness: f32,
    pub internal_brightness: f32,
    pub misc: f32,
    pub debug_shutter: f32,
}

/// The parameters for the preview shader. This is sent as a uniform
/// to the preview shader.
#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Transform {
    pub angle: f32,
    pub scale: f32,
    pub offset: [f32; 2],
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            angle: 0.0,
            scale: 1.0,
            offset: [0.0, 0.0],
        }
    }
}

impl Default for Coloring {
    fn default() -> Self {
        Self {
            color_frequency: 1.0,
            color_offset: 0.0,
            glow_spread: 1.0,
            glow_intensity: 1.0,
            brightness: 2.0,
            internal_brightness: 0.5,
            saturation: 1.0,
        }
    }
}

impl Viewport {
    /// Derives the transforms from another viewport to this one
    pub fn transforms_from(&self, other: &Self) -> Transform {
        let scale = f32::powf(2.0, -(self.zoom - other.zoom) as f32);
        let mut this_scale = Float::with_val(get_precision(self.zoom), 2.0);
        this_scale.pow_assign(-self.zoom);
        let aspect_scale = self.aspect_scale();
        let offset: [Float; 2] = [
            (self.x.clone() - other.x.clone()) / this_scale.clone() / aspect_scale.x,
            (self.y.clone() - other.y.clone()) / this_scale / aspect_scale.y,
        ];
        Transform {
            angle: 0.0,
            scale,
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
            + Float::with_val(precision, &self.x);
        let i = ((y / self.height as f64) * 2.0 - 1.0) * scale.clone() * aspect_scale.y
            + Float::with_val(precision, &self.y);
        (r, i)
    }

    /// Returns the offset in pixels from the center of this viewport to
    /// the given location in fractal coordinates
    pub fn coords_to_px_offset(&self, r: &Float, i: &Float) -> (f64, f64) {
        let precision = get_precision(self.zoom);
        let mut scale = Float::with_val(precision, 2.0);
        scale.pow_assign(-self.zoom);
        let aspect_scale = self.aspect_scale();

        let x = ((r.clone() - self.x.clone()) / scale.clone()).to_f64() / aspect_scale.x as f64;
        let y = ((i.clone() - self.y.clone()) / scale).to_f64() / aspect_scale.y as f64;
        (x * 0.5 * self.width as f64, y * 0.5 * self.height as f64)
    }
}

impl From<&Viewport> for Extent3d {
    fn from(viewport: &Viewport) -> Self {
        Self {
            width: viewport.width as u32,
            height: viewport.height as u32,
            depth_or_array_layers: 1,
        }
    }
}

impl From<&Image> for RenderParams {
    fn from(image: &Image) -> Self {
        RenderParams {
            image_width: image.viewport.width as u32,
            max_step: image.max_iter as u32,
            zoom: image.viewport.zoom as f32,
            saturation: image.coloring.saturation,
            color_frequency: image.coloring.color_frequency,
            color_offset: image.coloring.color_offset,
            glow_spread: image.coloring.glow_spread,
            glow_intensity: image.coloring.glow_intensity,
            brightness: image.coloring.brightness,
            internal_brightness: image.coloring.internal_brightness,
            misc: image.misc,
            debug_shutter: image.debug_shutter,
        }
    }
}

impl Default for Image {
    fn default() -> Self {
        Self {
            viewport: Viewport {
                width: 512,
                height: 512,
                zoom: -2.0,
                x: Float::with_val(53, -0.5),
                y: Float::with_val(53, 0.0),
            },
            probe_location: ProbeLocation {
                x: Float::with_val(53, -0.5),
                y: Float::with_val(53, 0.0),
            },
            max_iter: 10000,
            coloring: Coloring::default(),
            misc: 1.0,
            debug_shutter: 0.0,
        }
    }
}
