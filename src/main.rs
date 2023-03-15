use std::{env, sync::mpsc::channel};
use std::{
    str::FromStr,
    sync::{Arc, Mutex},
};

use color_eyre::{eyre::eyre, Result};
use corgi::{
    image_gen::render_thread,
    run,
    types::{GPUData, Image, PreviewRenderResources, Status},
};
use futures::executor::block_on;
use tracing::Level;
use tracing_subscriber::FmtSubscriber;
use winit::{dpi::PhysicalSize, event_loop::EventLoop, window::WindowBuilder};

fn main() -> Result<()> {
    // set up logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(
            env::var("CORGI_LOG_LEVEL")
                .ok()
                .and_then(|s| Level::from_str(&s).ok())
                .unwrap_or(Level::WARN),
        )
        .finish();

    tracing::subscriber::set_global_default(subscriber)?;
    color_eyre::install()?;

    // set up initial data
    // - message queue for image changes
    // - mutex for status
    let message_queue = channel::<Image>();
    let status_mutex = Arc::new(Mutex::new(Status::default()));

    // start render thread
    let movable = status_mutex.clone();

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_inner_size(PhysicalSize {
            width: 512,
            height: 512,
        })
        .build(&event_loop)?;
    let gpu_data = block_on(GPUData::init(&window))?;
    let initial_image = Image::default();
    let render_gpu_data = block_on(corgi::image_gen::GPUData::init(
        &initial_image,
        gpu_data.device.clone(),
        gpu_data.queue.clone(),
    ));
    let preview_data = PreviewRenderResources::init(
        &gpu_data.device,
        gpu_data.surface_config.format,
        render_gpu_data.rendered_image.clone(),
        (0, 0),
    )?;
    let r_thread = std::thread::spawn(move || {
        block_on(render_thread(message_queue.1, movable, render_gpu_data))
    });

    // start UI thread
    block_on(run(
        message_queue.0,
        status_mutex,
        window,
        event_loop,
        preview_data,
        gpu_data,
    ))?;

    r_thread
        .join()
        .map_err(|e| eyre!("Panicked on: {:?}", e))??;
    Ok(())
}
