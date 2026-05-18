use crate::shared::wgsl_primitives::*;

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

pub struct BufferValues {
    pub delta_n: Vec2f,
    pub zoom: f32,
    pub ref_iteration: u32,
    pub z_n_prime: Vec2f,
    pub zoom_prime: f32,
    pub orbits: Vec4f,
    pub stripes: Vec4f,
    pub step: i32,
}

/// The parameters for the preview shader. This is sent as a uniform
/// to the preview shader.
#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Transform {
    pub angle: f32,
    pub _padding: f32,
    pub prescale: Vec2f,
    pub postscale: Vec2f,
    pub offset: Vec2f,
}
