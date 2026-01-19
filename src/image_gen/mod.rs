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

use eframe::egui::mutex::RwLock;
use eframe::wgpu::{self, Extent3d};
pub use gpu_setup::{Constants, GPUData, SharedState, get_device_and_queue};
use image::ImageBuffer;
use little_exif::exif_tag::ExifTag;
use little_exif::metadata::Metadata;
use probe::probe;

use crate::types::serde::{SafeSaveLoad, is_metadata_supported};
use crate::types::{
    ColorParams, ComputeParams, ImageDiff, ImageTimings, ImgSpec, ProgressUpdate, RenderParams,
};

#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum RenderingError {
    #[error("Rendering was cancelled")]
    Cancelled,
}
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum SaveError {
    #[error("There is no rendered image")]
    NoImage,
    #[error("Failed to save the image data to a file")]
    Save(#[from] image::ImageError),
    #[error("Failed to serialize image spec")]
    Serialization(#[from] crate::types::serde::SaveLoadError),
    #[error("Metadata not supported for image format: {0:?}")]
    MetadataNotSupported(Option<std::ffi::OsString>),
    #[error("Failed to save metadata to image")]
    MetadataSave(#[from] std::io::Error),
}

pub struct Engine {
    // spec for cached data
    last_image: Option<ImgSpec>,
    // cache validity
    cache_validity: CacheValidity,
    // gpu data
    gpu_data: GPUData,
    // cache data
    probed_data: Vec<[f32; 2]>,
    // communication?
    cancelled: Arc<AtomicBool>,
    // engine settings
    constants: Constants,
}

#[derive(Clone, Copy, Default, Debug)]
struct CacheValidity {
    gpu_data: bool,
    probe: bool,
    gpu_probe: bool,
    compute: bool,
    color: bool,
}

impl CacheValidity {
    fn update(&mut self, diff: ImageDiff) {
        let ImageDiff {
            rebuild,
            reprobe,
            recompute,
            recolor,
        } = diff;
        if rebuild {
            *self = Self::default();
        }
        if reprobe {
            self.probe = false;
            self.gpu_probe = false;
            self.compute = false;
            self.color = false;
        }
        if recompute {
            self.compute = false;
            self.color = false;
        }
        if recolor {
            self.color = false;
        }
    }
}

impl Engine {
    pub fn init(
        shared: SharedState,
        label: &str,
        size: wgpu::Extent3d,
        max_iter: usize,
        constants: Constants,
        cancelled: Arc<AtomicBool>,
    ) -> Self {
        Self {
            last_image: None,
            cache_validity: CacheValidity::default(),
            gpu_data: GPUData::init(size, max_iter, shared, label),
            probed_data: vec![],
            cancelled,
            constants,
        }
    }

    pub fn render_image(
        &mut self,
        image: &ImgSpec,
        status_callback: &mut impl FnMut(ProgressUpdate),
    ) -> Result<ImageTimings, RenderingError> {
        let diff = self
            .last_image
            .as_ref()
            .map(|img| image.comp(img))
            .unwrap_or(ImageDiff::full());
        let mut timings = ImageTimings::default();
        self.cache_validity.update(diff);
        self.last_image = Some(image.clone());

        // the actual image generation process
        // - resize the GPU data
        // - probe the point
        // - generate the delta grid
        // - run the compute shader
        // - run the image render

        if !self.cache_validity.gpu_data {
            let start = Instant::now();
            self.rebuild(image, status_callback);
            timings.build = Instant::now() - start;
        }
        if self.is_cancelled(status_callback) {
            return Err(RenderingError::Cancelled);
        }

        if !self.cache_validity.probe || !self.cache_validity.gpu_probe {
            let start = Instant::now();
            if !self.cache_validity.probe {
                self.reprobe(image, status_callback);
            }
            if !self.cache_validity.gpu_probe {
                self.reupload(status_callback);
            }
            timings.probe = Instant::now() - start;
        }
        if self.is_cancelled(status_callback) {
            return Err(RenderingError::Cancelled);
        }

        if !self.cache_validity.compute {
            let start = Instant::now();
            self.recompute(image, status_callback)?;
            timings.compute = Instant::now() - start;
        }

        // This holds the lock until the render finishes.
        // This is suboptimal, as it might freeze the render thread, but
        // the color step should always complete with a low-enough time budget to
        // avoid dropped frames.
        if !self.cache_validity.color {
            let start = Instant::now();
            self.recolor(image, status_callback);
            timings.color = Instant::now() - start;
        }
        Ok(timings)
    }

    pub fn save_to_file(
        &self,
        path: &Path,
        add_metadata: bool,
        status_callback: &mut impl FnMut(ProgressUpdate),
    ) -> Result<(), SaveError> {
        let Some(image_settings) = &self.last_image else {
            return Err(SaveError::NoImage);
        };
        if !self.cache_validity.color {
            return Err(SaveError::NoImage);
        }
        status_callback(ProgressUpdate::msg("Fetching image data"));
        if let Some(data) = self.gpu_data.get_texture_data() {
            status_callback(ProgressUpdate::msg("Saving image"));
            let mut img = image::DynamicImage::ImageRgba8(
                ImageBuffer::from_raw(image_settings.width, image_settings.height, data)
                    .expect("image data to be properly formatted"),
            );
            img = image::DynamicImage::ImageRgb8(img.flipv().into_rgb8());
            img.save(path)?;

            // add metadata
            if add_metadata {
                status_callback(ProgressUpdate::msg("Updating metadata"));
                if !is_metadata_supported(path) {
                    return Err(SaveError::MetadataNotSupported(
                        path.extension().map(|os| os.to_os_string()),
                    ));
                }
                let mut meta = Metadata::new();
                let description = image_settings.stringify()?;

                meta.set_tag(ExifTag::ImageDescription(description));
                meta.set_tag(ExifTag::Software("Corgi".into()));
                meta.write_to_file(path)?;
            }
        }
        Ok(())
    }

    pub fn texture(&self) -> Arc<RwLock<wgpu::Texture>> {
        self.gpu_data.texture.clone()
    }

    pub fn update_constants(&mut self, c: Constants) {
        self.constants = c;
    }
}

impl Engine {
    fn rebuild(&mut self, image: &ImgSpec, status_callback: &mut impl FnMut(ProgressUpdate)) {
        status_callback(ProgressUpdate::msg("Rebuilding GPU Buffers"));
        self.gpu_data.resize(
            (image.width, image.height),
            image.location.max_iter as usize,
            image.get_flags(),
        );
        self.cache_validity.gpu_data = true;
    }

    fn reprobe(&mut self, image: &ImgSpec, status_callback: &mut impl FnMut(ProgressUpdate)) {
        let julia_point = match &image.location.fractal_kind {
            crate::types::FractalKind::Mandelbrot => None,
            crate::types::FractalKind::Julia(pt) => Some(pt),
        };
        // probe the point
        self.probed_data = probe::<f32>(
            &image.location.probe_location,
            image.location.max_iter,
            image.location.zoom,
            julia_point,
            status_callback,
        );
        self.cache_validity.probe = true;
    }

    fn reupload(&mut self, status_callback: &mut impl FnMut(ProgressUpdate)) {
        status_callback(ProgressUpdate::msg("Uploading probe"));
        // update the probe buffer
        self.gpu_data.shared.queue.write_buffer(
            &self.gpu_data.buffers.probe,
            0,
            bytemuck::cast_slice(&self.probed_data[..]),
        );
        self.gpu_data.shared.queue.submit([]);
        let _ = self
            .gpu_data
            .shared
            .device
            .poll(wgpu::PollType::wait_indefinitely());
        self.cache_validity.gpu_probe = true;
    }

    fn recompute(
        &mut self,
        image: &ImgSpec,
        status_callback: &mut impl FnMut(ProgressUpdate),
    ) -> Result<(), RenderingError> {
        status_callback(ProgressUpdate::partial("Computing iterations", 0.0));
        run_compute_step(
            &self.gpu_data,
            &self.probed_data,
            image,
            self.constants,
            self.cancelled.clone(),
            status_callback,
        )?;
        self.cache_validity.compute = true;
        Ok(())
    }

    fn recolor(&mut self, image: &ImgSpec, status_callback: &mut impl FnMut(ProgressUpdate)) {
        status_callback(ProgressUpdate::msg("Rendering Colors"));
        run_render_step(&self.gpu_data, image);
        self.cache_validity.color = true;
    }

    fn is_cancelled(&mut self, status_callback: &mut impl FnMut(ProgressUpdate)) -> bool {
        if self.cancelled.load(std::sync::atomic::Ordering::Relaxed) {
            status_callback(ProgressUpdate::partial("Cancelled", 1.0));
            true
        } else {
            false
        }
    }
}

/// Runs the compute shader on the GPU. This is the most expensive step, so the output
/// should be cached as much as possible. This step only needs to be run if the probe
/// location, max iteration, or image viewport has changed.
fn run_compute_step(
    gpu_data: &GPUData,
    probed_data: &[[f32; 2]],
    image: &ImgSpec,
    constants: Constants,
    cancelled: Arc<AtomicBool>,
    status_callback: &mut impl FnMut(ProgressUpdate),
) -> Result<(), RenderingError> {
    let GPUData {
        shared: SharedState { device, queue, .. },
        bind_groups,
        direct_f32_pipeline,
        perturbed_f32_pipeline,
        buffers,
        ..
    } = gpu_data;
    let texture_size: Extent3d = image.extents();

    let (compute_pipeline, pt, probe_len) = match image.algorithm() {
        crate::types::Algorithm::Directf32 => (
            direct_f32_pipeline,
            image.location.center.to_vec2(),
            image.location.max_iter as usize,
        ),
        crate::types::Algorithm::Perturbedf32 => {
            let offset = image
                .view()
                .complex_to_px_delta(&image.location.probe_location);
            (
                perturbed_f32_pipeline,
                offset / image.size(),
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
            crate::types::FractalKind::Mandelbrot => eframe::egui::Vec2::new(0.0, 0.0),
            crate::types::FractalKind::Julia(pt) => pt.to_vec2(),
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
            x: pt.x,
            y: pt.y,
            zoom: image.location.zoom,
            angle: image.location.angle,
            julia_x: julia_point.x,
            julia_y: julia_point.y,
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
        std::thread::yield_now();
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
        status_callback(ProgressUpdate::partial(
            "Computing iterations",
            (i * constants.iter_batch_size + parameters.chunk_max_iter) as f64
                / image.location.max_iter as f64,
        ));
        if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
            status_callback(ProgressUpdate::partial("Cancelled", 1.0));
            return Err(RenderingError::Cancelled);
        }
    }
    Ok(())
}

/// Runs the render shader on the GPU
fn run_render_step(gpu_data: &GPUData, image: &ImgSpec) {
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
