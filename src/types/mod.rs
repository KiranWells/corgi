/*!
# Types

A Collection of types used throughout the application, and their associated functions.
 */

mod image;
mod preview_resources;

use eframe::wgpu;
use std::path::PathBuf;
use std::time::Duration;
use std::{error::Error, fmt::Display};

use color_eyre::Report;

pub use self::image::*;
pub use self::preview_resources::*;

pub const ESCAPE_RADIUS: f64 = 1e10;
// This needs to be set low enough to ensure the GPU is not
// busy for too long; that causes stuttering and possibly
// shader execution failures.
pub const MAX_GPU_GROUP_ITER: usize = 10000;

/// Get the precision for a given zoom level
pub fn get_precision(zoom: f64) -> u32 {
    ((zoom * 1.5) as u32).max(53)
}

#[derive(Debug)]
pub enum Message {
    NewPreviewSettings(Image),
    NewOutputSettings(Image),
    SaveToFile(PathBuf),
}

pub enum StatusMessage {
    Progress(String, f64),
    NewPreviewViewport(Viewport),
}

/// Shared status between the main thread and the render thread
#[derive(Default, Debug, Clone)]
pub struct Status {
    pub message: String,
    pub progress: Option<f64>,
    pub rendered_image: Option<Image>,
}

/// Error type for the render thread
#[derive(Debug)]
pub enum RenderErr {
    /// Signals a need to resize the window
    Resize,
    /// Signals an error that should cause the application to quit
    Quit(Report),
    /// Signals an error that should be logged but not cause the application to quit
    Warn(Report),
}

impl From<wgpu::SurfaceError> for RenderErr {
    fn from(e: wgpu::SurfaceError) -> Self {
        match e {
            wgpu::SurfaceError::Lost => Self::Resize,
            wgpu::SurfaceError::OutOfMemory => Self::Quit(e.into()),
            wgpu::SurfaceError::Timeout => Self::Warn(e.into()),
            wgpu::SurfaceError::Outdated => Self::Warn(e.into()),
            wgpu::SurfaceError::Other => Self::Warn(e.into()),
        }
    }
}

impl From<Report> for RenderErr {
    fn from(e: Report) -> Self {
        Self::Quit(e)
    }
}

impl Display for RenderErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Resize => write!(f, "Resize"),
            Self::Quit(e) => write!(f, "Quit: {e}"),
            Self::Warn(e) => write!(f, "Warn: {e}"),
        }
    }
}

impl Error for RenderErr {}

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

/// A container for the egui-related state
pub struct EguiData {
    pub ctx: eframe::egui::Context,
    pub renderer: eframe::egui_wgpu::Renderer,
    pub needs_rerender: bool,
}
