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
use winit::{dpi::PhysicalSize, window::WindowBuilder};

use types::{GPUData, Image, PreviewRenderResources, Status};

pub async fn run() -> Result<()> {
    // set up initial data
    // - message queue for image changes
    // - mutex for status
    let (sender, receiver) = mpsc::channel::<Image>(10);
    let status = Arc::new(Mutex::new(Status::default()));

    // start render thread

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_inner_size(PhysicalSize {
            width: 512,
            height: 512,
        })
        .build(&event_loop)?;
    let gpu_data = GPUData::init(&window).await?;
    let initial_image = Image::default();
    let render_gpu_data = image_gen::GPUData::init(
        &initial_image,
        gpu_data.device.clone(),
        gpu_data.queue.clone(),
    )
    .await;
    let preview_resources = PreviewRenderResources::init(
        &gpu_data.device,
        gpu_data.surface_config.format,
        render_gpu_data.rendered_image.clone(),
        (0, 0),
    )
    .await?;
    let render_thread_status = status.clone();

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
