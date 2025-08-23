use std::{env, str::FromStr};

use clap::Parser;
use color_eyre::Result;
use corgi::app::{CorgiApp, CorgiCliOptions};
use eframe::{egui, egui_wgpu, wgpu};
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
