use std::ops::Deref;
use std::sync::Arc;

use corgi_lib::shared::types::Transform;
use corgi_lib::shared::wgsl_primitives::Vec2;
use corgi_lib::types::View;
use eframe::egui::{self};
use eframe::egui_wgpu::{self, CallbackTrait};
use eframe::wgpu::util::DeviceExt;
use eframe::wgpu::{self, Device};
use parking_lot::RwLock;
use wgpu::{Extent3d, Queue};

use crate::ui::UITab;

/// Resources necessary for rendering the preview image
struct SubResources {
    format: wgpu::TextureFormat,
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    texture_bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    texture: wgpu::Texture,
    size: (u32, u32),
}

/// Resources necessary for rendering the preview image
/// for each tab.
pub struct PreviewRenderResources {
    preview: SubResources,
    output: SubResources,
    explore_texture: Arc<RwLock<wgpu::Texture>>,
    style_texture: Arc<RwLock<wgpu::Texture>>,
    output_texture: Arc<RwLock<wgpu::Texture>>,
}

pub struct ThumbnailRenderResources {
    sub: SubResources,
    texture: Arc<RwLock<wgpu::Texture>>,
}

impl PreviewRenderResources {
    pub fn init(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        explore_texture: Arc<RwLock<wgpu::Texture>>,
        style_texture: Arc<RwLock<wgpu::Texture>>,
        output_texture: Arc<RwLock<wgpu::Texture>>,
        preview_size: (u32, u32),
        output_size: (u32, u32),
    ) -> Self {
        let preview = SubResources::init(device, format, preview_size);
        let output = SubResources::init(device, format, output_size);
        Self {
            preview,
            output,
            explore_texture,
            style_texture,
            output_texture,
        }
    }
}

impl ThumbnailRenderResources {
    pub fn init(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        texture: Arc<RwLock<wgpu::Texture>>,
        size: (u32, u32),
    ) -> Self {
        let sub = SubResources::init(device, format, size);
        Self { sub, texture }
    }
}

impl SubResources {
    /// Create a new set of preview render resources
    pub fn init(device: &wgpu::Device, format: wgpu::TextureFormat, size: (u32, u32)) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("preview"),
            source: wgpu::ShaderSource::Wgsl(wesl::include_wesl!("preview").into()),
        });

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
                    min_binding_size: None,
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
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
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
            bind_group_layouts: &[Some(&bind_group_layout), Some(&texture_bind_group_layout)],
            immediate_size: 0,
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
            multiview_mask: None,
            cache: None,
        });

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Preview Uniform Buffer"),
            contents: bytemuck::cast_slice(&[Transform {
                angle: 0.0,
                _padding: 0.0,
                prescale: Vec2::splat(1.0),
                postscale: Vec2::splat(1.0),
                offset: Vec2::splat(0.0),
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

        Self {
            format,
            pipeline,
            bind_group,
            texture_bind_group,
            uniform_buffer,
            texture,
            size,
        }
    }

    /// Resize the render resources. This must be called when the render thread resizes,
    /// and will refresh the texture view and the uniform buffer.
    pub fn resize(
        &mut self,
        device: &Device,
        queue: &Queue,
        new_size: (u32, u32),
        source_texture: &impl Deref<Target = wgpu::Texture>,
    ) {
        *self = Self::init(device, self.format, new_size);
        self.swap(device, queue, source_texture);
    }

    /// Copies the source texture onto the preview texture
    pub fn swap(
        &self,
        device: &Device,
        queue: &Queue,
        source_texture: &impl Deref<Target = wgpu::Texture>,
    ) {
        if self.texture.size() != source_texture.size() {
            return;
        }
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        encoder.copy_texture_to_texture(
            source_texture.as_image_copy(),
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

/// Callback data for rendering the preview
pub struct PaintCallback {
    pub rendered_viewport: View,
    pub view: View,
    pub swap: bool,
    pub tab: UITab,
}

impl CallbackTrait for PaintCallback {
    fn prepare(
        &self,
        device: &eframe::wgpu::Device,
        queue: &eframe::wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _egui_encoder: &mut eframe::wgpu::CommandEncoder,
        callback_resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<eframe::wgpu::CommandBuffer> {
        let res = callback_resources
            .get_mut::<PreviewRenderResources>()
            .expect("to get render resources");
        let texture = match self.tab {
            UITab::Explore => res.explore_texture.read(),
            UITab::Style => res.style_texture.read(),
            UITab::Render => res.output_texture.read(),
        };
        let res = if self.tab == UITab::Render {
            &mut res.output
        } else {
            &mut res.preview
        };
        if self.swap {
            // copy the preview texture to the used texture
            res.swap(device, queue, &texture);
        }

        let extents = self.rendered_viewport.extents();
        let size = (extents.width, extents.height);
        if size != *res.size() {
            // resize the render resources, refreshing the texture reference
            res.resize(device, queue, size, &texture);
        }
        let transforms = self.rendered_viewport.transforms_from(&self.view);

        res.prepare(device, queue, transforms);
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut eframe::wgpu::RenderPass<'static>,
        callback_resources: &egui_wgpu::CallbackResources,
    ) {
        let res = callback_resources
            .get::<PreviewRenderResources>()
            .expect("to get render resources");
        let res = if self.tab == UITab::Render {
            &res.output
        } else {
            &res.preview
        };
        res.paint(render_pass);
    }
}

/// Callback data for rendering the preview
pub struct ThumbPaintCallback {
    pub size: (u32, u32),
    pub swap: bool,
}

impl CallbackTrait for ThumbPaintCallback {
    fn prepare(
        &self,
        device: &eframe::wgpu::Device,
        queue: &eframe::wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _egui_encoder: &mut eframe::wgpu::CommandEncoder,
        callback_resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<eframe::wgpu::CommandBuffer> {
        let res = callback_resources
            .get_mut::<ThumbnailRenderResources>()
            .expect("to get render resources");
        if self.swap {
            // copy the preview texture to the used texture
            res.sub.swap(device, queue, &res.texture.read());
        }

        if self.size != *res.sub.size() {
            // resize the render resources, refreshing the texture reference
            res.sub
                .resize(device, queue, self.size, &res.texture.read());
        }
        res.sub.prepare(device, queue, Transform::default());
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut eframe::wgpu::RenderPass<'static>,
        callback_resources: &egui_wgpu::CallbackResources,
    ) {
        let res = callback_resources
            .get::<ThumbnailRenderResources>()
            .expect("to get render resources");
        res.sub.paint(render_pass);
    }
}
