use clap::Parser;
use eframe::egui::mutex::Mutex;
use nanoserde::DeJson;
use std::fs::read_to_string;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crate::image_gen;
use crate::types::Debouncer;
use crate::{
    types::{Image, PreviewRenderResources, Status},
    ui::CorgiUI,
};

#[derive(Parser)]
#[command(version, about, long_about = "../README.md")]
pub struct CorgiCliOptions {
    /// Optional image settings file to start with
    #[arg(short, long, value_name = "FILE")]
    image_file: Option<PathBuf>,
}

/// The App State management struct
pub struct CorgiApp {
    ui_state: CorgiUI,
    sender: mpsc::Sender<Image>,
    last_rendered: Image,
    debouncer: Debouncer,
}

impl CorgiApp {
    pub fn new_dyn(
        cc: &eframe::CreationContext<'_>,
        cli_options: CorgiCliOptions,
    ) -> std::result::Result<Box<dyn eframe::App>, Box<dyn std::error::Error + Send + Sync>> {
        let wgpu = cc
            .wgpu_render_state
            .as_ref()
            .expect("Eframe must be launched with the wgpu backend");
        let status = Arc::new(Mutex::new(Status::default()));
        let (sender, receiver) = mpsc::channel::<Image>();
        let mut initial_image = Image::default();
        if let Some(image_file) = &cli_options.image_file {
            initial_image = Image::deserialize_json(read_to_string(image_file)?.as_str())?
        }
        let render_gpu_data = image_gen::GPUData::init(&initial_image, wgpu);
        let render_thread_status = status.clone();
        let resources = PreviewRenderResources::init(
            &wgpu.device,
            wgpu.target_format,
            render_gpu_data.rendered_image.clone(),
            (0, 0),
        )?;
        wgpu.renderer.write().callback_resources.insert(resources);
        let ctx = cc.egui_ctx.clone();
        thread::spawn(move || {
            image_gen::render_thread(receiver, render_thread_status, render_gpu_data, ctx)
        });
        let ui_state = CorgiUI::new(status, initial_image);
        Ok(Box::new(CorgiApp {
            sender,
            debouncer: Debouncer::new(std::time::Duration::from_millis(16)),
            last_rendered: ui_state.image().clone(),
            ui_state,
        }))
    }
}

impl eframe::App for CorgiApp {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        self.ui_state.generate_ui(ctx);
        let image = self.ui_state.image().clone();
        //  sanity check on image size
        if !(image.viewport.width < 10
            || image.viewport.height < 10
            || image.viewport.width * image.viewport.height > 20_000_000
            || self.ui_state.mouse_down())
        {
            // send the new image to the render thread, but only if
            // - the zoom level has not changed OR
            // - the image zoom level did change in the past and the debouncer delay has passed
            //   (meaning the user has stopped zooming)
            if &self.last_rendered != self.ui_state.image() {
                if image.viewport.zoom == self.last_rendered.viewport.zoom {
                    if self.sender.send(image.clone()).is_ok() {
                        self.debouncer.reset();
                    } else {
                        tracing::warn!("Failed to send image update")
                    }
                } else {
                    // if the zoom level changed, we need to debounce the input
                    self.debouncer.trigger();
                    ctx.request_repaint_after(self.debouncer.remaining().unwrap());
                }
                self.last_rendered = image;
            } else if self.debouncer.poll() {
                // the zoom level previously changed, and the debouncer delay has passed
                if self.sender.send(self.ui_state.image().clone()).is_err() {
                    tracing::warn!("Failed to send image update")
                }
            } else if self.debouncer.active() {
                ctx.request_repaint_after(
                    self.debouncer
                        .remaining()
                        .unwrap_or(Duration::from_millis(16)),
                );
            }
        }
    }
}
