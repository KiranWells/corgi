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

impl ComputeParams {
    pub fn create(image: &crate::types::ImgSpec, probe_len: usize) -> Self {
        Self::create_with_alg(image, probe_len, image.algorithm())
    }
    pub fn create_with_alg(
        image: &crate::types::ImgSpec,
        probe_len: usize,
        algorithm: crate::types::Algorithm,
    ) -> Self {
        use crate::types::Algorithm::*;
        let julia_point = match &image.location.fractal_kind {
            crate::types::FractalKind::Mandelbrot => emath::Vec2::new(0.0, 0.0),
            crate::types::FractalKind::Julia(pt) => pt.to_vec2(),
        };
        let (pt, probe_len) = match algorithm {
            Directf32 | Directf32CPU | DirectFloatCPU => {
                (image.location.center.to_vec2(), image.location.max_iter)
            }
            Perturbedf32 | Perturbedf32CPU => {
                let offset = image
                    .view()
                    .complex_to_px_delta(&image.location.probe_location);
                (offset / image.size(), probe_len as u32)
            }
        };
        ComputeParams {
            width: image.width,
            height: image.height,
            max_iter: image.location.max_iter,
            chunk_max_iter: 0,
            probe_len,
            iter_offset: 0,
            x: pt.x,
            y: pt.y,
            zoom: image.location.zoom,
            angle: image.location.angle,
            julia_x: julia_point.x,
            julia_y: julia_point.y,
        }
    }

    pub fn with_iter(
        mut self,
        image: &crate::types::ImgSpec,
        i: u32,
        iter_batch_size: u32,
    ) -> Self {
        self.chunk_max_iter = if (i + 1) * iter_batch_size > image.location.max_iter {
            image.location.max_iter % iter_batch_size
        } else {
            iter_batch_size
        };
        self.iter_offset = i * iter_batch_size;
        self
    }
}

#[derive(Debug, Clone)]
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

impl BufferValues {
    pub fn zero() -> Self {
        BufferValues {
            delta_n: Vec2::splat(0.0),
            zoom: 0.0,
            ref_iteration: 0,
            z_n_prime: Vec2::splat(0.0),
            zoom_prime: 0.0,
            orbits: Vec4::splat(0.0),
            stripes: Vec4::splat(0.0),
            step: 0,
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
    pub prescale: Vec2f,
    pub postscale: Vec2f,
    pub offset: Vec2f,
}
