/*!
# Types

A Collection of types used throughout the application, and their associated functions.
 */

mod coloring;
mod image;

use std::path::PathBuf;
use std::time::Duration;

pub use self::coloring::*;
pub use self::image::*;

pub const ESCAPE_RADIUS: f64 = 1e10;

/// Get the precision for a given zoom level
pub fn get_precision(zoom: f64) -> u32 {
    ((zoom * 1.25) as u32).max(53)
}

#[derive(Debug)]
pub enum ImageGenCommand {
    NewPreviewSettings(Image),
    NewOutputSettings(Image),
    SaveToFile(PathBuf),
}

#[derive(Debug)]
pub enum StatusMessage {
    Progress(String, f64),
    NewPreviewViewport(Duration, Viewport),
    NewOutputViewport(Duration, Viewport),
}

/// Shared status between the main thread and the render thread
#[derive(Default, Debug, Clone)]
pub struct Status {
    pub message: String,
    pub progress: Option<f64>,
    pub rendered_image: Option<Image>,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ColorParams {
    pub saturation: f32,
    pub brightness: f32,
    pub color_frequency: f32,
    pub color_offset: f32,
    pub gradient_kind: u32,
    pub lighting_kind: u32,
    padding: [u32; 2],
    pub gradient: [f32; 12],
    pub color_layer_types: [u8; 8],
    pub light_layer_types: [u8; 8],
    pub color_strengths: [f32; 8],
    pub color_params: [f32; 8],
    pub light_strengths: [f32; 8],
    pub light_params: [f32; 8],
    pub lights: [Light; 3],
    pub overlays: Overlays,
}

impl From<&Coloring> for ColorParams {
    fn from(value: &Coloring) -> Self {
        let (gradient_kind, gradient_vec) = match value.gradient {
            Gradient::Flat(data) => {
                let mut new_data = data.to_vec();
                new_data.extend_from_slice(&[0.0; 9]);
                (0, new_data)
            }
            Gradient::Procedural(data) => (1, data.concat()),
            Gradient::Manual(data) => (2, data.concat()),
            Gradient::Hsv(saturation, value) => {
                let mut new_data = vec![saturation, value];
                new_data.extend_from_slice(&[0.0; 10]);
                (3, new_data)
            }
        };
        let mut gradient = [0.0; 12];
        gradient.copy_from_slice(&gradient_vec);
        ColorParams {
            saturation: value.saturation,
            brightness: value.brightness,
            color_frequency: value.color_frequency,
            color_offset: value.color_offset,
            gradient_kind,
            lighting_kind: value.lighting_kind as u32,
            gradient,
            color_layer_types: value.color_layers.map(|x| x.kind as u8),
            light_layer_types: value.light_layers.map(|x| x.kind as u8),
            color_strengths: value.color_layers.map(|x| x.strength),
            color_params: value.color_layers.map(|x| x.param),
            light_strengths: value.light_layers.map(|x| x.strength),
            light_params: value.light_layers.map(|x| x.param),
            lights: value.lights,
            overlays: value.overlays,
            padding: [0; 2],
        }
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
    pub cx: f32,
    pub cy: f32,
    pub zoom: f32,
}

/// The parameters for the render shader. This is sent as a uniform
/// to the render shader.
#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct RenderParams {
    pub width: u32,
    pub height: u32,
    pub max_step: u32,
    pub zoom: f32,
    pub misc: f32,
    pub debug_shutter: f32,
}

impl From<&Image> for RenderParams {
    fn from(image: &Image) -> Self {
        RenderParams {
            width: (image.viewport.width as f64 * image.viewport.scaling) as u32,
            height: (image.viewport.height as f64 * image.viewport.scaling) as u32,
            max_step: image.max_iter as u32,
            zoom: image.viewport.zoom as f32,
            misc: image.misc,
            debug_shutter: image.debug_shutter,
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
    pub scale: [f32; 2],
    pub offset: [f32; 2],
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            angle: 0.0,
            _padding: 0.0,
            scale: [1.0, 1.0],
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
