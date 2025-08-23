use std::{num::NonZeroU64, sync::Arc};

use color_eyre::Result;
use eframe::{
    egui::mutex::RwLock,
    wgpu::{self, Device, include_wgsl, util::DeviceExt},
};
use wgpu::{Extent3d, Queue};

use super::Transform;

/// Resources necessary for rendering the preview image
pub struct PreviewRenderResources {
    format: wgpu::TextureFormat,
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    texture_bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    texture: wgpu::Texture,
    preview_texture: Arc<RwLock<wgpu::Texture>>,
    output_texture: Arc<RwLock<wgpu::Texture>>,
    size: (u32, u32),
}

impl PreviewRenderResources {
    /// Create a new set of preview render resources
    pub fn init(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        preview_texture: Arc<RwLock<wgpu::Texture>>,
        output_texture: Arc<RwLock<wgpu::Texture>>,
        size: (u32, u32),
    ) -> Result<Self> {
        let shader = device.create_shader_module(include_wgsl!("../shaders/preview.wgsl"));

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            size: Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            label: Some(format!("Texture at time {:?}", std::time::Instant::now()).as_str()),
            view_formats: &[],
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Preview Bind Group Layout"),
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

        let fractal_texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let fractal_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
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
                label: Some("Fractal Texture Bind Group Layout"),
            });
        let texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&fractal_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&fractal_sampler),
                },
            ],
            label: Some("Fractal Texture Bind Group"),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Preview Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout, &texture_bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Preview Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions {
                    constants: &[],
                    zero_initialize_workgroup_memory: false,
                },
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(format.into())],
                compilation_options: wgpu::PipelineCompilationOptions {
                    constants: &[],
                    zero_initialize_workgroup_memory: false,
                },
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 4,
                ..Default::default()
            },
            multiview: None,
            cache: None,
        });

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Preview Uniform Buffer"),
            contents: bytemuck::cast_slice(&[Transform {
                angle: 0.0,
                scale: 0.0,
                offset: [0.0; 2],
            }]),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Preview Bind Group"),
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
            preview_texture,
            output_texture,
            size,
        })
    }

    /// Resize the render resources. This must be called when the render thread resizes,
    /// and will refresh the texture view and the uniform buffer.
    pub fn resize(&mut self, device: &Device, queue: &Queue, new_size: (u32, u32)) -> Result<()> {
        *self = Self::init(
            device,
            self.format,
            self.preview_texture.clone(),
            self.output_texture.clone(),
            new_size,
        )?;
        self.swap(device, queue);
        Ok(())
    }

    pub fn swap(&self, device: &Device, queue: &Queue) {
        if self.texture.size() != self.preview_texture.read().size() {
            return;
        }
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        encoder.copy_texture_to_texture(
            self.preview_texture.read().as_image_copy(),
            self.texture.as_image_copy(),
            self.texture.size(),
        );
        queue.submit([encoder.finish()]);
    }

    /// Prepare the render resources for a new frame; for use in a callback
    pub fn prepare(&self, _device: &Device, queue: &Queue, transform: Transform) {
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[transform]));
    }

    /// Render the preview to the given render pass; for use in a callback
    pub fn paint(&self, render_pass: &mut wgpu::RenderPass) {
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.set_bind_group(1, &self.texture_bind_group, &[]);
        render_pass.draw(0..6, 0..1);
    }

    /// Get the size of the preview
    pub fn size(&self) -> &(u32, u32) {
        &self.size
    }
}
