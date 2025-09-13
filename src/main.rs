use std::{
    env, str::FromStr, sync::atomic::AtomicBool, time::Instant,
};

use clap::Parser;
use color_eyre::{Result, eyre::eyre};
use corgi::{
    app::{CorgiApp, CorgiCliOptions},
    image_gen::{GPUData, SharedState, is_metadata_supported, render_image},
    types::Image,
};
use eframe::{egui, egui_wgpu, wgpu};
use image::ImageBuffer;
use little_exif::{exif_tag::ExifTag, metadata::Metadata};
use nanoserde::SerJson;
use pollster::FutureExt;
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

fn main() -> Result<()> {
    let cli_options = CorgiCliOptions::parse();
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

    // cli only render
    if let Some(path) = cli_options.output_file {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .block_on()?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
                // WebGL doesn't support all of wgpu's features, so if
                // we're building for the web we'll have to disable some.
                required_limits: wgpu::Limits::default(),
                memory_hints: Default::default(),
                trace: wgpu::Trace::Off,
            })
            .block_on()?;

        let image = Image::load_from_file(
            &cli_options
                .image_file
                .ok_or(eyre!("No image file specified"))?,
        )?;
        let mut gpu_data = GPUData::init(
            &image.viewport,
            image.max_iter as usize,
            SharedState::new(device, queue),
            "cli renderer",
        );
        let (send, _) = std::sync::mpsc::channel();
        let now = Instant::now();
        render_image(
            &mut gpu_data,
            &mut (vec![], vec![]),
            &image,
            None,
            send,
            std::sync::Arc::new(AtomicBool::new(false)),
            None,
        );
        println!("Rendering took {:?}", Instant::now().duration_since(now));
        if let Some(data) = gpu_data.get_texture_data() {
            let mut img = image::DynamicImage::ImageRgba8(
                ImageBuffer::from_raw(
                    image.viewport.width as u32,
                    image.viewport.height as u32,
                    data,
                )
                .expect("image data to be properly formatted"),
            );
            img = image::DynamicImage::ImageRgb8(img.flipv().into_rgb8());
            if let Err(err) = img.save(path.clone()) {
                tracing::error!("Failed to save image: {err}");
            } else {
                // add metadata
                if is_metadata_supported(&path) {
                    let mut meta = Metadata::new();
                    meta.set_tag(ExifTag::ImageDescription(image.serialize_json()));
                    meta.set_tag(ExifTag::Software("Corgi".into()));
                    if let Err(err) = meta.write_to_file(&path) {
                        tracing::error!("Failed to write metadata to file: {err:?}");
                    }
                }
            }
        }
        return Ok(());
    }

    // start app
    let eframe_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title("Corgi Fractal Renderer"),
        vsync: true,
        hardware_acceleration: eframe::HardwareAcceleration::Preferred,
        renderer: eframe::Renderer::Wgpu,
        multisampling: 4,
        wgpu_options: egui_wgpu::WgpuConfiguration {
            present_mode: wgpu::PresentMode::AutoVsync,
            desired_maximum_frame_latency: None,
            ..Default::default()
        },
        ..Default::default()
    };
    eframe::run_native(
        "Corgi",
        eframe_options,
        Box::new(|cc| CorgiApp::new_dyn(cc, cli_options)),
    )?;
    Ok(())
}
