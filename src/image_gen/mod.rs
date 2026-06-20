/*!
# Image Generation

This module contains all logic for creating images of the mandelbrot set to display on screen.

The [`Engine`] struct manages rendering from [`ImgSpec`]s, with
the main entry point of [`Engine::render_image`]
 */

mod gpu_setup;
mod hpf_algorithm;
pub mod probe;
pub mod shader_types;

use std::fs::OpenOptions;
use std::io::BufWriter;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use documented::DocumentedFields;
pub use gpu_setup::{Constants, GPUData, SharedState, get_device_and_queue};
use image::ImageFormat;
use image::codecs::avif::AvifEncoder;
use image::codecs::gif::GifEncoder;
use image::codecs::jpeg::JpegEncoder;
use little_exif::exif_tag::ExifTag;
use little_exif::metadata::Metadata;
use parking_lot::RwLock;
use probe::probe;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use serde::{Deserialize, Serialize};
use wgpu::{self, Extent3d};

use crate::shared::algorithms::{direct_32, perturbed_32};
use crate::shared::coloring::main::{ColorParams, RenderParams};
use crate::shared::types::{BufferValues, ComputeParams};
use crate::shared::wgsl_primitives::{Vec2, Vec4};
use crate::types::serde::{SafeSaveLoad, is_metadata_supported};
use crate::types::{Algorithm, ImageDiff, ImgSpec};

#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum RenderingError {
    #[error("Rendering was cancelled")]
    Cancelled,
}

#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum SaveError {
    #[error(transparent)]
    GetImage(#[from] GetImageError),
    #[error("Failed to load data from GPU")]
    FailedLoad(#[from] wgpu::BufferAsyncError),
    #[error("Failed to save the image data to a file")]
    Save(#[from] image::ImageError),
    #[error("Failed to save the image data to an EXR file")]
    ExrSave(#[from] exr::error::Error),
    #[error("Failed to serialize image spec")]
    Serialization(#[from] crate::types::serde::SaveLoadError),
    #[error("Metadata not supported for image format: {0:?}")]
    MetadataNotSupported(Option<std::ffi::OsString>),
    #[error("Failed to save metadata to image")]
    MetadataSave(#[from] std::io::Error),
}

#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum CompressionParamsError {
    #[error("Speed must be within 1-100, inclusive")]
    InvalidSpeed,
    #[error("Quality must be within 1-100, inclusive")]
    InvalidQuality,
}

#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum GetImageError {
    #[error("There is no rendered image")]
    NoImage,
    #[error("Failed to load data from GPU")]
    FailedLoad(#[from] wgpu::BufferAsyncError),
    #[error("Data on the GPU could not be made into an image")]
    BadImageData,
}

/// Manages rendering fractal images.
///
/// Handles caching, status updates, and cancellation.
pub struct Engine {
    /// The [`ImgSpec`] used for the last rendering operation, even if it didn't complete.
    last_image: Option<ImgSpec>,
    /// Describes how many cached stages of the image rendering operation are still valid.
    cache_validity: CacheValidity,
    /// GPU data structures used for rendering.
    gpu_data: GPUData,
    /// The entire set of the "probe" point's iterations.
    probed_data: Vec<[f32; 2]>,
    /// Whether the current operation has been cancelled.
    cancelled: Arc<AtomicBool>,
    /// Parameters for configuring internal rendering behavior.
    constants: Constants,
}

/// Tracking struct for cache validation.
#[derive(Clone, Copy, Default, Debug)]
struct CacheValidity {
    gpu_data: bool,
    probe: bool,
    gpu_probe: bool,
    compute: bool,
    color: bool,
}

impl CacheValidity {
    /// Reset the cache validity based on the steps that need to be
    /// re-run to calculate a new image.
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

#[derive(Debug)]
pub struct ProgressUpdate {
    pub message: &'static str,
    /// A percentage of completion in \[0,1]
    pub progress: Option<f64>,
}

impl ProgressUpdate {
    /// Creates a progress update without a percentage
    pub fn msg(message: &'static str) -> Self {
        Self {
            message,
            progress: None,
        }
    }

    /// Creates a progress update with a given percentage in \[0,1]
    pub fn partial(message: &'static str, percent: f64) -> Self {
        Self {
            message,
            progress: Some(percent),
        }
    }
}

/// Tracking struct for execution times for each rendering step
#[derive(Debug, Default, Clone, Copy)]
pub struct ImageTimings {
    pub probe: Duration,
    pub compute: Duration,
    pub color: Duration,
    pub build: Duration,
}

impl ImageTimings {
    /// Estimate how long rendering an image will take based on the
    /// steps that need to be run and the tracked timings.
    pub fn estimate_time(&self, diff: ImageDiff) -> Duration {
        let mut estimated_time = Duration::ZERO;
        if diff.reprobe {
            estimated_time += self.probe;
        }
        if diff.recompute {
            estimated_time += self.compute;
        }
        if diff.recolor {
            estimated_time += self.color;
        }
        if diff.rebuild {
            estimated_time += self.build;
        }
        estimated_time
    }

    /// Update the tracked timings with new data.
    ///
    /// Currently uses a basic moving average.
    pub fn merge(&mut self, new_timings: &Self) {
        if !new_timings.probe.is_zero() {
            self.probe = (self.probe + new_timings.probe) / 2;
        }
        if !new_timings.compute.is_zero() {
            self.compute = (self.compute + new_timings.compute) / 2;
        }
        if !new_timings.color.is_zero() {
            self.color = (self.color + new_timings.color) / 2;
        }
        if !new_timings.build.is_zero() {
            self.build = (self.build + new_timings.build) / 2;
        }
    }
}

impl std::fmt::Display for ImageTimings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Total: {:?} ",
            self.build + self.probe + self.compute + self.color
        )?;
        if !self.build.is_zero() {
            write!(f, "Build: {:?} ", self.build)?;
        }
        if !self.probe.is_zero() {
            write!(f, "Probe: {:?} ", self.probe)?;
        }
        if !self.compute.is_zero() {
            write!(f, "Compute: {:?} ", self.compute)?;
        }
        if !self.color.is_zero() {
            write!(f, "Color: {:?}", self.color)?;
        }
        Ok(())
    }
}

/// Settings used for compression speed
/// and quality, if relevant for the given
/// image type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, DocumentedFields)]
pub struct CompressionParams {
    /// A value from 1-100. Lower gives higher compression but
    /// takes more time to compress.
    pub speed: u8,
    /// A value from 1-100. Higher values give better image
    /// quality but take up more space on disk. For AVIF, a
    /// value of 100 means lossless compression.
    pub quality: u8,
}

impl Default for CompressionParams {
    fn default() -> Self {
        Self {
            speed: 20,
            quality: 80,
        }
    }
}

impl CompressionParams {
    pub fn new(speed: u8, quality: u8) -> Result<Self, CompressionParamsError> {
        if !(1..=100).contains(&speed) {
            Err(CompressionParamsError::InvalidSpeed)
        } else if !(1..=100).contains(&quality) {
            Err(CompressionParamsError::InvalidQuality)
        } else {
            Ok(Self { speed, quality })
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

    /// Renders the given image, only running steps where the cached data is not
    /// valid.
    pub fn render_image(
        &mut self,
        image: &ImgSpec,
        status_callback: &mut impl FnMut(ProgressUpdate),
    ) -> Result<ImageTimings, RenderingError> {
        let mut timings = ImageTimings::default();
        self.cache_validity.update(self.diff(image));
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
                self.reupload(status_callback)?;
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

        if !self.cache_validity.color {
            let start = Instant::now();
            self.recolor(image, status_callback)?;
            timings.color = Instant::now() - start;
        }
        Ok(timings)
    }

    /// Save the current rendered image to a file. Returns [`SaveError::NoImage`]
    /// if the current rendered image is not valid, such as if the most recent render was cancelled
    /// or because [`Self::render_image`] has not been called yet.
    pub fn save_to_file(
        &self,
        path: &Path,
        name: Option<String>,
        compression_params: CompressionParams,
        add_metadata: bool,
        status_callback: &mut impl FnMut(ProgressUpdate),
    ) -> Result<(), SaveError> {
        status_callback(ProgressUpdate::msg("Fetching image data"));
        let img = self.get_image()?;
        status_callback(ProgressUpdate::msg("Saving image"));
        let lazy_writer = || -> Result<BufWriter<std::fs::File>, SaveError> {
            Ok(BufWriter::new(
                OpenOptions::new()
                    .write(true)
                    .truncate(true)
                    .create(true)
                    .open(path)?,
            ))
        };
        match ImageFormat::from_path(path)? {
            // 1 <= quality <= 100
            ImageFormat::Jpeg => img.write_with_encoder(JpegEncoder::new_with_quality(
                lazy_writer()?,
                compression_params.quality,
            ))?,
            // 1 <= speed <= 30
            ImageFormat::Gif => img.write_with_encoder(GifEncoder::new_with_speed(
                lazy_writer()?,
                ((compression_params.speed as u32 * 30) / 100).max(1) as i32,
            ))?,
            // 1 <= speed <= 10, 1 <= quality <= 100
            ImageFormat::Avif => img.write_with_encoder(AvifEncoder::new_with_speed_quality(
                lazy_writer()?,
                (compression_params.speed / 10).max(1),
                compression_params.quality,
            ))?,
            _ => img.save(path)?,
        }

        // add metadata
        if add_metadata {
            status_callback(ProgressUpdate::msg("Updating metadata"));
            if !is_metadata_supported(path) {
                return Err(SaveError::MetadataNotSupported(
                    path.extension().map(|os| os.to_os_string()),
                ));
            }
            let image_settings = self
                .last_image
                .as_ref()
                .expect("image to be available if get_image succeeded");
            let mut meta = Metadata::new();
            let description = image_settings.stringify()?;

            meta.set_tag(ExifTag::ImageDescription(description));
            if let Some(name) = name {
                // There is no "name" field, so thumbnails use this instead
                meta.set_tag(ExifTag::Make(name));
            }
            meta.set_tag(ExifTag::Software("Corgi".into()));
            meta.write_to_file(path)?;
        }
        Ok(())
    }

    pub fn get_image(&self) -> Result<image::DynamicImage, GetImageError> {
        let Some(image_settings) = &self.last_image else {
            return Err(GetImageError::NoImage);
        };
        if !self.cache_validity.color {
            return Err(GetImageError::NoImage);
        }
        let data = self.gpu_data.get_texture_data()?;

        let Some(img) =
            image::ImageBuffer::from_raw(image_settings.width, image_settings.height, data)
        else {
            return Err(GetImageError::BadImageData);
        };
        Ok(image::DynamicImage::ImageRgba8(img).flipv())
    }

    /// Update the cached probe data within this engine. Assumes the probe corresponds
    /// to the given image spec.
    ///
    /// This is only valuable if the next call to [`Self::render_image`] uses
    /// a very similar image spec, and this Engine's cached probe will not be valid.
    pub fn pre_cache_probe(&mut self, probed_data: Vec<[f32; 2]>, image: &ImgSpec) {
        let diff = self.diff(image);
        self.cache_validity.update(diff);
        self.last_image = Some(image.clone());
        if !self.cache_validity.gpu_data {
            self.rebuild(image, &mut |_| {});
        }
        if !self.cache_validity.probe {
            tracing::debug!("Successfully pre-cached probe");
            self.probed_data = probed_data;
            self.cache_validity.probe = true;
        }
    }

    /// Returns a handle to the texture this engine renders to
    pub fn texture(&self) -> Arc<RwLock<wgpu::Texture>> {
        self.gpu_data.texture.clone()
    }

    pub fn update_constants(&mut self, c: Constants) {
        self.constants = c;
    }

    /// Returns a reference to the internal probe cache, if it is valid for the given
    /// image spec.
    pub fn get_probe_cache(&self, image: &ImgSpec) -> Option<&[[f32; 2]]> {
        if let Some(last) = self.last_image.as_ref()
            && self.cache_validity.probe
            && image.location.center == last.location.center
            && image.location.max_iter == last.location.max_iter
        {
            Some(&self.probed_data)
        } else {
            None
        }
    }

    pub fn save_to_exr(
        &self,
        path: &Path,
        status_callback: &mut impl FnMut(ProgressUpdate),
    ) -> Result<(), SaveError> {
        use exr::prelude::*;
        let size = self.gpu_data.texture.read().size();
        let size = (size.width as usize, size.height as usize);
        // Color layer
        let color_data = self.gpu_data.get_texture_data()?;

        fn byte_to_linear_f16(byte: u8) -> f16 {
            f16::from_f32((((byte as f32) / 256.0 + 0.055) / 1.055).powf(2.4))
        }

        let color_channels = SpecificChannels::rgb(|loc: Vec2<usize>| {
            (
                byte_to_linear_f16(color_data[(loc.x() + loc.y() * size.0) * 4]),
                byte_to_linear_f16(color_data[(loc.x() + loc.y() * size.0) * 4 + 1]),
                byte_to_linear_f16(color_data[(loc.x() + loc.y() * size.0) * 4 + 2]),
            )
        });

        let color_layer = Layer::new(
            size,
            LayerAttributes {
                layer_name: Some("Final Color".into()),
                ..Default::default()
            },
            Encoding {
                compression: Compression::B44A,
                ..Default::default()
            },
            color_channels,
        );

        // Step layer
        let step_data = self.gpu_data.get_buffer_data(gpu_setup::BufferId::Step)?;

        let step_channels = SpecificChannels::build()
            .with_channel("x") // step
            .with_channel("y") // escaped
            .with_pixel_fn(|loc: Vec2<usize>| {
                let index = (loc.x() + loc.y() * size.0) * 4;
                let steps = i32::from_ne_bytes(step_data[index..index + 4].try_into().unwrap());
                (steps.unsigned_abs(), ((steps.signum() + 1) / 2) as u32)
            });

        let step_layer = Layer::new(
            size,
            LayerAttributes {
                layer_name: Some("Steps".into()),
                ..Default::default()
            },
            Encoding {
                compression: Compression::PXR24,
                ..Default::default()
            },
            step_channels,
        );

        // Z layer
        let z_data = self.gpu_data.get_buffer_data(gpu_setup::BufferId::Z)?;

        let z_channels = SpecificChannels::build()
            .with_channel("x") // real
            .with_channel("y") // imaginary
            .with_channel("z") // scale
            .with_pixel_fn(|loc: Vec2<usize>| {
                let index = (loc.x() + loc.y() * size.0) * 16;
                let x = f32::from_ne_bytes(z_data[index..index + 4].try_into().unwrap());
                let y = f32::from_ne_bytes(z_data[index + 4..index + 8].try_into().unwrap());
                let scale = f32::from_ne_bytes(z_data[index + 8..index + 12].try_into().unwrap());
                (x, y, scale)
            });

        let z_layer = Layer::new(
            size,
            LayerAttributes {
                layer_name: Some("Z".into()),
                ..Default::default()
            },
            Encoding {
                compression: Compression::PIZ,
                ..Default::default()
            },
            z_channels,
        );

        // dz layer
        let dz_data = self.gpu_data.get_buffer_data(gpu_setup::BufferId::Dz)?;

        let dz_channels = SpecificChannels::build()
            .with_channel("x") // real
            .with_channel("y") // imaginary
            .with_channel("z") // scale
            .with_pixel_fn(|loc: Vec2<usize>| {
                let index = (loc.x() + loc.y() * size.0) * 16;
                let x = f32::from_ne_bytes(dz_data[index..index + 4].try_into().unwrap());
                let y = f32::from_ne_bytes(dz_data[index + 4..index + 8].try_into().unwrap());
                let scale = f32::from_ne_bytes(dz_data[index + 8..index + 12].try_into().unwrap());
                (x, y, scale)
            });

        let dz_layer = Layer::new(
            size,
            LayerAttributes {
                layer_name: Some("dZ".into()),
                ..Default::default()
            },
            Encoding {
                compression: Compression::PIZ,
                ..Default::default()
            },
            dz_channels,
        );

        // orbit layer
        let orbit_data = self.gpu_data.get_buffer_data(gpu_setup::BufferId::Orbit)?;

        let orbit_channels = SpecificChannels::build()
            .with_channel("x")
            .with_channel("y")
            .with_channel("z")
            .with_channel("w")
            .with_pixel_fn(|loc: Vec2<usize>| {
                let index = (loc.x() + loc.y() * size.0) * 16;
                let x = f32::from_ne_bytes(orbit_data[index..index + 4].try_into().unwrap());
                let y = f32::from_ne_bytes(orbit_data[index + 4..index + 8].try_into().unwrap());
                let z = f32::from_ne_bytes(orbit_data[index + 8..index + 12].try_into().unwrap());
                let w = f32::from_ne_bytes(orbit_data[index + 12..index + 16].try_into().unwrap());
                (x, y, z, w)
            });

        let orbit_layer = Layer::new(
            size,
            LayerAttributes {
                layer_name: Some("Orbit".into()),
                ..Default::default()
            },
            Encoding {
                compression: Compression::PIZ,
                ..Default::default()
            },
            orbit_channels,
        );

        // stripe layer
        let stripe_data = self.gpu_data.get_buffer_data(gpu_setup::BufferId::Stripe)?;

        let stripe_channels = SpecificChannels::build()
            .with_channel("x")
            .with_channel("y")
            .with_channel("z")
            .with_channel("w")
            .with_pixel_fn(|loc: Vec2<usize>| {
                let index = (loc.x() + loc.y() * size.0) * 16;
                let x = f32::from_ne_bytes(stripe_data[index..index + 4].try_into().unwrap());
                let y = f32::from_ne_bytes(stripe_data[index + 4..index + 8].try_into().unwrap());
                let z = f32::from_ne_bytes(stripe_data[index + 8..index + 12].try_into().unwrap());
                let w = f32::from_ne_bytes(stripe_data[index + 12..index + 16].try_into().unwrap());
                (x, y, z, w)
            });

        let stripe_layer = Layer::new(
            size,
            LayerAttributes {
                layer_name: Some("Stripe".into()),
                ..Default::default()
            },
            Encoding {
                compression: Compression::PIZ,
                ..Default::default()
            },
            stripe_channels,
        );

        Image::empty(ImageAttributes::with_size(size))
            .with_layer(color_layer)
            .with_layer(step_layer)
            .with_layer(z_layer)
            .with_layer(dz_layer)
            .with_layer(orbit_layer)
            .with_layer(stripe_layer)
            .write()
            .on_progress(|progress| {
                status_callback(ProgressUpdate::partial("Writing EXR file", progress))
            })
            .to_file(path)?;

        Ok(())
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
        self.probed_data = probe::<f32>(
            &image.location.probe_location,
            image.location.max_iter,
            image.location.zoom,
            julia_point,
            status_callback,
        );
        self.cache_validity.probe = true;
    }

    fn reupload(
        &mut self,
        status_callback: &mut impl FnMut(ProgressUpdate),
    ) -> Result<(), RenderingError> {
        status_callback(ProgressUpdate::msg("Uploading probe"));
        self.gpu_data.shared.queue.write_buffer(
            &self.gpu_data.buffers.probe,
            0,
            bytemuck::cast_slice(&self.probed_data[..]),
        );
        // We wait for this operation to complete to get accurate timings;
        // this upload is usually a negligible cost.
        let si = self.gpu_data.shared.queue.submit([]);

        self.poll_unless_cancelled(Some(si), status_callback)?;
        self.cache_validity.gpu_probe = true;
        Ok(())
    }

    fn recompute(
        &mut self,
        image: &ImgSpec,
        status_callback: &mut impl FnMut(ProgressUpdate),
    ) -> Result<(), RenderingError> {
        status_callback(ProgressUpdate::partial("Computing iterations", 0.0));
        match image.algorithm() {
            Algorithm::Directf32 | Algorithm::Perturbedf32 => run_compute_step(
                &self.gpu_data,
                &self.probed_data,
                image,
                self.constants,
                self.cancelled.clone(),
                status_callback,
            )?,
            Algorithm::Directf32CPU | Algorithm::Perturbedf32CPU | Algorithm::DirectFloatCPU => {
                run_cpu_compute(
                    &self.gpu_data,
                    &self.probed_data,
                    image,
                    self.constants,
                    self.cancelled.clone(),
                    status_callback,
                )?
            }
        }
        self.poll_unless_cancelled(None, status_callback)?;
        self.cache_validity.compute = true;
        Ok(())
    }

    fn recolor(
        &mut self,
        image: &ImgSpec,
        status_callback: &mut impl FnMut(ProgressUpdate),
    ) -> Result<(), RenderingError> {
        status_callback(ProgressUpdate::msg("Rendering Colors"));
        let si = run_render_step(&self.gpu_data, image);
        self.poll_unless_cancelled(Some(si), status_callback)?;
        self.cache_validity.color = true;
        Ok(())
    }

    fn poll_unless_cancelled(
        &mut self,
        si: Option<wgpu::SubmissionIndex>,
        status_callback: &mut impl FnMut(ProgressUpdate),
    ) -> Result<(), RenderingError> {
        loop {
            match self.gpu_data.shared.device.poll(wgpu::PollType::Wait {
                submission_index: si.clone(),
                timeout: Some(Duration::from_millis(100)),
            }) {
                Ok(wgpu::PollStatus::QueueEmpty) => return Ok(()),
                Ok(wgpu::PollStatus::WaitSucceeded) => return Ok(()),
                Ok(wgpu::PollStatus::Poll) => unreachable!(),
                Err(wgpu::PollError::Timeout) => {
                    if self.is_cancelled(status_callback) {
                        return Err(RenderingError::Cancelled);
                    }
                }
                Err(wgpu::PollError::WrongSubmissionIndex(a, b)) => {
                    tracing::error!("Received the wrong submission index! {a}, {b}");
                    return Ok(());
                }
            }
        }
    }

    fn is_cancelled(&mut self, status_callback: &mut impl FnMut(ProgressUpdate)) -> bool {
        if self.cancelled.load(Ordering::Relaxed) {
            status_callback(ProgressUpdate::partial("Cancelled", 1.0));
            true
        } else {
            false
        }
    }

    fn diff(&self, image: &ImgSpec) -> ImageDiff {
        self.last_image
            .as_ref()
            .map(|img| image.compare(img))
            .unwrap_or(ImageDiff::full())
    }
}

fn run_cpu_compute(
    gpu_data: &GPUData,
    probed_data: &[[f32; 2]],
    image: &ImgSpec,
    constants: Constants,
    cancelled: Arc<AtomicBool>,
    status_callback: &mut impl FnMut(ProgressUpdate),
) -> Result<(), RenderingError> {
    let image_size = image.width * image.height;
    let mut data = vec![];

    let render_func: for<'a> fn(_, _, _, _, &'a _) -> BufferValues = match image.algorithm() {
        Algorithm::Directf32 | Algorithm::Perturbedf32 => unreachable!(),
        Algorithm::Directf32CPU => direct_32::calculate_point,
        Algorithm::Perturbedf32CPU => perturbed_32::calculate_point,
        // direct float function interface is not compatible
        Algorithm::DirectFloatCPU => direct_32::calculate_point,
    };

    let parameters = ComputeParams::create(image, probed_data.len());

    // run render function

    let (send, rcv) = mpsc::channel();

    let finished = AtomicU64::new(0);
    std::thread::scope(|s| {
        s.spawn(|| {
            data = (0..image_size)
                .into_par_iter()
                .map(|index| {
                    if cancelled.load(Ordering::Relaxed) {
                        return BufferValues::zero();
                    }
                    let mut bv = BufferValues::zero();
                    let mut hpf_bv = hpf_algorithm::BufferValues::zero();
                    for i in 0..=(image.location.max_iter / constants.iter_batch_size) {
                        let parameters = parameters.with_iter(image, i, constants.iter_batch_size);
                        if parameters.chunk_max_iter == 0 {
                            break;
                        }
                        if image.algorithm() == Algorithm::DirectFloatCPU {
                            let new_values = hpf_algorithm::calculate_point(
                                Vec2::new(index % image.width, index / image.width),
                                hpf_bv.clone(),
                                image.get_flags(),
                                parameters,
                                image.location.center.clone(),
                            );
                            hpf_bv = new_values;
                            if hpf_bv.step != 0 {
                                break;
                            }
                        } else {
                            let new_values = render_func(
                                Vec2::new(index % image.width, index / image.width),
                                bv.clone(),
                                image.get_flags(),
                                parameters,
                                bytemuck::cast_slice(probed_data),
                            );
                            bv = new_values;
                            if bv.step != 0 {
                                break;
                            }
                        }
                    }
                    if image.algorithm() == Algorithm::DirectFloatCPU {
                        bv = hpf_bv.to_f32();
                    }
                    if finished
                        .fetch_add(1, Ordering::Relaxed)
                        .is_multiple_of(1000)
                    {
                        let _ = send.send(ProgressUpdate::partial(
                            "Computing iterations",
                            finished.load(Ordering::Relaxed) as f64 / image_size as f64,
                        ));
                    };
                    bv
                })
                .collect::<Vec<BufferValues>>();
            drop(send);
        });
        while let Ok(val) = rcv.recv() {
            status_callback(val);
        }
    });
    if cancelled.load(Ordering::Relaxed) {
        return Err(RenderingError::Cancelled);
    }
    status_callback(ProgressUpdate::partial("Computing iterations", 1.0));

    // upload data to GPU buffers
    gpu_data.upload_buffer_data(
        gpu_setup::BufferId::Step,
        bytemuck::cast_slice(&data.iter().map(|x| x.step).collect::<Vec<_>>()),
    );
    gpu_data.upload_buffer_data(
        gpu_setup::BufferId::Orbit,
        bytemuck::cast_slice(&data.iter().map(|x| x.orbits).collect::<Vec<_>>()),
    );
    gpu_data.upload_buffer_data(
        gpu_setup::BufferId::Stripe,
        bytemuck::cast_slice(&data.iter().map(|x| x.stripes).collect::<Vec<_>>()),
    );
    gpu_data.upload_buffer_data(
        gpu_setup::BufferId::Z,
        bytemuck::cast_slice(
            &data
                .iter()
                .map(|x| {
                    Vec4::new(
                        x.delta_n.x,
                        x.delta_n.y,
                        x.zoom,
                        bytemuck::cast(x.ref_iteration),
                    )
                })
                .collect::<Vec<_>>(),
        ),
    );
    gpu_data.upload_buffer_data(
        gpu_setup::BufferId::Dz,
        bytemuck::cast_slice(
            &data
                .iter()
                .map(|x| Vec4::new(x.z_n_prime.x, x.z_n_prime.y, x.zoom_prime, 0.0))
                .collect::<Vec<_>>(),
        ),
    );

    Ok(())
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

    let compute_pipeline = match image.algorithm() {
        crate::types::Algorithm::Directf32 => direct_f32_pipeline,
        crate::types::Algorithm::Perturbedf32 => perturbed_f32_pipeline,
        _ => unreachable!(),
    };
    let parameters = ComputeParams::create(image, probed_data.len());

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
        // Update the parameters
        let parameters = parameters.with_iter(image, i, constants.iter_batch_size);
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
        if cancelled.load(Ordering::Relaxed) {
            status_callback(ProgressUpdate::partial("Cancelled", 1.0));
            return Err(RenderingError::Cancelled);
        }
    }
    Ok(())
}

/// Runs the render shader on the GPU
fn run_render_step(gpu_data: &GPUData, image: &ImgSpec) -> wgpu::SubmissionIndex {
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
    queue.submit(Some(encoder.finish()))
}
