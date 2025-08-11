use eframe::wgpu::Extent3d;
use rug::{Float, ops::PowAssign};

use super::get_precision;

/// A representation of the current viewed portion of the fractal
#[derive(Debug, Clone, PartialEq)]
pub struct Viewport {
    pub width: usize,
    pub height: usize,
    pub zoom: f64,
    pub x: Float,
    pub y: Float,
}

/// A representation of the current image being rendered, including
/// the viewport, coloring, and other parameters
#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    pub viewport: Viewport,
    pub max_iter: usize,
    pub probe_location: (Float, Float),
    pub coloring: Coloring,
    pub misc: f32,
    pub debug_shutter: f32,
}

/// The coloring parameters for the image
#[derive(Debug, Clone, Copy, PartialEq)]
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
        let offset: [Float; 2] = [
            (self.x.clone() - other.x.clone()) / this_scale.clone(),
            (self.y.clone() - other.y.clone())
                / this_scale
                / (self.height as f32 / self.width as f32),
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

    /// Gets the fractal coordinates of a pixel from viewport coordinates
    pub fn get_real_coords(&self, x: f64, y: f64) -> (Float, Float) {
        let precision = get_precision(self.zoom);
        let mut scale = Float::with_val(precision, 2.0);
        scale.pow_assign(-self.zoom);

        let r = ((x / self.width as f64) * 2.0 - 1.0) * scale.clone()
            + Float::with_val(precision, &self.x);
        let i = ((y / self.height as f64) * 2.0 - 1.0) * scale.clone() / self.aspect_ratio()
            + Float::with_val(precision, &self.y);
        (r, i)
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
            probe_location: (Float::with_val(53, -0.5), Float::with_val(53, 0.0)),
            max_iter: 10000,
            coloring: Coloring::default(),
            misc: 1.0,
            debug_shutter: 0.0,
        }
    }
}
