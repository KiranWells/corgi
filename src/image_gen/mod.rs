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
use gpu_setup::SharedState;
use image::ImageBuffer;
use little_exif::exif_tag::ExifTag;
use little_exif::metadata::Metadata;
use nanoserde::SerJson;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::time::Instant;
use tracing::debug;

use crate::types::{
    Algorithm, ColorParams, ComputeParams, Image, ImageDiff, MAX_GPU_GROUP_ITER, Message,
    RenderParams, StatusMessage,
};
use probe::probe;

pub use gpu_setup::GPUData;

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
        let shared = SharedState::new(wgpu.device.clone(), wgpu.queue.clone());

        WorkerState {
            preview_state: GPUData::init(&preview_settings.viewport, shared.clone(), "Preview"),
            output_state: GPUData::init(&output_settings.viewport, shared, "Output"),
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
                let start = Instant::now();
                time!(
                    "full render",
                    render_image(
                        &mut self.preview_state,
                        &mut self.probe_buffer,
                        &image,
                        self.preview_settings.as_ref(),
                        self.send.clone(),
                        self.cancelled.clone(), // TODO: should this be the same cancel??
                        self.ctx.clone(),
                    )
                );
                let _ = self.send.send(StatusMessage::NewPreviewViewport(
                    Instant::now() - start,
                    image.viewport.clone(),
                ));
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
                self.ctx.request_repaint();
                if let Some(data) = self.output_state.get_texture_data() {
                    let _ = self
                        .send
                        .send(StatusMessage::Progress("Saving image".into(), 0.0));
                    self.ctx.request_repaint();
                    let mut img = image::DynamicImage::ImageRgba8(
                        ImageBuffer::from_raw(
                            output_settings.viewport.width as u32,
                            output_settings.viewport.height as u32,
                            data,
                        )
                        .expect("image data to be properly formatted"),
                    );
                    img = image::DynamicImage::ImageRgb8(img.flipv().into_rgb8());
                    if let Err(err) = img.save(path.clone()) {
                        tracing::error!("Failed to save image: {err}");
                        let _ = self.send.send(StatusMessage::Progress(
                            format!("Failed to save image: {err}"),
                            0.0,
                        ));
                        self.ctx.request_repaint();
                    } else {
                        // add metadata
                        if is_metadata_supported(&path) {
                            let mut meta = Metadata::new();
                            meta.set_tag(ExifTag::ImageDescription(
                                self.output_settings.as_ref().unwrap().serialize_json(),
                            ));
                            meta.set_tag(ExifTag::Software("Corgi".into()));
                            if let Err(err) = meta.write_to_file(&path) {
                                tracing::error!("Failed to write metadata to file: {err:?}");
                            }
                        }
                        let _ = self
                            .send
                            .send(StatusMessage::Progress("Image save complete".into(), 100.0));
                        self.ctx.request_repaint();
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

pub fn is_metadata_supported(path: &Path) -> bool {
    matches!(path.extension(), Some(x) if x == "jpg" || x == "jpeg" || x == "png" || x == "webp")
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
    let diff = last_image
        .map(|img| image.comp(img))
        .unwrap_or(ImageDiff::full());

    // the actual image generation process
    // - resize the GPU data
    // - probe the point
    // - generate the delta grid
    // - run the compute shader
    // - run the image render

    if diff.resize {
        gpu_data.resize(&image.viewport);
    }

    if diff.reprobe {
        let _ = send.send(StatusMessage::Progress("Probing point".into(), 0.0));
        ctx.request_repaint();
        // probe the point
        *probed_data = time!(
            "Probing point",
            probe::<f32>(&image.probe_location, image.max_iter, image.viewport.zoom)
        );
    }

    if diff.recompute {
        let _ = send.send(StatusMessage::Progress(
            format!("Computing iteration 1 of {}", image.max_iter),
            0.0,
        ));
        ctx.request_repaint();
        time!(
            "Running compute shader",
            run_compute_step(probed_data, image, gpu_data, &send, cancelled, &ctx)
        );
    }

    // This holds the lock until the render finishes.
    // This is suboptimal, as it might freeze the render thread, but
    // the color step should always complete with a low-enough time budget to
    // avoid dropped frames.
    if diff.recolor {
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
    image: &Image,
    gpu_data: &GPUData,
    send: &mpsc::Sender<StatusMessage>,
    _cancelled: Arc<AtomicBool>,
    ctx: &egui::Context,
) {
    let GPUData {
        shared: SharedState { device, queue, .. },
        bind_groups,
        direct_f32_pipeline,
        perturbed_f32_pipeline,
        buffers,
        ..
    } = gpu_data;
    let texture_size: Extent3d = (&image.viewport).into();

    let (compute_pipeline, x, y, probe_len) = match image.algorithm() {
        crate::types::Algorithm::Directf32 => (
            direct_f32_pipeline,
            image.viewport.x.to_f32(),
            image.viewport.y.to_f32(),
            image.max_iter as usize,
        ),
        crate::types::Algorithm::Perturbedf32 => (
            perturbed_f32_pipeline,
            (image.viewport.x.clone() - image.probe_location.x.clone()).to_f32(),
            (image.viewport.y.clone() - image.probe_location.y.clone()).to_f32(),
            probed_data.0.len(),
        ),
    };

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
            x,
            y,
            zoom: image.viewport.zoom as f32,
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
        if image.algorithm() != Algorithm::Directf32 {
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
        }

        // submit the compute shader command buffer
        let si = queue.submit(Some(command_buffer));
        // This slows down render times, so we avoid it in release
        #[cfg(debug_assertions)]
        time!("wait",
            let _ = device.poll(wgpu::MaintainBase::WaitForSubmissionIndex(si));
        );
        #[cfg(not(debug_assertions))]
        let _ = si;
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
}

/// Runs the render shader on the GPU
fn run_render_step(image: &Image, gpu_data: &GPUData) {
    let GPUData {
        shared: SharedState { device, queue, .. },
        bind_groups,
        buffers,
        color_pipeline,
        ..
    } = gpu_data;
    let color_params: RenderParams = image.into();
    queue.write_buffer(
        &buffers.external_coloring,
        0,
        bytemuck::cast_slice(&[ColorParams::from(&image.external_coloring)]),
    );
    queue.write_buffer(
        &buffers.internal_coloring,
        0,
        bytemuck::cast_slice(&[ColorParams::from(&image.internal_coloring)]),
    );
    queue.write_buffer(
        &buffers.render_parameters,
        0,
        bytemuck::cast_slice(&[color_params]),
    );
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
        cpass.set_pipeline(color_pipeline);
        cpass.dispatch_workgroups(
            (image.viewport.width as f64 / 16.0).ceil() as u32,
            (image.viewport.height as f64 / 16.0).ceil() as u32,
            1,
        );
    }

    // submit the render command queue
    let si = queue.submit(Some(encoder.finish()));
    let _ = device.poll(wgpu::MaintainBase::WaitForSubmissionIndex(si));
}
