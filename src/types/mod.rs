mod gpu;
mod image;
mod preview_resources;

use std::{error::Error, fmt::Display};

use color_eyre::Report;

pub use self::gpu::*;
pub use self::image::*;
pub use self::preview_resources::*;

pub const ESCAPE_RADIUS: f64 = 1e10;
pub const MAX_GPU_GROUP_ITER: usize = 500;

pub fn get_precision(zoom: f64) -> u32 {
    ((zoom * 1.5) as u32).max(53)
}

#[derive(Default, Debug, Clone)]
pub struct Status {
    pub message: String,
    pub progress: Option<f64>,
    pub rendered_image: Option<Image>,
}

#[derive(Debug)]
pub enum RenderErr {
    Resize,
    Quit(Report),
    Warn(Report),
}

impl From<wgpu::SurfaceError> for RenderErr {
    fn from(e: wgpu::SurfaceError) -> Self {
        match e {
            wgpu::SurfaceError::Lost => Self::Resize,
            wgpu::SurfaceError::OutOfMemory => Self::Quit(e.into()),
            wgpu::SurfaceError::Timeout => Self::Warn(e.into()),
            wgpu::SurfaceError::Outdated => Self::Warn(e.into()),
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
            Self::Quit(e) => write!(f, "Quit: {}", e),
            Self::Warn(e) => write!(f, "Warn: {}", e),
        }
    }
}

impl Error for RenderErr {}

pub struct Debouncer {
    wait_time: std::time::Duration,
    last_triggered: Option<std::time::Instant>,
}

impl Debouncer {
    pub fn new(wait: std::time::Duration) -> Self {
        Self {
            wait_time: wait,
            last_triggered: None,
        }
    }

    pub fn trigger(&mut self) {
        self.last_triggered = Some(std::time::Instant::now());
    }

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

    pub fn reset(&mut self) {
        self.last_triggered = None;
    }
}

pub struct EguiData {
    pub state: egui_winit::State,
    pub ctx: egui::Context,
    pub renderer: egui_wgpu::Renderer,
    pub needs_rerender: bool,
}
