use std::{env, str::FromStr};

use color_eyre::{Result, eyre::eyre};
use corgi::app::CorgiApp;
use eframe::{egui, egui_wgpu, wgpu};
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

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

    // start app
    let eframe_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title("Corgi Fractal Renderer"),
        vsync: true,
        hardware_acceleration: eframe::HardwareAcceleration::Preferred,
        renderer: eframe::Renderer::Wgpu,
        wgpu_options: egui_wgpu::WgpuConfiguration {
            present_mode: wgpu::PresentMode::AutoVsync,
            desired_maximum_frame_latency: None,
            ..Default::default()
        },
        ..Default::default()
    };
    eframe::run_native("Corgi", eframe_options, Box::new(CorgiApp::new_dyn))
        .or(Err(eyre!("Error in eframe application")))?;
    Ok(())
}
