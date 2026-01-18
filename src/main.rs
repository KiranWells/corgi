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

use clap::Parser;
use color_eyre::Result;
use color_eyre::eyre::eyre;
use corgi::image_gen::{Constants, Engine, SharedState, get_device_and_queue};
use corgi::types::serde::SafeSaveLoad;
use corgi::types::{ImgSpec, OptLevel, ProgressUpdate};
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
        let mut image = ImgSpec::load(&settings_file)?;
        image.optimization_level = OptLevel::AccuracyOptimized;
        let mut engine = Engine::init(
            image.extents(),
            image.location.max_iter as usize,
            SharedState::new(device, queue),
            "cli renderer",
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
            &path,
            &mut status_callback,
            corgi::types::serde::is_metadata_supported(&path),
        )?;
        println!();
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
