#![doc = include_str!("../README.md")]
pub mod app;
pub mod config;
pub mod ui;
pub mod worker;

use std::env;
use std::fs::read_to_string;
use std::io::Write;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use clap::Parser;
use color_eyre::Result;
use color_eyre::eyre::eyre;
use corgi::image_gen::{
    Constants, GPUData, SharedState, get_device_and_queue, render_image, save_to_file,
};
use corgi::types::{Image, OptLevel, StatusMessage};
use directories::ProjectDirs;
use eframe::{egui, egui_wgpu, wgpu};
use pollster::FutureExt;
use serde::Deserialize;
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

use crate::app::{CorgiApp, CorgiCliOptions};
use crate::config::{Cache, Config, Context, Theme};

fn load_from_toml<T: for<'a> Deserialize<'a> + Default>(path: &PathBuf) -> T {
    if path.exists()
        && let Ok(text) = read_to_string(path)
        && let Ok(value) = toml::from_str(&text)
    {
        value
    } else {
        T::default()
    }
}

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

        let Some(settings_file) = cli_options.settings_file else {
            return Err(eyre!("No settings file specified, exiting."));
        };
        if !settings_file.exists() {
            return Err(eyre!("Settings file does not exist"));
        }
        let mut image = Image::load_from_file(&settings_file)?;
        image.optimization_level = OptLevel::AccuracyOptimized;
        let mut gpu_data = GPUData::init(
            &image.viewport,
            image.max_iter as usize,
            SharedState::new(device, queue),
            "cli renderer",
            Constants {
                iter_batch_size: 100_000,
            },
        );
        let now = Instant::now();
        fn status_callback(sm: StatusMessage) {
            match sm {
                corgi::types::StatusMessage::Progress(msg, percent) => {
                    println!("{:>6.2}% | {}", percent * 100.0, msg);
                    let _ = std::io::stdout().lock().flush();
                }
                corgi::types::StatusMessage::NewPreviewViewport(..) => todo!(),
                corgi::types::StatusMessage::NewOutputViewport(..) => todo!(),
            }
        }
        render_image(
            &mut gpu_data,
            &mut vec![],
            &image,
            None,
            std::sync::Arc::new(AtomicBool::new(false)),
            status_callback,
        );
        println!("Rendering took {:?}", Instant::now().duration_since(now));
        save_to_file(&gpu_data, &image, &path, status_callback);
        return Ok(());
    }

    // load context from storage
    let proj_dirs = ProjectDirs::from("com", "kiranwells", "corgi")
        .ok_or(eyre!("Failed to find configuration directory"))?;
    let config: Config = load_from_toml(&proj_dirs.config_dir().join("config.toml"));
    let theme: Theme = load_from_toml(&proj_dirs.config_dir().join("theme.toml"));
    let cache: Cache = load_from_toml(&proj_dirs.cache_dir().join("cache.toml"));
    let context = Context::new(config, cache, theme);

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
