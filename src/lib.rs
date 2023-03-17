#![doc = include_str!("../README.md")]

mod app;
pub mod image_gen;
pub mod types;
mod ui;

use std::sync::Arc;

use color_eyre::Result;
use image_gen::render_thread;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use winit::event_loop::EventLoop;
use winit::window::WindowBuilder;

use types::{GPUHandles, Image, PreviewRenderResources, Status};

/// Main entry point for the application
pub async fn run() -> Result<()> {
    // set up initial data
    // - message queue for image changes
    // - mutex for status
    let (sender, receiver) = mpsc::channel::<Image>(10);
    let status = Arc::new(Mutex::new(Status::default()));

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new().build(&event_loop)?;

    // render thread data setup
    let (gpu_data, window) = GPUHandles::init(window).await?;
    let initial_image = Image::default();
    let render_gpu_data = image_gen::GPUData::init(
        &initial_image,
        gpu_data.device.clone(),
        gpu_data.queue.clone(),
    )
    .await;
    let render_thread_status = status.clone();

    let preview_resources = PreviewRenderResources::init(
        &gpu_data.device,
        gpu_data.surface_config.format,
        render_gpu_data.rendered_image.clone(),
        (0, 0),
    )
    .await?;

    let state = app::CorgiState::init(
        window,
        &event_loop,
        sender,
        status,
        preview_resources,
        gpu_data,
    )
    .await?;

    tokio::spawn(
        async move { render_thread(receiver, render_thread_status, render_gpu_data).await },
    );

    state.start(event_loop)
}
