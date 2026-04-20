#![doc = include_str!("../README.md")]
pub mod app;
pub mod config;
pub mod ui;
pub mod worker;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use clap::Parser;
use color_eyre::Result;
use color_eyre::eyre::eyre;
use corgi_lib::image_gen::{
    CompressionParams, Constants, Engine, ProgressUpdate, SharedState, get_device_and_queue,
};
use corgi_lib::types::serde::SafeSaveLoad;
use corgi_lib::types::{ImgSpec, OptLevel};
use eframe::{egui, egui_wgpu, wgpu};
use pollster::FutureExt;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{EnvFilter, fmt};

use crate::app::{CorgiApp, CorgiCliOptions};
use crate::config::{Config, Context, Theme};

fn main() -> Result<()> {
    let cli_options = CorgiCliOptions::parse();
    // set up logging
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_env("CORGI_LOG_LEVEL"))
        .init();
    color_eyre::install()?;

    if let Some(path) = cli_options.output_file {
        headless_render(
            cli_options.settings_file.as_ref(),
            &path,
            CompressionParams::new(
                cli_options.compression_speed,
                cli_options.compression_quality,
            )?,
        )?;
        return Ok(());
    }

    let context = Context::load()?;

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
        Box::new(|cc| CorgiApp::create(cc, cli_options, context)),
    )?;
    Ok(())
}

/// Render a given image without starting the UI, and with non-interactive settings.
///
/// Prints status to the console.
fn headless_render(
    settings_file: Option<&PathBuf>,
    path: &Path,
    compression_params: CompressionParams,
) -> Result<()> {
    let (device, queue) = get_device_and_queue().block_on()?;
    let Some(settings_file) = settings_file else {
        return Err(eyre!("No settings file specified, exiting."));
    };
    if !settings_file.exists() {
        return Err(eyre!("Settings file does not exist"));
    }
    let mut image = ImgSpec::load(settings_file)?;
    image.optimization_level = OptLevel::AccuracyOptimized;
    let mut engine = Engine::init(
        SharedState::new(device, queue),
        "cli renderer",
        image.extents(),
        image.location.max_iter as usize,
        Constants {
            iter_batch_size: 10_000,
        },
        std::sync::Arc::new(AtomicBool::new(false)),
    );
    let mut current_step = "";
    let mut status_callback = |pu: ProgressUpdate| {
        if let Some(percent) = pu.progress {
            if current_step == pu.message {
                print!("\r");
            } else {
                if !current_step.is_empty() {
                    println!();
                }
                current_step = pu.message;
            }
            print!("{:>6.2}% | {}", percent * 100.0, pu.message);
        } else {
            if !current_step.is_empty() {
                println!();
            }
            print!("------- | {}", pu.message);
            current_step = pu.message;
        }
        let _ = std::io::stdout().lock().flush();
    };
    let timings = engine.render_image(&image, &mut status_callback)?;
    print!("\n------- | Rendering timings: {}", timings);
    engine.save_to_file(
        path,
        None,
        compression_params,
        corgi_lib::types::serde::is_metadata_supported(path),
        &mut status_callback,
    )?;
    println!();
    Ok(())
}
