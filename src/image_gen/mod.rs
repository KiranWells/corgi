/*!
# Image Generation

This module contains all logic for creating images of the mandelbrot set to display on screen.

The [`render_thread`] function is the main entry point for the image generation process.
It is responsible for receiving messages from the main thread, and sending the resulting
images back to the main thread.
 */

mod gpu_setup;
mod probe;

use eframe::egui::mutex::RwLock;
use eframe::wgpu::{self, Extent3d};
use eframe::{egui, egui_wgpu};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use tracing::debug;
use wgpu::PollType;

use crate::types::{
    ComputeParams, Image, MAX_GPU_GROUP_ITER, Message, RenderParams, StatusMessage, Viewport,
};
use probe::probe;

pub use gpu_setup::GPUData;
use probe::generate_delta_grid;

// #[cfg(debug_assertions)]
macro_rules! time {
    ($name:literal, $($expression:tt)*) => {{
        let start = std::time::Instant::now();
        let result = { $($expression)* };
        let elapsed = start.elapsed();
        debug!("{} done in {:?}", $name, elapsed);
        result
    }};
}

// #[cfg(not(debug_assertions))]
// macro_rules! time {
//     ($name:literal, $expression:expr) => {{
//         $expression
//     }};
// }

pub struct WorkerState {
    preview_state: GPUData,
    output_state: GPUData,
    probe_buffer: (Vec<[f32; 2]>, Vec<[f32; 2]>),
    preview_settings: Option<Image>,
    output_settings: Option<Image>,
    recv: mpsc::Receiver<Message>,
    cancelled: Arc<AtomicBool>,
    send: mpsc::Sender<StatusMessage>,
    ctx: egui::Context,
}

impl WorkerState {
    pub fn new(
        wgpu: &egui_wgpu::RenderState,
        preview_settings: Image,
        output_settings: Image,
        recv: mpsc::Receiver<Message>,
        send: mpsc::Sender<StatusMessage>,
        cancelled: Arc<AtomicBool>,
        ctx: egui::Context,
    ) -> Self {
        let compute_shader = wgpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("calculate"),
                source: wgpu::ShaderSource::Wgsl(wesl::include_wesl!("calculate").into()),
            });

        WorkerState {
            preview_state: GPUData::init(
                &preview_settings.viewport,
                wgpu,
                compute_shader.clone(),
                "Preview",
            ),
            output_state: GPUData::init(&output_settings.viewport, wgpu, compute_shader, "Output"),
            probe_buffer: (vec![], vec![]),
            preview_settings: None,
            output_settings: None,
            recv,
            send,
            cancelled,
            ctx,
        }
    }

    /// Main entry point for the image generation process. This should be called in a separate thread,
    /// and will run until the given message channel is closed. `status` is used to communicate the
    /// current status of the render process to the main thread.
    pub fn run(&mut self) {
        while let Ok(msg) = self.recv.recv() {
            let mut new_preview = None;
            let mut new_output = None;
            let mut file_save = None;
            match msg {
                Message::NewPreviewSettings(image) => {
                    new_preview = Some(image);
                }
                Message::NewOutputSettings(image) => {
                    new_output = Some(image);
                }
                Message::SaveToFile(path) => {
                    file_save = Some(path);
                }
            }
            loop {
                let next = self.recv.try_recv();
                match next {
                    Ok(Message::NewPreviewSettings(image)) => {
                        new_preview = Some(image);
                    }
                    Ok(Message::NewOutputSettings(image)) => {
                        new_output = Some(image);
                    }
                    Ok(Message::SaveToFile(path)) => {
                        file_save = Some(path);
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => return,
                }
            }
            if let Some(image) = new_preview {
                render_image(
                    &mut self.preview_state,
                    &mut self.probe_buffer,
                    &image,
                    self.preview_settings.as_ref(),
                    self.send.clone(),
                    self.cancelled.clone(), // TODO: should this be the same cancel??
                    self.ctx.clone(),
                );
                let _ = self
                    .send
                    .send(StatusMessage::NewPreviewViewport(image.viewport.clone()));
                self.preview_settings = Some(image);
                self.ctx.request_repaint();
            }
            if let Some(image) = new_output {
                // TODO: move to separate thread
                render_image(
                    &mut self.output_state,
                    &mut self.probe_buffer,
                    &image,
                    self.output_settings.as_ref(),
                    self.send.clone(),
                    self.cancelled.clone(),
                    self.ctx.clone(),
                );
                self.output_settings = Some(image);
                // let _ = self.send.send(StatusMessage::New(image.viewport.clone()));
                let _ = self.send.send(StatusMessage::Progress(
                    "Finished rendering output".into(),
                    100.0,
                ));
                self.ctx.request_repaint();
            }
            if let Some(path) = file_save {
                // copy output buffer to CPU
                let output_settings = self.output_settings.as_ref().unwrap();
                let _ = self
                    .send
                    .send(StatusMessage::Progress("Fetching image data".into(), 0.0));
                if let Some(data) = self.output_state.get_texture_data() {
                    let _ = self
                        .send
                        .send(StatusMessage::Progress("Saving image".into(), 0.0));
                    if let Err(err) = image::save_buffer(
                        path,
                        data.as_slice(),
                        output_settings.viewport.width as u32,
                        output_settings.viewport.height as u32,
                        image::ColorType::Rgba8,
                    ) {
                        tracing::error!("Failed to save image: {err}");
                        let _ = self.send.send(StatusMessage::Progress(
                            format!("Failed to save image: {err}"),
                            0.0,
                        ));
                    } else {
                        let _ = self
                            .send
                            .send(StatusMessage::Progress("Image save complete".into(), 100.0));
                    }
                }
                // write to file
            }
        }
    }

    pub fn preview_texture(&self) -> Arc<RwLock<wgpu::Texture>> {
        self.preview_state.texture.clone()
    }
    pub fn output_texture(&self) -> Arc<RwLock<wgpu::Texture>> {
        self.output_state.texture.clone()
    }
}

fn render_image(
    gpu_data: &mut GPUData,
    probed_data: &mut (Vec<[f32; 2]>, Vec<[f32; 2]>),
    image: &Image,
    last_image: Option<&Image>,
    send: mpsc::Sender<StatusMessage>,
    cancelled: Arc<AtomicBool>,
    ctx: egui::Context,
) {
    let resize;
    let reprobe;
    let regenerate_delta;
    let recompute;
    let recolor;
    let mut delta_grid = vec![];
    if let Some(last_image) = last_image {
        // if the image has not changed, skip the render
        if image == last_image {
            return;
        }
        // if the viewport has changed, resize the GPU data
        resize = image.viewport.width != last_image.viewport.width
            || image.viewport.height != last_image.viewport.height;
        // if the max iteration or probe location has changed, re-run the probe
        reprobe = image.max_iter != last_image.max_iter
            || image.probe_location.x != last_image.probe_location.x
            || image.probe_location.y != last_image.probe_location.y;
        // if the probe location has changed or the image viewport has changed, re-generate the delta grid
        regenerate_delta = image.viewport != last_image.viewport || reprobe;
        // if the image generation parameters have changed, re-run the compute shader
        recompute = image.max_iter != last_image.max_iter || regenerate_delta || reprobe;
        // if the image coloring parameters have changed, re-run the image render
        recolor = image.coloring != last_image.coloring
            || recompute
            || image.misc != last_image.misc
            || image.debug_shutter != last_image.debug_shutter;
    } else {
        // if there is no last image, re-run everything
        resize = true;
        reprobe = true;
        regenerate_delta = true;
        recompute = true;
        recolor = true;
    }

    // the actual image generation process
    // - resize the GPU data
    // - probe the point
    // - generate the delta grid
    // - run the compute shader
    // - run the image render

    if resize {
        gpu_data.resize(&image.viewport);
    }

    if reprobe {
        let _ = send.send(StatusMessage::Progress("Probing point".into(), 0.0));
        ctx.request_repaint();
        // probe the point
        *probed_data = time!(
            "Probing point",
            probe::<f32>(&image.probe_location, image.max_iter, image.viewport.zoom)
        );
    }

    if regenerate_delta {
        let _ = send.send(StatusMessage::Progress("Generating Delta Grid".into(), 0.0));
        ctx.request_repaint();
        // generate the delta grid
        delta_grid = time!(
            "Generating delta grid",
            generate_delta_grid::<f32>(&image.probe_location, &image.viewport)
        );
    }

    if resize || regenerate_delta {
        let _ = send.send(StatusMessage::Progress("Updating buffers".into(), 0.0));
        ctx.request_repaint();
        // if the image has been resized, but the probe location has not changed, copy the delta grid to the GPU
        gpu_data.queue.write_buffer(
            &gpu_data.buffers.delta_0,
            0,
            bytemuck::cast_slice(&delta_grid),
        );
    }

    if recompute {
        let _ = send.send(StatusMessage::Progress(
            format!("Computing iteration 1 of {}", image.max_iter),
            0.0,
        ));
        ctx.request_repaint();
        time!(
            "Running compute shader",
            run_compute_step(
                probed_data,
                &image.viewport,
                gpu_data,
                &send,
                cancelled,
                &ctx
            )
        );
    }

    // This holds the lock until the render finishes.
    // This is suboptimal, as it might freeze the render thread, but
    // the color step should always complete with a low-enough time budget to
    // avoid dropped frames.
    if recolor {
        let _ = send.send(StatusMessage::Progress("Rendering Colors".into(), 0.0));
        ctx.request_repaint();
        time!("Running image render", run_render_step(image, gpu_data));
    }
}

/// Runs the compute shader on the GPU. This is the most expensive step, so the output
/// should be cached as much as possible. This step only needs to be run if the probe
/// location, max iteration, or image viewport has changed.
fn run_compute_step(
    probed_data: &(Vec<[f32; 2]>, Vec<[f32; 2]>),
    viewport: &Viewport,
    gpu_data: &GPUData,
    send: &mpsc::Sender<StatusMessage>,
    _cancelled: Arc<AtomicBool>,
    ctx: &egui::Context,
) {
    let GPUData {
        device,
        queue,
        bind_groups,
        compute_pipeline,
        buffers,
        ..
    } = gpu_data;
    let texture_size: Extent3d = viewport.into();
    let probe_len = probed_data.0.len();
    debug_assert!(probe_len == probed_data.1.len());

    // Compute passes have encountered timeouts on some GPUs, so we split the compute passes into
    // multiple smaller passes.
    for i in 0..=(probe_len / MAX_GPU_GROUP_ITER) {
        let parameters;
        time! {
            "Compute step batch",
        // Create encoder for CPU - GPU communication
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        // Begin compute dispatch
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });
            cpass.set_bind_group(0, &bind_groups.compute_buffers, &[]);
            cpass.set_bind_group(1, &bind_groups.compute_parameters, &[]);
            cpass.set_pipeline(compute_pipeline);
            cpass.dispatch_workgroups(
                (texture_size.width as f64 / 16.0).ceil() as u32,
                (texture_size.height as f64 / 16.0).ceil() as u32,
                1,
            );
        }

        let command_buffer = encoder.finish();
        // Update the parameters
        parameters = ComputeParams {
            width: texture_size.width,
            height: texture_size.height,
            max_iter: probe_len as u32,
            probe_len: if probe_len >= (i + 1) * MAX_GPU_GROUP_ITER {
                MAX_GPU_GROUP_ITER as u32
            } else {
                (probe_len % MAX_GPU_GROUP_ITER) as u32
            },
            iter_offset: (i * MAX_GPU_GROUP_ITER) as u32,
        };
        if parameters.probe_len == 0 {
            break;
        }
        queue.write_buffer(
            &buffers.compute_parameters,
            0,
            bytemuck::cast_slice(&[parameters]),
        );

        // update the probe buffer
        queue.write_buffer(
            &buffers.probe,
            0,
            bytemuck::cast_slice(
                &probed_data.0[i * MAX_GPU_GROUP_ITER
                    ..i * MAX_GPU_GROUP_ITER + parameters.probe_len as usize],
            ),
        );
        queue.write_buffer(
            &buffers.probe,
            MAX_GPU_GROUP_ITER as u64 * 8,
            bytemuck::cast_slice(
                &probed_data.1[i * MAX_GPU_GROUP_ITER
                    ..i * MAX_GPU_GROUP_ITER + parameters.probe_len as usize],
            ),
        );

        // submit the compute shader command buffer
        queue.submit(Some(command_buffer));
        };
        let _ = send.send(StatusMessage::Progress(
            format!(
                "Computing iteration {} of {}",
                i * MAX_GPU_GROUP_ITER + parameters.probe_len as usize,
                probe_len
            ),
            (i * MAX_GPU_GROUP_ITER + parameters.probe_len as usize) as f64 / probe_len as f64,
        ));
        ctx.request_repaint();
    }
    // ensure the queue is complete
    let _ = gpu_data.device.poll(PollType::Wait);
}

/// Runs the render shader on the GPU
fn run_render_step(image: &Image, gpu_data: &GPUData) {
    let GPUData {
        device,
        queue,
        bind_groups,
        buffers,
        render_pipeline_layout,
        ..
    } = gpu_data;
    let color_params: RenderParams = image.into();
    queue.write_buffer(
        &buffers.render_parameters,
        0,
        bytemuck::cast_slice(&[color_params]),
    );

    // select the render shader
    let render_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Render Shader"),
        source: wgpu::ShaderSource::Wgsl(wesl::include_wesl!("color").into()),
    });

    // create the render pipeline
    let render_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("Render Pipeline"),
        layout: Some(render_pipeline_layout),
        module: &render_shader,
        entry_point: Some("main_color"),
        compilation_options: wgpu::PipelineCompilationOptions {
            constants: &[],
            zero_initialize_workgroup_memory: false,
        },
        cache: None,
    });

    // create encoder for CPU - GPU communication
    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

    // begin render dispatch
    {
        let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: None,
            timestamp_writes: None,
        });
        cpass.set_bind_group(0, &bind_groups.render_buffers, &[]);
        cpass.set_bind_group(1, &bind_groups.render_texture, &[]);
        cpass.set_bind_group(2, &bind_groups.render_parameters, &[]);
        cpass.set_pipeline(&render_pipeline);
        cpass.dispatch_workgroups(
            (image.viewport.width as f64 / 16.0).ceil() as u32,
            (image.viewport.height as f64 / 16.0).ceil() as u32,
            1,
        );
    }

    // submit the render command queue
    queue.submit(Some(encoder.finish()));
}
