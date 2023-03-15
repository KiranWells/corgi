use std::{
    error::Error,
    fmt::Display,
    num::NonZeroU64,
    sync::{Arc, RwLock},
};

use color_eyre::{eyre::eyre, Report, Result};
use rug::{ops::PowAssign, Float};
use tracing::info;
use wgpu::{
    util::DeviceExt, AdapterInfo, Device, Dx12Compiler, Queue, Surface, SurfaceConfiguration,
};
use winit::{dpi::PhysicalSize, window::Window};

pub const ESCAPE_RADIUS: f64 = 1e10;
pub const MAX_GPU_GROUP_ITER: usize = 500;

pub fn get_precision(zoom: f64) -> u32 {
    ((zoom * 1.5) as u32).max(53)
}

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

#[derive(Default)]
pub struct Status {
    pub message: String,
    pub progress: Option<f64>,
    pub rendered_image: Option<Image>,
    // TODO: severity
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

pub struct GPUData {
    // general GPU Handles
    pub device: Arc<Device>,
    pub queue: Arc<Queue>,
    // Windowing handles
    pub surface: Surface,
    pub surface_config: SurfaceConfiguration,
}

pub struct InternalState {
    pub size: PhysicalSize<u32>,
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

impl GPUData {
    pub async fn init(window: &Window) -> Result<Self> {
        let size = window.inner_size();

        // The instance is a handle to our GPU
        // Backends::all => Vulkan + Metal + DX12 + Browser WebGPU
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::GL,
            dx12_shader_compiler: Dx12Compiler::default(),
        });
        let surface = unsafe { instance.create_surface(window) }?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or(eyre!("Failed to find an appropriate adapter"))?;

        let use_high_precision_float = adapter.features().contains(wgpu::Features::SHADER_FLOAT64);

        let (device, queue) = if let Ok(r) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    features: if use_high_precision_float {
                        wgpu::Features::SHADER_FLOAT64
                    } else {
                        wgpu::Features::empty()
                    },
                    limits: wgpu::Limits::default(),
                },
                None,
            )
            .await
        {
            r
        } else {
            adapter
                .request_device(
                    &wgpu::DeviceDescriptor {
                        label: None,
                        features: wgpu::Features::empty(),
                        limits: wgpu::Limits::default(),
                    },
                    None,
                )
                .await?
        };

        let surface_config = surface
            .get_default_config(&adapter, size.width, size.height)
            .ok_or(eyre!(
                "Failed to get default surface config, is this surface supported?"
            ))?;
        surface.configure(&device, &surface_config);

        let AdapterInfo { name, backend, .. } = adapter.get_info();
        info!("Running on {name} with the {backend:?} backend");

        Ok(Self {
            device: Arc::new(device),
            queue: Arc::new(queue),
            surface,
            surface_config,
        })
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        // recreate the surface with the new size
        self.surface_config.width = new_size.width;
        self.surface_config.height = new_size.height;
        self.surface.configure(&self.device, &self.surface_config);
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

pub struct PreviewRenderResources {
    format: wgpu::TextureFormat,
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    texture_bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    texture: Arc<RwLock<wgpu::Texture>>,
    size: (usize, usize),
}

impl PreviewRenderResources {
    pub fn init(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        texture: Arc<RwLock<wgpu::Texture>>,
        size: (usize, usize),
    ) -> Result<Self> {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("custom3d"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/preview.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("custom3d"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: NonZeroU64::new(16),
                },
                count: None,
            }],
        });
        let diffuse_texture_view = texture
            .read()
            .map_err(|e| eyre!("Failed to read texture: {:?}", e))?
            .create_view(&wgpu::TextureViewDescriptor::default());
        let diffuse_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        // This should match the filterable field of the
                        // corresponding Texture entry above.
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
                label: Some("texture_bind_group_layout"),
            });
        let texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&diffuse_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&diffuse_sampler),
                },
            ],
            label: Some("diffuse_bind_group"),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("custom3d"),
            bind_group_layouts: &[&bind_group_layout, &texture_bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("custom3d"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(format.into())],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("custom3d"),
            contents: bytemuck::cast_slice(&[Transform {
                angle: 0.0,
                scale: 0.0,
                offset: [0.0; 2],
            }]),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("custom3d"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        Ok(Self {
            format,
            pipeline,
            bind_group,
            texture_bind_group,
            uniform_buffer,
            texture,
            size,
        })
    }

    pub fn resize(&mut self, device: &Device, new_size: (usize, usize)) -> Result<()> {
        *self = Self::init(device, self.format, self.texture.clone(), new_size)?;
        Ok(())
    }

    pub fn prepare(&self, _device: &wgpu::Device, queue: &wgpu::Queue, transform: Transform) {
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[transform]));
    }

    pub fn paint<'rp>(&'rp self, render_pass: &mut wgpu::RenderPass<'rp>) {
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.set_bind_group(1, &self.texture_bind_group, &[]);
        render_pass.draw(0..6, 0..1);
    }

    pub fn size(&self) -> &(usize, usize) {
        &self.size
    }
}

#[derive(Debug)]
pub enum RenderErr {
    Resize,
    Quit(Report),
    Warn(Report),
}

impl From<wgpu::SurfaceError> for RenderErr {
    fn from(e: wgpu::SurfaceError) -> Self {
        match e {
            wgpu::SurfaceError::Lost => Self::Resize,
            wgpu::SurfaceError::OutOfMemory => Self::Quit(e.into()),
            wgpu::SurfaceError::Timeout => Self::Warn(e.into()),
            wgpu::SurfaceError::Outdated => Self::Warn(e.into()),
        }
    }
}

impl From<Report> for RenderErr {
    fn from(e: Report) -> Self {
        Self::Quit(e)
    }
}

impl Display for RenderErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Resize => write!(f, "Resize"),
            Self::Quit(e) => write!(f, "Quit: {}", e),
            Self::Warn(e) => write!(f, "Warn: {}", e),
        }
    }
}

impl Error for RenderErr {}
