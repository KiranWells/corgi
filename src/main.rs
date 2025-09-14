#![doc = include_str!("../README.md")]
pub mod app;
pub mod ui;
pub mod worker;

use std::{env, str::FromStr, sync::atomic::AtomicBool, time::Instant};

use app::{CorgiApp, CorgiCliOptions};
use clap::Parser;
use color_eyre::{Result, eyre::eyre};
use corgi::{
    image_gen::{GPUData, SharedState, get_device_and_queue, is_metadata_supported, render_image},
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
                .map(|s| Level::from_str(&s).expect("log level to be valid"))
                .unwrap_or(Level::WARN),
        )
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;
    color_eyre::install()?;

    // cli only render
    if let Some(path) = cli_options.output_file {
        let (device, queue) = get_device_and_queue().block_on()?;

        let image = Image::load_from_file(
            &cli_options
                .settings_file
                .ok_or(eyre!("No image file specified"))?,
        )?;
        let mut gpu_data = GPUData::init(
            &image.viewport,
            image.max_iter as usize,
            SharedState::new(device, queue),
            "cli renderer",
        );
        let now = Instant::now();
        render_image(
            &mut gpu_data,
            &mut (vec![], vec![]),
            &image,
            None,
            std::sync::Arc::new(AtomicBool::new(false)),
            |_sm| {},
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
        Box::new(|cc| CorgiApp::create(cc, cli_options)),
    )?;
    Ok(())
}
