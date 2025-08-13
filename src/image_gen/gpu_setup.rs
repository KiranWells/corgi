/*!
# GPU Setup

This types contained in this module are designed to manage the GPU handles and data.
The `GPUData` struct contains all the handles and data needed to render an image,
and accepts a pre-existing device and queue to allow for multiple renderers to share
the same GPU.

*/

use std::sync::{Arc, mpsc};

use eframe::{
    egui::mutex::RwLock,
    egui_wgpu::RenderState,
    wgpu::{
        self, BindGroup, BindGroupLayoutEntry, Buffer, ComputePipeline, Device, PipelineLayout,
        Queue, Texture, TextureView,
    },
};
use wgpu::ShaderModule;

use crate::types::{ComputeParams, MAX_GPU_GROUP_ITER, RenderParams, Viewport};

/// A struct containing all of the GPU handles for the application
/// and the data needed to render an image. Use the `init` function
/// to create a new instance.
pub struct GPUData {
    /// The name for this Data object
    pub label: String,
    // general GPU Handles
    /// An Arc to the device
    pub device: Device,
    /// An Arc to the queue
    pub queue: Queue,
    // Rendering data
    /// The shader module for the compute shader
    pub compute_shader: ShaderModule,
    /// The compute pipeline for the compute shader
    pub compute_pipeline: ComputePipeline,
    /// The shader module for the color shader
    pub color_shader: ShaderModule,
    /// The color pipeline for the color shader
    pub color_pipeline: ComputePipeline,
    /// The texture that the image will be rendered to.
    /// This is shared between the renderer and caller.
    pub texture: Arc<RwLock<Texture>>,
    // Data
    /// A struct containing all of the buffers used by the GPU
    pub buffers: Buffers,
    /// A struct containing all of the bind groups used by the GPU
    pub bind_groups: BindGroups,
}

/// A struct containing all of the buffers used by the GPU
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

/// A struct containing all of the bind groups used by the GPU
pub struct BindGroups {
    pub compute_buffers: BindGroup,
    pub compute_parameters: BindGroup,
    pub render_buffers: BindGroup,
    pub render_parameters: BindGroup,
    pub render_texture: BindGroup,
}

impl GPUData {
    /// Initializes the GPU handles for use in rendering an image.
    pub fn init(
        viewport: &Viewport,
        wgpu: &RenderState,
        compute_shader: ShaderModule,
        label: &str,
    ) -> Self {
        let device = wgpu.device.clone();
        let queue = wgpu.queue.clone();

        let texture = Self::create_texture(&device, viewport);
        let final_texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let buffers = Buffers::init(&device, viewport);
        let (bind_groups, compute_pipeline_layout, render_pipeline_layout) =
            BindGroups::init(&device, &buffers, &final_texture_view);

        let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(format!("{label} Compute Pipeline").as_str()),
            layout: Some(&compute_pipeline_layout),
            module: &compute_shader,
            entry_point: Some("main_mandel"),
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &[],
                zero_initialize_workgroup_memory: false,
            },
            cache: None,
        });

        let color_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(format!("{label} Color Shader").as_str()),
            source: wgpu::ShaderSource::Wgsl(wesl::include_wesl!("color").into()),
        });

        let color_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(format!("{label} Color Pipeline").as_str()),
            layout: Some(&render_pipeline_layout),
            module: &color_shader,
            entry_point: Some("main_color"),
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &[],
                zero_initialize_workgroup_memory: false,
            },
            cache: None,
        });

        Self {
            label: label.into(),
            device,
            queue,
            compute_shader,
            compute_pipeline,
            color_shader,
            color_pipeline,
            texture: Arc::new(RwLock::new(texture)),
            buffers,
            bind_groups,
            // pipeline_cache,
        }
    }

    /// Resizes the image to the new viewport and recreates necessary handles.
    /// Any objects which created a texture view of the image will need to recreate it.
    pub fn resize(&mut self, new_view: &Viewport) {
        // recreate the texture with the new size
        let texture = Self::create_texture(&self.device, new_view);
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        self.buffers.resize(new_view, &self.device);

        let (bind_groups, compute_pipeline_layout, render_pipeline_layout) =
            BindGroups::init(&self.device, &self.buffers, &texture_view);

        self.bind_groups = bind_groups;

        self.compute_pipeline =
            self.device
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some(format!("{} Compute Pipeline", self.label).as_str()),
                    layout: Some(&compute_pipeline_layout),
                    module: &self.compute_shader,
                    entry_point: Some("main_mandel"),
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants: &[],
                        zero_initialize_workgroup_memory: false,
                    },
                    cache: None,
                });

        self.color_pipeline =
            self.device
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some(format!("{} Color Pipeline", self.label).as_str()),
                    layout: Some(&render_pipeline_layout),
                    module: &self.color_shader,
                    entry_point: Some("main_color"),
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants: &[],
                        zero_initialize_workgroup_memory: false,
                    },
                    cache: None,
                });

        *self.texture.write() = texture;
    }

    /// Creates a texture for the image to be rendered to.
    fn create_texture(device: &Device, viewport: &Viewport) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            size: viewport.into(),
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            label: Some(format!("Texture at time {:?}", std::time::Instant::now()).as_str()),
            view_formats: &[],
        })
    }

    pub fn get_texture_data(&self) -> Option<Vec<u8>> {
        let ext = self.texture.read().size();
        // let padded_width = ext.width * 4;
        let padded_width = ((ext.width * 4) as f32 / 256.0).ceil() as u32 * 256;
        let tmp_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: padded_width as u64 * ext.height as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        encoder.copy_texture_to_buffer(
            self.texture.read().as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &tmp_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_width),
                    rows_per_image: None,
                },
            },
            self.texture.read().size(),
        );
        self.queue.submit([encoder.finish()]);
        let slice = tmp_buffer.slice(..);
        let (send, recv) = mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |res| {
            let _ = send.send(res);
        });
        let _ = self.device.poll(wgpu::PollType::Wait);
        match recv.recv() {
            Ok(Ok(())) => {
                let mut out = Vec::new();
                for chunk in tmp_buffer
                    .slice(..)
                    .get_mapped_range()
                    .chunks(padded_width as usize)
                {
                    out.extend_from_slice(&chunk[..(ext.width * 4) as usize]);
                }
                Some(out)
            }
            Ok(Err(err)) => {
                tracing::error!("Error: {err:?}");
                None
            }
            Err(err) => {
                tracing::error!("Error: {err:?}");
                None
            }
        }
    }
}

/// The different types of buffers that can be created.
enum BuffType {
    /// A buffer that is only used by the shader, and is not accessible by the host.
    ShaderOnly,
    /// A buffer that can be written to by the host, but not read.
    HostWritable,
    /// A buffer that can be read by the host; used for the target of a copy operation.
    HostReadable,
    /// A uniform buffer that can be written by the host.
    Uniform,
}

impl Buffers {
    /// Creates all of the buffers used by the image renderer.
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

    /// Creates a buffer of the given type.
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

    /// Resizes the necessary buffers to the new viewport.
    /// Layouts generated from the buffers will need to be recreated.
    pub fn resize(&mut self, new_view: &Viewport, device: &Device) {
        use BuffType::*;
        // replace all sized buffers (not uniforms or probe)
        let image_size = new_view.width * new_view.height;
        self.delta_0 = Self::create_buffer::<f32>(device, image_size * 2, HostWritable);
        self.delta_n = Self::create_buffer::<f32>(device, image_size * 2, ShaderOnly);
        self.delta_prime = Self::create_buffer::<f32>(device, image_size * 2, ShaderOnly);
        self.step = Self::create_buffer::<u32>(device, image_size, ShaderOnly);
        self.orbit = Self::create_buffer::<f32>(device, image_size, ShaderOnly);
        self.r = Self::create_buffer::<f32>(device, image_size, ShaderOnly);
        self.dr = Self::create_buffer::<f32>(device, image_size, ShaderOnly);
        self.readable =
            Self::create_buffer::<u32>(device, new_view.width * new_view.height, HostReadable);
    }
}

impl BindGroups {
    /// Creates the bind groups for the compute and render pipelines.
    /// Returns the bind groups and the pipeline layouts for the compute and render pipelines.
    fn init(
        device: &Device,
        buffers: &Buffers,
        texture_view: &TextureView,
    ) -> (Self, PipelineLayout, PipelineLayout) {
        let Buffers {
            probe,
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

        // create the bind groups for the compute shader
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Compute Bind Group Layout"),
            entries: &[
                Self::create_buffer_layout_entry(0, true),
                Self::create_buffer_layout_entry(1, true),
                Self::create_buffer_layout_entry(2, false),
                Self::create_buffer_layout_entry(3, false),
                Self::create_buffer_layout_entry(4, false),
                Self::create_buffer_layout_entry(5, false),
                Self::create_buffer_layout_entry(6, false),
                Self::create_buffer_layout_entry(7, false),
            ],
        });

        let compute_buffers = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: probe.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: delta_0.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: delta_n.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: delta_prime.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: step.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: orbit.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: r.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: dr.as_entire_binding(),
                },
            ],
            label: Some("Compute Bind Group"),
        });

        let params_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Compute Parameters Bind Group Layout"),
                entries: &[Self::create_uniform_layout_entry(0)],
            });

        let compute_parameters = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &params_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: compute_parameters.as_entire_binding(),
            }],
            label: Some("Compute Parameters Bind Group"),
        });

        // create the bind group for the texture

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
                label: Some("Render Texture Bind Group Layout"),
            });

        let render_texture = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &texture_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(texture_view),
            }],
            label: Some("Render Texture Bind Group"),
        });

        // create a bind group for the render buffers

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

        // create the parameters group

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

        let render_parameters_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &params_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: render_parameters.as_entire_binding(),
            }],
            label: None,
        });

        // create pipeline layouts

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
                render_parameters: render_parameters_group,
                render_texture,
            },
            compute_pipeline_layout,
            render_pipeline_layout,
        )
    }

    /// Creates a bind group layout entry at the given binding with the given read-only flag.
    /// This is meant to be used for buffers that are accessed by a compute shader.
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

    /// Creates a bind group layout entry at the given binding for a uniform buffer.
    /// This is meant to be used for uniforms.
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
