/*!
# Types

A Collection of types used throughout the application, and their associated functions.
 */

mod coloring;
mod image;
pub mod serde;

use std::fmt::Display;
use std::path::PathBuf;
use std::time::Duration;

use eframe::egui::Vec2;

pub use self::coloring::*;
pub use self::image::*;
use crate::image_gen::Constants;

pub const ESCAPE_RADIUS: f64 = 1e10;

/// Get the precision for a given zoom level
pub fn get_precision(zoom: f32) -> u32 {
    ((zoom * 1.25) as u32).max(53)
}

#[derive(Debug)]
pub enum ImageGenCommand {
    Render(RendererId, Box<ImgSpec>),
    UpdateConstants(RendererId, Constants),
    SaveToFile(RendererId, PathBuf),
    ShutDown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RendererId {
    Explore,
    Style,
    Render,
}

#[derive(Debug)]
pub enum StatusMessage {
    Progress(ProgressUpdate),
    RenderFinished(RendererId, ImageTimings, View),
    Error(color_eyre::Report),
}

#[derive(Debug)]
pub struct ProgressUpdate {
    pub message: &'static str,
    pub progress: Option<f64>,
}

impl ProgressUpdate {
    pub fn msg(message: &'static str) -> Self {
        Self {
            message,
            progress: None,
        }
    }

    pub fn partial(message: &'static str, percent: f64) -> Self {
        Self {
            message,
            progress: Some(percent),
        }
    }
}

/// Shared status between the main thread and the render thread
#[derive(Default, Debug, Clone)]
pub struct Status {
    pub message: String,
    pub progress: Option<f64>,
    pub rendered_image: Option<ImgSpec>,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ColorParams {
    pub saturation: f32,
    pub brightness: f32,
    pub color_frequency: f32,
    pub color_offset: f32,
    pub gradient_kind: u32,
    pub gradient_size: u32,
    pub lighting_kind: u32,
    padding: u32,
    pub color_layer_types: [u8; MAX_LAYERS],
    pub light_layer_types: [u8; MAX_LAYERS],
    pub color_strengths: [f32; MAX_LAYERS],
    pub color_params: [f32; MAX_LAYERS],
    pub light_strengths: [f32; MAX_LAYERS],
    pub light_params: [f32; MAX_LAYERS],
    pub lights: [Light; MAX_LIGHTS],
    pub overlays: OverlayParams,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct OverlayParams {
    pub iteration_outline: [f32; 4],
    pub set_outline: [f32; 4],
}

impl From<&Coloring> for ColorParams {
    fn from(value: &Coloring) -> Self {
        let (gradient_kind, gradient_vec) = value.gradient.decompose();
        fn map_to_array<T: Default + Copy, U, const N: usize>(
            v: &[U],
            f: impl Fn(&U) -> T,
        ) -> [T; N] {
            let mut arr = [T::default(); N];
            let newv = v.iter().map(f).collect::<Vec<T>>();
            arr[..newv.len()].copy_from_slice(&newv);
            arr
        }
        ColorParams {
            saturation: value.saturation,
            brightness: value.brightness,
            color_frequency: value.color_frequency,
            color_offset: value.color_offset,
            gradient_kind,
            gradient_size: gradient_vec.len() as u32 / 4,
            lighting_kind: value.lighting_kind as u32,
            color_layer_types: map_to_array(&value.color_layers, |x| x.kind as u8),
            light_layer_types: map_to_array(&value.light_layers, |x| x.kind as u8),
            color_strengths: map_to_array(&value.color_layers, |x| x.strength),
            color_params: map_to_array(&value.color_layers, |x| x.param),
            light_strengths: map_to_array(&value.light_layers, |x| x.strength),
            light_params: map_to_array(&value.light_layers, |x| x.param),
            lights: map_to_array(&value.lights, Light::clone),
            overlays: (&value.overlays).into(),
            padding: 0,
        }
    }
}

impl From<&Overlays> for OverlayParams {
    fn from(value: &Overlays) -> Self {
        Self {
            iteration_outline: pack_outline(&value.iteration_outline),
            set_outline: pack_outline(&value.set_outline),
        }
    }
}

fn pack_outline(value: &Option<Outline>) -> [f32; 4] {
    if let Some(inner) = value {
        let mut packed = inner.color.to_rgba_unmultiplied();
        packed[3] *= 0.999;
        packed[3] += inner.parameter as f32;
        packed
    } else {
        [0.0; 4]
    }
}

/// The parameters for the compute shader. This is sent as a uniform
/// to the compute shader.
#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ComputeParams {
    pub width: u32,
    pub height: u32,
    pub max_iter: u32,
    pub chunk_max_iter: u32,
    pub probe_len: u32,
    pub iter_offset: u32,
    pub x: f32,
    pub y: f32,
    pub zoom: f32,
    pub angle: f32,
    pub julia_x: f32,
    pub julia_y: f32,
}

/// The parameters for the render shader. This is sent as a uniform
/// to the render shader.
#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct RenderParams {
    pub width: u32,
    pub height: u32,
}

impl From<&ImgSpec> for RenderParams {
    fn from(image: &ImgSpec) -> Self {
        RenderParams {
            width: (image.width as f64) as u32,
            height: (image.height as f64) as u32,
        }
    }
}

/// The parameters for the preview shader. This is sent as a uniform
/// to the preview shader.
#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Transform {
    pub angle: f32,
    pub _padding: f32,
    pub prescale: [f32; 2],
    pub postscale: [f32; 2],
    pub offset: [f32; 2],
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            angle: 0.0,
            _padding: 0.0,
            prescale: [1.0, 1.0],
            postscale: [1.0, 1.0],
            offset: [0.0, 0.0],
        }
    }
}

/// Debouncer for events
///
/// The debouncer will only return true once the wait time has passed,
/// and will return false until triggered again.
///
/// # Usage
///
/// ```
/// use corgi::types::Debouncer;
///
/// let now = std::time::Instant::now();
/// let mut debouncer = Debouncer::new(std::time::Duration::from_millis(100));
///
/// // Trigger the debouncer
/// debouncer.trigger();
///
/// // Poll the debouncer
/// // This will return false until 100ms have passed
/// while !debouncer.poll() {
///    // sleep for 100ms
///    std::thread::sleep(std::time::Duration::from_millis(10));
/// }
/// // The debouncer can now be triggered again
/// assert!(now.elapsed() >= std::time::Duration::from_millis(100));
///
/// // Reset the debouncer
/// debouncer.reset();
/// assert!(!debouncer.poll());
/// ```
#[derive(Debug)]
pub struct Debouncer {
    pub wait_time: std::time::Duration,
    last_triggered: Option<std::time::Instant>,
}

impl Debouncer {
    /// Create a new debouncer with the given wait time
    pub fn new(wait: std::time::Duration) -> Self {
        Self {
            wait_time: wait,
            last_triggered: None,
        }
    }

    /// Trigger the debouncer. This will reset the timer.
    pub fn trigger(&mut self) {
        self.last_triggered = Some(std::time::Instant::now());
    }

    /// Poll the debouncer. This will return true if the wait time has passed,
    /// and will only return true once. It will return false until triggered again,
    /// and the wait time has passed.
    pub fn poll(&mut self) -> bool {
        if let Some(v) = self.last_triggered {
            let now = std::time::Instant::now();
            if now - v >= self.wait_time {
                self.last_triggered = None;
                return true;
            }
        }
        false
    }

    /// Reset the debouncer. This will reset the timer, requiring the debouncer
    /// to be triggered again before it will return true.
    pub fn reset(&mut self) {
        self.last_triggered = None;
    }

    /// Returns whether the debouncer has a valid last_triggered time.
    /// This will be true if the debouncer is still waiting or if
    /// it is already complete, but has not been polled.
    pub fn active(&self) -> bool {
        self.last_triggered.is_some()
    }

    /// Returns a duration representing the time until poll will return true,
    /// or None if there is no more time to wait (even if poll has not yet been called).
    pub fn remaining(&self) -> Option<Duration> {
        if let Some(v) = self.last_triggered {
            let now = std::time::Instant::now();
            if now - v >= self.wait_time {
                None
            } else {
                Some(self.wait_time - (now - v))
            }
        } else {
            None
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ImageTimings {
    pub probe: Duration,
    pub compute: Duration,
    pub color: Duration,
    pub build: Duration,
}

impl ImageTimings {
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

impl Display for ImageTimings {
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

pub trait Rotate {
    fn rotated(&self, angle: f32) -> Self;
}

impl Rotate for Vec2 {
    fn rotated(&self, angle: f32) -> Self {
        Self {
            x: self.x * angle.cos() - self.y * angle.sin(),
            y: self.x * angle.sin() + self.y * angle.cos(),
        }
    }
}
