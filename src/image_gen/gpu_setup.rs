/*!
# GPU Setup

This initializes the GPU and creates the buffers and shaders.

*/

use std::sync::{Arc, RwLock};

use color_eyre::{eyre::eyre, Result};
use wgpu::{
    BindGroup, BindGroupLayoutEntry, Buffer, ComputePipeline, Device, PipelineLayout, Queue,
    Texture, TextureView,
};

use crate::types::{ComputeParams, Image, RenderParams, Viewport, MAX_GPU_GROUP_ITER};

pub struct GPUData {
    // general GPU Handles
    pub device: Arc<Device>,
    pub queue: Arc<Queue>,
    // Rendering data
    pub compute_shader: wgpu::ShaderModule,
    pub compute_pipeline: ComputePipeline,
    pub render_pipeline_layout: PipelineLayout,
    pub rendered_image: Arc<RwLock<Texture>>,
    pub buffers: Buffers,
    pub bind_groups: BindGroups,
}

pub struct Buffers {
    // compute input
    pub probe: Buffer,
    // pub probe_prime: Buffer,
    pub delta_0: Buffer,
    pub delta_n: Buffer,
    pub delta_prime: Buffer,
    // parameters
    pub compute_parameters: Buffer,
    pub render_parameters: Buffer,
    // intermediate data
    pub step: Buffer,
    pub orbit: Buffer,
    pub r: Buffer,
    pub dr: Buffer,
    // output buffers
    pub readable: Buffer,
}

pub struct BindGroups {
    pub compute_buffers: BindGroup,
    pub compute_parameters: BindGroup,
    pub render_buffers: BindGroup,
    pub render_parameters: BindGroup,
    pub render_texture: BindGroup,
}

impl GPUData {
    pub async fn init(image: &Image, device: Arc<Device>, queue: Arc<Queue>) -> Self {
        let compute_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/calculate.wgsl").into()),
        });

        // create a texture for the image
        let rendered_image = Self::create_texture(&device, &image.viewport);
        let final_texture_view =
            rendered_image.create_view(&wgpu::TextureViewDescriptor::default());

        let buffers = Buffers::init(&device, &image.viewport);
        let (bind_groups, compute_pipeline_layout, render_pipeline_layout) =
            BindGroups::init(&device, &buffers, &final_texture_view);

        // Create compute pipeline
        let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: None,
            layout: Some(&compute_pipeline_layout),
            module: &compute_shader,
            entry_point: "main_mandel",
        });

        Self {
            device,
            queue,
            compute_shader,
            compute_pipeline,
            render_pipeline_layout,
            rendered_image: Arc::new(RwLock::new(rendered_image)),
            buffers,
            bind_groups,
        }
    }

    pub fn resize(&mut self, new_view: &Viewport) -> Result<()> {
        // recreate the texture with the new size
        let rendered_image = Self::create_texture(&self.device, new_view);
        let texture_view = rendered_image.create_view(&wgpu::TextureViewDescriptor::default());
        // resize the buffers
        self.buffers.resize(new_view, &self.device);

        let (bind_groups, compute_pipeline_layout, render_pipeline_layout) =
            BindGroups::init(&self.device, &self.buffers, &texture_view);

        self.bind_groups = bind_groups;

        self.compute_pipeline =
            self.device
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: None,
                    layout: Some(&compute_pipeline_layout),
                    module: &self.compute_shader,
                    entry_point: "main_mandel",
                });
        self.render_pipeline_layout = render_pipeline_layout;

        *self
            .rendered_image
            .write()
            .map_err(|e| eyre!("Failed to lock image: {:?}", e))? = rendered_image;
        Ok(())
    }

    fn create_texture(device: &Device, viewport: &Viewport) -> wgpu::Texture {
        let texture_size = Self::get_texture_size(viewport);
        device.create_texture(&wgpu::TextureDescriptor {
            // All textures are stored as 3D, we represent our 2D texture
            // by setting depth to 1.
            size: texture_size,
            mip_level_count: 1, // We'll talk about this a little later
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            // TEXTURE_BINDING tells wgpu that we want to use this texture in shaders
            // COPY_DST means that we want to copy data to this texture
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            label: Some(format!("Texture at time {:?}", std::time::Instant::now()).as_str()),
            view_formats: &[],
        })
    }

    pub fn get_texture_size(viewport: &Viewport) -> wgpu::Extent3d {
        // pads the width to a multiple of 256 bytes
        let width = viewport.width as u32; // (viewport.width as f32 * 4.0 / 256.0).ceil() as u32 * 64;
        wgpu::Extent3d {
            width,
            height: viewport.height as u32,
            depth_or_array_layers: 1,
        }
    }
}

enum BuffType {
    ShaderOnly,
    HostWritable,
    HostReadable,
    Uniform,
}
impl Buffers {
    fn init(device: &Device, viewport: &Viewport) -> Self {
        use BuffType::*;
        let image_size = viewport.width * viewport.height;
        Self {
            probe: Self::create_buffer::<f32>(device, MAX_GPU_GROUP_ITER * 2 * 2, HostWritable),
            // probe_prime: Self::create_buffer::<f32>(device, MAX_GPU_GROUP_ITER * 2, HostWritable),
            delta_0: Self::create_buffer::<f32>(device, image_size * 2, HostWritable),
            delta_n: Self::create_buffer::<f32>(device, image_size * 2, ShaderOnly),
            delta_prime: Self::create_buffer::<f32>(device, image_size * 2, ShaderOnly),
            compute_parameters: Self::create_buffer::<ComputeParams>(device, 1, Uniform),
            render_parameters: Self::create_buffer::<RenderParams>(device, 1, Uniform),
            step: Self::create_buffer::<u32>(device, image_size, ShaderOnly),
            orbit: Self::create_buffer::<f32>(device, image_size, ShaderOnly),
            r: Self::create_buffer::<f32>(device, image_size, ShaderOnly),
            dr: Self::create_buffer::<f32>(device, image_size, ShaderOnly),
            readable: Self::create_buffer::<u32>(device, image_size, HostReadable),
        }
    }

    fn create_buffer<T>(device: &Device, size: usize, ty: BuffType) -> Buffer {
        use BuffType::*;
        device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (size * core::mem::size_of::<T>()) as u64,
            usage: match ty {
                ShaderOnly => wgpu::BufferUsages::STORAGE,
                HostWritable => wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                HostReadable => wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                Uniform => wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            },
            mapped_at_creation: false,
        })
    }

    pub fn resize(&mut self, new_view: &Viewport, device: &Device) {
        use BuffType::*;
        // replace all sized buffers (deltas, intermediates, and readable)
        let image_size = new_view.width * new_view.height;
        let texture_size = GPUData::get_texture_size(new_view);
        self.delta_0 = Self::create_buffer::<f32>(device, image_size * 2, HostWritable);
        self.delta_n = Self::create_buffer::<f32>(device, image_size * 2, ShaderOnly);
        self.delta_prime = Self::create_buffer::<f32>(device, image_size * 2, ShaderOnly);
        self.step = Self::create_buffer::<u32>(device, image_size, ShaderOnly);
        self.orbit = Self::create_buffer::<f32>(device, image_size, ShaderOnly);
        self.r = Self::create_buffer::<f32>(device, image_size, ShaderOnly);
        self.dr = Self::create_buffer::<f32>(device, image_size, ShaderOnly);
        self.readable = Self::create_buffer::<u32>(
            device,
            (texture_size.width * texture_size.height) as usize,
            HostReadable,
        );
    }
}

impl BindGroups {
    fn init(
        device: &Device,
        buffers: &Buffers,
        texture_view: &TextureView,
    ) -> (Self, PipelineLayout, PipelineLayout) {
        let Buffers {
            probe,
            // probe_prime,
            delta_0,
            delta_n,
            delta_prime,
            step,
            orbit,
            r,
            dr,
            // readable,
            compute_parameters,
            render_parameters,
            ..
        } = buffers;

        // create bind group layout for the compute shader
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                Self::create_buffer_layout_entry(0, true),
                // Self::create_buffer_layout_entry(1, true),
                Self::create_buffer_layout_entry(2, true),
                Self::create_buffer_layout_entry(3, false),
                Self::create_buffer_layout_entry(4, false),
                Self::create_buffer_layout_entry(5, false),
                Self::create_buffer_layout_entry(6, false),
                Self::create_buffer_layout_entry(7, false),
                Self::create_buffer_layout_entry(8, false),
            ],
        });

        // create a bind group for the compute shader
        let compute_buffers = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: probe.as_entire_binding(),
                },
                // wgpu::BindGroupEntry {
                //     binding: 1,
                //     resource: probe_prime.as_entire_binding(),
                // },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: delta_0.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: delta_n.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: delta_prime.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: step.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: orbit.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: r.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: dr.as_entire_binding(),
                },
            ],
            label: None,
        });

        let params_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[Self::create_uniform_layout_entry(0)],
            });

        let compute_parameters = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &params_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: compute_parameters.as_entire_binding(),
            }],
            label: None,
        });

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        view_dimension: wgpu::TextureViewDimension::D2,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        access: wgpu::StorageTextureAccess::WriteOnly,
                    },
                    count: None,
                }],
                label: Some("texture_bind_group_layout"),
            });

        let render_texture = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &texture_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(texture_view),
            }],
            label: Some("texture_bind_group"),
        });

        // create a bind group layout for the intermediates for the color shader
        let render_buffers_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[
                    Self::create_buffer_layout_entry(0, true),
                    Self::create_buffer_layout_entry(1, true),
                    Self::create_buffer_layout_entry(2, true),
                    Self::create_buffer_layout_entry(3, true),
                ],
            });

        // create a bind group for the render shader
        let render_buffers = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &render_buffers_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: step.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: orbit.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: r.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: dr.as_entire_binding(),
                },
            ],
            label: None,
        });

        let params_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let render_parameters = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &params_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: render_parameters.as_entire_binding(),
            }],
            label: None,
        });

        let compute_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[&bind_group_layout, &params_bind_group_layout],
                push_constant_ranges: &[],
            });

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[
                    &render_buffers_layout,
                    &texture_bind_group_layout,
                    &params_bind_group_layout,
                ],
                push_constant_ranges: &[],
            });

        (
            Self {
                compute_buffers,
                compute_parameters,
                render_buffers,
                render_parameters,
                render_texture,
            },
            compute_pipeline_layout,
            render_pipeline_layout,
        )
    }

    pub fn refresh_texture_bind_group(&mut self, device: &Device, texture_view: &TextureView) {
        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        view_dimension: wgpu::TextureViewDimension::D2,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        access: wgpu::StorageTextureAccess::WriteOnly,
                    },
                    count: None,
                }],
                label: Some("texture_bind_group_layout"),
            });
        self.render_texture = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &texture_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(texture_view),
            }],
            label: Some("texture_bind_group"),
        });
    }

    const fn create_buffer_layout_entry(binding: u32, read_only: bool) -> BindGroupLayoutEntry {
        wgpu::BindGroupLayoutEntry {
            binding,
            count: None,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                has_dynamic_offset: false,
                min_binding_size: None,
                ty: wgpu::BufferBindingType::Storage { read_only },
            },
        }
    }

    const fn create_uniform_layout_entry(binding: u32) -> BindGroupLayoutEntry {
        wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }
    }
}
