/*!
# Shader Types

This module contains the types that are passed to the GPU and
conversion logic from internal types.
 */
use crate::types::{Coloring, ImgSpec, Light, Outline, Overlays};

// These constants need to match the values defined in the
// compute shaders.
pub const MAX_GRADIENT_STOPS: usize = 50;
pub const MAX_LAYERS: usize = 8;
pub const MAX_LIGHTS: usize = 3;

/// The GPU-safe version of coloring data. This is sent as a uniform
/// to the compute shader.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ColorParams {
    pub saturation: f32,
    pub brightness: f32,
    pub color_frequency: f32,
    pub color_offset: f32,
    pub gradient_kind: u32,
    pub gradient_size: u32,
    pub lighting_kind: u32,
    padding: u32,
    pub color_layer_types: [u8; MAX_LAYERS],
    pub light_layer_types: [u8; MAX_LAYERS],
    pub color_strengths: [f32; MAX_LAYERS],
    pub color_params: [f32; MAX_LAYERS],
    pub light_strengths: [f32; MAX_LAYERS],
    pub light_params: [f32; MAX_LAYERS],
    pub lights: [Light; MAX_LIGHTS],
    pub overlays: OverlayParams,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct OverlayParams {
    pub iteration_outline: [f32; 4],
    pub set_outline: [f32; 4],
}

impl From<&Coloring> for ColorParams {
    fn from(value: &Coloring) -> Self {
        let (gradient_kind, gradient_vec) = value.gradient.decompose();
        fn map_to_array<T: Default + Copy, U, const N: usize>(
            v: &[U],
            f: impl Fn(&U) -> T,
        ) -> [T; N] {
            let mut arr = [T::default(); N];
            let newv = v.iter().map(f).collect::<Vec<T>>();
            arr[..newv.len()].copy_from_slice(&newv);
            arr
        }
        ColorParams {
            saturation: value.saturation,
            brightness: value.brightness,
            color_frequency: value.color_frequency,
            color_offset: value.color_offset,
            gradient_kind,
            gradient_size: gradient_vec.len() as u32 / 4,
            lighting_kind: value.lighting_kind as u32,
            color_layer_types: map_to_array(&value.color_layers, |x| x.kind as u8),
            light_layer_types: map_to_array(&value.light_layers, |x| x.kind as u8),
            color_strengths: map_to_array(&value.color_layers, |x| x.strength),
            color_params: map_to_array(&value.color_layers, |x| x.param),
            light_strengths: map_to_array(&value.light_layers, |x| x.strength),
            light_params: map_to_array(&value.light_layers, |x| x.param),
            lights: map_to_array(&value.lights, Light::clone),
            overlays: (&value.overlays).into(),
            padding: 0,
        }
    }
}

impl From<&Overlays> for OverlayParams {
    fn from(value: &Overlays) -> Self {
        Self {
            iteration_outline: pack_outline(&value.iteration_outline),
            set_outline: pack_outline(&value.set_outline),
        }
    }
}

fn pack_outline(value: &Option<Outline>) -> [f32; 4] {
    if let Some(inner) = value {
        let mut packed = inner.color.to_rgba_unmultiplied();
        packed[3] *= 0.999;
        packed[3] += inner.parameter as f32;
        packed
    } else {
        [0.0; 4]
    }
}

/// The parameters for the compute shader. This is sent as a uniform
/// to the compute shader.
#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ComputeParams {
    pub width: u32,
    pub height: u32,
    pub max_iter: u32,
    pub chunk_max_iter: u32,
    pub probe_len: u32,
    pub iter_offset: u32,
    pub x: f32,
    pub y: f32,
    pub zoom: f32,
    pub angle: f32,
    pub julia_x: f32,
    pub julia_y: f32,
}

/// The parameters for the render shader. This is sent as a uniform
/// to the render shader.
#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct RenderParams {
    pub width: u32,
    pub height: u32,
}

impl From<&ImgSpec> for RenderParams {
    fn from(image: &ImgSpec) -> Self {
        RenderParams {
            width: (image.width as f64) as u32,
            height: (image.height as f64) as u32,
        }
    }
}

/// The parameters for the preview shader. This is sent as a uniform
/// to the preview shader.
#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Transform {
    pub angle: f32,
    pub _padding: f32,
    pub prescale: [f32; 2],
    pub postscale: [f32; 2],
    pub offset: [f32; 2],
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            angle: 0.0,
            _padding: 0.0,
            prescale: [1.0, 1.0],
            postscale: [1.0, 1.0],
            offset: [0.0, 0.0],
        }
    }
}
