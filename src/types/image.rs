use rug::{ops::PowAssign, Float};

use super::get_precision;

#[derive(Debug, Clone, PartialEq)]
pub struct Viewport {
    pub width: usize,
    pub height: usize,
    pub zoom: f64,
    pub x: Float,
    pub y: Float,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    pub viewport: Viewport,
    pub max_iter: usize,
    pub probe_location: (Float, Float),
    pub coloring: Coloring,
    pub misc: f32,
}

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

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ComputeParams {
    pub width: u32,
    pub height: u32,
    pub max_iter: u32,
    pub probe_len: u32,
    pub iter_offset: u32,
}

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
}

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
            internal_brightness: 1.0,
            saturation: 1.0,
        }
    }
}

impl Viewport {
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

    pub fn aspect_ratio(&self) -> f64 {
        self.width as f64 / self.height as f64
    }

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

impl Image {
    pub fn to_render_params(&self) -> RenderParams {
        RenderParams {
            image_width: self.viewport.width as u32,
            max_step: self.max_iter as u32,
            zoom: self.viewport.zoom as f32,
            saturation: self.coloring.saturation,
            color_frequency: self.coloring.color_frequency,
            color_offset: self.coloring.color_offset,
            glow_spread: self.coloring.glow_spread,
            glow_intensity: self.coloring.glow_intensity,
            brightness: self.coloring.brightness,
            internal_brightness: self.coloring.internal_brightness,
            misc: self.misc,
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
            max_iter: 1000,
            coloring: Coloring::default(),
            misc: 1.0,
        }
    }
}
