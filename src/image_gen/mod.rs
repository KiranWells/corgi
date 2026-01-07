/*!
# Image Generation

This module contains all logic for creating images of the mandelbrot set to display on screen.

The [`render_thread`] function is the main entry point for the image generation process.
It is responsible for receiving messages from the main thread, and sending the resulting
images back to the main thread.
 */

mod gpu_setup;
mod probe;

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use eframe::wgpu::{self, Extent3d};
pub use gpu_setup::{Constants, GPUData, SharedState, get_device_and_queue};
use image::ImageBuffer;
use little_exif::exif_tag::ExifTag;
use little_exif::metadata::Metadata;
use probe::probe;

use crate::types::serde::SafeSaveLoad;
use crate::types::{
    ColorParams, ComputeParams, ImageDiff, ImageTimings, ImgSpec, RenderParams, RenderResult,
    StatusMessage,
};

pub fn is_metadata_supported(path: &Path) -> bool {
    matches!(path.extension(), Some(x) if x == "jpg" || x == "jpeg" || x == "png" || x == "webp" || x == "avif")
}

#[must_use]
pub fn render_image(
    gpu_data: &mut GPUData,
    probed_data: &mut Vec<[f32; 2]>,
    image: &ImgSpec,
    last_image: Option<&ImgSpec>,
    cancelled: Arc<AtomicBool>,
    mut status_callback: impl FnMut(StatusMessage),
) -> RenderResult {
    let diff = last_image
        .map(|img| image.comp(img))
        .unwrap_or(ImageDiff::full());
    let mut timings = ImageTimings::default();

    // the actual image generation process
    // - resize the GPU data
    // - probe the point
    // - generate the delta grid
    // - run the compute shader
    // - run the image render

    if diff.rebuild {
        let start = Instant::now();
        gpu_data.resize(
            (image.width, image.height),
            image.location.max_iter as usize,
            image.get_flags(),
        );
        timings.build = Instant::now() - start;
    }
    if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
        status_callback(StatusMessage::Progress("Cancelled".into(), 1.0));
        return RenderResult::Unfinished;
    }

    if diff.reprobe {
        let start = Instant::now();
        status_callback(StatusMessage::Progress("Probing point".into(), 0.0));
        let julia_point = match &image.location.fractal_kind {
            crate::types::FractalKind::Mandelbrot => None,
            crate::types::FractalKind::Julia(pt) => Some(pt),
        };
        // probe the point
        *probed_data = probe::<f32>(
            &image.location.probe_location,
            image.location.max_iter,
            image.location.zoom,
            julia_point,
        );
        status_callback(StatusMessage::Progress("Uploading probe".into(), 0.0));
        // update the probe buffer
        gpu_data.shared.queue.write_buffer(
            &gpu_data.buffers.probe,
            0,
            bytemuck::cast_slice(&probed_data[..]),
        );
        gpu_data.shared.queue.submit([]);
        let _ = gpu_data
            .shared
            .device
            .poll(wgpu::PollType::wait_indefinitely());
        timings.probe = Instant::now() - start;
    }
    if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
        status_callback(StatusMessage::Progress("Cancelled".into(), 1.0));
        return RenderResult::Unfinished;
    }

    if diff.recompute {
        let start = Instant::now();
        status_callback(StatusMessage::Progress(
            format!("Computing iteration 1 of {}", image.location.max_iter),
            0.0,
        ));
        if !run_compute_step(
            probed_data,
            image,
            gpu_data,
            cancelled,
            &mut status_callback,
        ) {
            return RenderResult::Unfinished;
        }
        timings.compute = Instant::now() - start;
    }

    // This holds the lock until the render finishes.
    // This is suboptimal, as it might freeze the render thread, but
    // the color step should always complete with a low-enough time budget to
    // avoid dropped frames.
    if diff.recolor {
        let start = Instant::now();
        status_callback(StatusMessage::Progress("Rendering Colors".into(), 0.0));
        run_render_step(image, gpu_data);
        timings.color = Instant::now() - start;
    }
    RenderResult::Finished(timings)
}

/// Runs the compute shader on the GPU. This is the most expensive step, so the output
/// should be cached as much as possible. This step only needs to be run if the probe
/// location, max iteration, or image viewport has changed.
#[must_use]
fn run_compute_step(
    probed_data: &[[f32; 2]],
    image: &ImgSpec,
    gpu_data: &GPUData,
    cancelled: Arc<AtomicBool>,
    status_callback: &mut impl FnMut(StatusMessage),
) -> bool {
    let GPUData {
        shared: SharedState { device, queue, .. },
        bind_groups,
        direct_f32_pipeline,
        perturbed_f32_pipeline,
        buffers,
        constants,
        ..
    } = gpu_data;
    let texture_size: Extent3d = image.extents();

    let (compute_pipeline, x, y, probe_len) = match image.algorithm() {
        crate::types::Algorithm::Directf32 => (
            direct_f32_pipeline,
            image.location.center.x.to_f32(),
            image.location.center.y.to_f32(),
            image.location.max_iter as usize,
        ),
        crate::types::Algorithm::Perturbedf32 => {
            let (x, y) = image
                .view()
                .coords_to_px_offset(&image.location.probe_location);
            (
                perturbed_f32_pipeline,
                x as f32 / image.width as f32,
                y as f32 / image.height as f32,
                probed_data.len(),
            )
        }
    };

    // Compute passes have encountered timeouts on some GPUs, so we split the compute passes into
    // multiple smaller passes.
    for i in 0..=(image.location.max_iter / constants.iter_batch_size) {
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
        let julia_point = match &image.location.fractal_kind {
            crate::types::FractalKind::Mandelbrot => (0.0, 0.0),
            crate::types::FractalKind::Julia(pt) => (pt.x.to_f32(), pt.y.to_f32()),
        };
        // Update the parameters
        let parameters = ComputeParams {
            width: texture_size.width,
            height: texture_size.height,
            max_iter: image.location.max_iter,
            chunk_max_iter: if (i + 1) * constants.iter_batch_size > image.location.max_iter {
                image.location.max_iter % constants.iter_batch_size
            } else {
                constants.iter_batch_size
            },
            probe_len: probe_len as u32,
            iter_offset: i * constants.iter_batch_size,
            x,
            y,
            zoom: image.location.zoom,
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
        {
            let start = Instant::now();
            let _ = device.poll(wgpu::PollType::Wait {
                submission_index: Some(si),
                timeout: Some(Duration::from_secs(1)),
            });
            tracing::trace!("Compute step batch took {:?}", Instant::now() - start);
        }
        #[cfg(not(debug_assertions))]
        let _ = si;
        status_callback(StatusMessage::Progress(
            format!(
                "Computing iteration {} of {}",
                i * constants.iter_batch_size + parameters.chunk_max_iter,
                image.location.max_iter
            ),
            (i * constants.iter_batch_size + parameters.chunk_max_iter) as f64
                / image.location.max_iter as f64,
        ));
        if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
            status_callback(StatusMessage::Progress("Cancelled".into(), 1.0));
            return false;
        }
    }
    true
}

/// Runs the render shader on the GPU
fn run_render_step(image: &ImgSpec, gpu_data: &GPUData) {
    let GPUData {
        shared: SharedState { device, queue, .. },
        bind_groups,
        buffers,
        color_pipeline,
        ..
    } = gpu_data;
    let color_params: RenderParams = image.into();
    let (_, mut external_colors) = image.style.external_coloring.gradient.decompose();
    let (_, internal_colors) = image.style.internal_coloring.gradient.decompose();
    external_colors.extend(internal_colors);
    queue.write_buffer(
        &buffers.external_coloring,
        0,
        bytemuck::cast_slice(&[ColorParams::from(&image.style.external_coloring)]),
    );
    queue.write_buffer(
        &buffers.internal_coloring,
        0,
        bytemuck::cast_slice(&[ColorParams::from(&image.style.internal_coloring)]),
    );
    queue.write_buffer(
        &buffers.render_parameters,
        0,
        bytemuck::cast_slice(&[color_params]),
    );
    queue.write_buffer(
        &buffers.gradient,
        0,
        bytemuck::cast_slice(external_colors.as_slice()),
    );
    // create encoder for CPU - GPU communication
    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

    let texture_size: Extent3d = image.extents();
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
    image_settings: &ImgSpec,
    path: &Path,
    mut status_callback: impl FnMut(StatusMessage),
) {
    status_callback(StatusMessage::Progress("Fetching image data".into(), 0.0));
    if let Some(data) = gpu_data.get_texture_data() {
        status_callback(StatusMessage::Progress("Saving image".into(), 0.0));
        let mut img = image::DynamicImage::ImageRgba8(
            ImageBuffer::from_raw(image_settings.width, image_settings.height, data)
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
                let serialized = image_settings.stringify();
                match serialized {
                    Err(err) => {
                        tracing::error!("Failed to save image: {err}");
                        status_callback(StatusMessage::Progress(
                            format!("Failed to save image: {err}"),
                            0.0,
                        ));
                    }
                    Ok(description) => {
                        meta.set_tag(ExifTag::ImageDescription(description));
                        meta.set_tag(ExifTag::Software("Corgi".into()));
                        if let Err(err) = meta.write_to_file(path) {
                            tracing::error!("Failed to write metadata to file: {err:?}");
                        }
                    }
                }
            }
            status_callback(StatusMessage::Progress("Image save complete".into(), 1.0));
        }
    }
}
