/*!
# Image Generation

This module contains all logic for creating images of the mandelbrot set to display on screen.

The [`render_thread`] function is the main entry point for the image generation process.
It is responsible for receiving messages from the main thread, and sending the resulting
images back to the main thread.
 */

mod gpu_setup;
mod probe;

use eframe::wgpu::{self, Extent3d};
use image::ImageBuffer;
use little_exif::{exif_tag::ExifTag, metadata::Metadata};
use nanoserde::SerJson;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::{path::Path, time::Duration};
use tracing::debug;

use crate::types::{ColorParams, ComputeParams, Image, ImageDiff, RenderParams, StatusMessage};
use probe::probe;

pub use gpu_setup::{Constants, GPUData, SharedState, get_device_and_queue};

macro_rules! time {
    ($name:literal; $($expression:tt)*) => {{
        let start = std::time::Instant::now();
        let result = { $($expression)* };
        let elapsed = start.elapsed();
        debug!("{} done in {:?}", $name, elapsed);
        result
    }};
}

pub fn is_metadata_supported(path: &Path) -> bool {
    matches!(path.extension(), Some(x) if x == "jpg" || x == "jpeg" || x == "png" || x == "webp")
}

pub fn render_image(
    gpu_data: &mut GPUData,
    probed_data: &mut Vec<[f32; 2]>,
    image: &Image,
    last_image: Option<&Image>,
    cancelled: Arc<AtomicBool>,
    mut status_callback: impl FnMut(StatusMessage),
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
        gpu_data.resize(&image.viewport, image.max_iter as usize, image.get_flags());
    }

    if diff.reprobe {
        status_callback(StatusMessage::Progress("Probing point".into(), 0.0));
        let julia_point = match &image.fractal_kind {
            crate::types::FractalKind::Mandelbrot => None,
            crate::types::FractalKind::Julia(pt) => Some(pt),
        };
        // probe the point
        *probed_data = time!(
            "Probing point";
            probe::<f32>(&image.probe_location, image.max_iter, image.viewport.zoom, julia_point)
        );
        status_callback(StatusMessage::Progress("Uploading probe".into(), 0.0));
        // update the probe buffer
        time!("probe upload";
            gpu_data.shared.queue.write_buffer(
                &gpu_data.buffers.probe,
                0,
                bytemuck::cast_slice(&probed_data[..]),
            );
            gpu_data.shared.queue.submit([]);
            let _ = gpu_data.shared.device.poll(wgpu::PollType::wait_indefinitely());
        );
    }

    if diff.recompute {
        status_callback(StatusMessage::Progress(
            format!("Computing iteration 1 of {}", image.max_iter),
            0.0,
        ));
        time!(
            "Running compute shader";
            run_compute_step(probed_data, image, gpu_data, cancelled, &mut status_callback)
        );
    }

    // This holds the lock until the render finishes.
    // This is suboptimal, as it might freeze the render thread, but
    // the color step should always complete with a low-enough time budget to
    // avoid dropped frames.
    if diff.recolor {
        status_callback(StatusMessage::Progress("Rendering Colors".into(), 0.0));
        time!("Running image render"; run_render_step(image, gpu_data));
    }
}

/// Runs the compute shader on the GPU. This is the most expensive step, so the output
/// should be cached as much as possible. This step only needs to be run if the probe
/// location, max iteration, or image viewport has changed.
fn run_compute_step(
    probed_data: &[[f32; 2]],
    image: &Image,
    gpu_data: &GPUData,
    _cancelled: Arc<AtomicBool>,
    status_callback: &mut impl FnMut(StatusMessage),
) {
    let GPUData {
        shared: SharedState { device, queue, .. },
        bind_groups,
        direct_f32_pipeline,
        perturbed_f32_pipeline,
        buffers,
        constants,
        ..
    } = gpu_data;
    let texture_size: Extent3d = (&image.viewport).into();

    let (compute_pipeline, x, y, probe_len) = match image.algorithm() {
        crate::types::Algorithm::Directf32 => (
            direct_f32_pipeline,
            image.viewport.center.x.to_f32(),
            image.viewport.center.y.to_f32(),
            image.max_iter as usize,
        ),
        crate::types::Algorithm::Perturbedf32 => {
            let (x, y) = image
                .viewport
                .coords_to_px_offset(&image.probe_location.x, &image.probe_location.y);
            (
                perturbed_f32_pipeline,
                x as f32 / image.viewport.width as f32,
                y as f32 / image.viewport.height as f32,
                probed_data.len(),
            )
        }
    };

    // Compute passes have encountered timeouts on some GPUs, so we split the compute passes into
    // multiple smaller passes.
    for i in 0..=(image.max_iter / constants.iter_batch_size) {
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
        let julia_point = match &image.fractal_kind {
            crate::types::FractalKind::Mandelbrot => (0.0, 0.0),
            crate::types::FractalKind::Julia(pt) => (pt.x.to_f32(), pt.y.to_f32()),
        };
        // Update the parameters
        let parameters = ComputeParams {
            width: texture_size.width,
            height: texture_size.height,
            max_iter: image.max_iter as u32,
            chunk_max_iter: if (constants.iter_batch_size + 1) * i > image.max_iter {
                (image.max_iter % constants.iter_batch_size) as u32
            } else {
                constants.iter_batch_size as u32
            },
            probe_len: probe_len as u32,
            iter_offset: (i * constants.iter_batch_size) as u32,
            x,
            y,
            cx: image.probe_location.x.to_f32(),
            cy: image.probe_location.y.to_f32(),
            zoom: image.viewport.zoom as f32,
            julia_x: julia_point.0,
            julia_y: julia_point.1,
        };
        if parameters.chunk_max_iter == 0 {
            break;
        }
        queue.write_buffer(
            &buffers.compute_parameters,
            0,
            bytemuck::cast_slice(&[parameters]),
        );

        // submit the compute shader command buffer
        let si = queue.submit(Some(command_buffer));
        // This slows down render times, so we avoid it in release
        #[cfg(debug_assertions)]
        time!("Compute step batch";
            let _ = device.poll(wgpu::PollType::Wait { submission_index: Some(si), timeout: Some(Duration::from_secs(1)) });
        );
        #[cfg(not(debug_assertions))]
        let _ = si;
        status_callback(StatusMessage::Progress(
            format!(
                "Computing iteration {} of {}",
                i * constants.iter_batch_size + parameters.chunk_max_iter as u64,
                image.max_iter
            ),
            (i * constants.iter_batch_size + parameters.chunk_max_iter as u64) as f64
                / image.max_iter as f64,
        ));
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

    let texture_size: Extent3d = (&image.viewport).into();
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
            (texture_size.width as f64 / 16.0).ceil() as u32,
            (texture_size.height as f64 / 16.0).ceil() as u32,
            1,
        );
    }

    // submit the render command queue
    let si = queue.submit(Some(encoder.finish()));
    let _ = device.poll(wgpu::PollType::Wait {
        submission_index: Some(si),
        timeout: Some(Duration::from_secs(1)),
    });
}

pub fn save_to_file(
    gpu_data: &GPUData,
    image_settings: &Image,
    path: &Path,
    mut status_callback: impl FnMut(StatusMessage),
) {
    status_callback(StatusMessage::Progress("Fetching image data".into(), 0.0));
    if let Some(data) = gpu_data.get_texture_data() {
        status_callback(StatusMessage::Progress("Saving image".into(), 0.0));
        let mut img = image::DynamicImage::ImageRgba8(
            ImageBuffer::from_raw(
                image_settings.viewport.width as u32,
                image_settings.viewport.height as u32,
                data,
            )
            .expect("image data to be properly formatted"),
        );
        img = image::DynamicImage::ImageRgb8(img.flipv().into_rgb8());
        if let Err(err) = img.save(path) {
            tracing::error!("Failed to save image: {err}");
            status_callback(StatusMessage::Progress(
                format!("Failed to save image: {err}"),
                0.0,
            ));
        } else {
            // add metadata
            if is_metadata_supported(path) {
                let mut meta = Metadata::new();
                meta.set_tag(ExifTag::ImageDescription(image_settings.serialize_json()));
                meta.set_tag(ExifTag::Software("Corgi".into()));
                if let Err(err) = meta.write_to_file(path) {
                    tracing::error!("Failed to write metadata to file: {err:?}");
                }
            }
            status_callback(StatusMessage::Progress("Image save complete".into(), 1.0));
        }
    }
}
