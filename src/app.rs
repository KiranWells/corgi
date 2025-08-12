use clap::Parser;
use nanoserde::DeJson;
use std::fs::read_to_string;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::thread;

use crate::image_gen::WorkerState;
use crate::types::Message;
use crate::types::StatusMessage;
use crate::{
    types::{Image, PreviewRenderResources},
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
    send: mpsc::Sender<Message>,
    recv: mpsc::Receiver<StatusMessage>,
    last_rendered: Image,
    previous_frame: Image,
    // TODO: add debouncing on all 'recalculate' functions
    // with a dynamic delay based on recalculate cost
    // debouncer: Debouncer,
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
        let (ui_send, worker_recv) = mpsc::channel::<Message>();
        let (worker_send, ui_recv) = mpsc::channel::<StatusMessage>();
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut initial_image = Image::default();
        let output_image = Image::default();
        if let Some(image_file) = &cli_options.image_file {
            initial_image = Image::deserialize_json(read_to_string(image_file)?.as_str())?
        }
        let ctx = cc.egui_ctx.clone();
        let mut worker_state = WorkerState::new(
            wgpu,
            initial_image.clone(),
            output_image,
            worker_recv,
            worker_send,
            cancelled,
            ctx,
        );
        let resources = PreviewRenderResources::init(
            &wgpu.device,
            wgpu.target_format,
            worker_state.preview_texture(),
            worker_state.output_texture(),
            (initial_image.viewport.width, initial_image.viewport.height),
        )?;
        wgpu.renderer.write().callback_resources.insert(resources);
        thread::spawn(move || {
            worker_state.run();
        });
        let ui_state = CorgiUI::new(initial_image);
        Ok(Box::new(CorgiApp {
            send: ui_send,
            recv: ui_recv,
            // debouncer: Debouncer::new(std::time::Duration::from_millis(16)),
            last_rendered: ui_state.image().clone(),
            previous_frame: ui_state.image().clone(),
            ui_state,
        }))
    }
}

impl eframe::App for CorgiApp {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        let image = self.ui_state.image().clone();
        for msg in self.recv.try_iter() {
            match msg {
                StatusMessage::Progress(message, progress) => {
                    self.ui_state.status.message = message;
                    self.ui_state.status.progress = Some(progress);
                }
                StatusMessage::NewPreviewViewport(viewport) => {
                    self.ui_state.status.message = "Finished rendering".into();
                    self.ui_state.status.progress = None;
                    self.ui_state.rendered_viewport = viewport;
                    self.ui_state.swap = true;
                }
            }
        }
        self.ui_state.generate_ui(ctx);
        //  sanity check on image size
        if !(image.viewport.width < 10
            || image.viewport.height < 10
            || image.viewport.width * image.viewport.height > 20_000_000)
        {
            // send the new image to the render thread, but only if
            // - the image is different
            // - the image has not changed for a full frame
            if &self.last_rendered != self.ui_state.image() {
                if self.ui_state.image() == &self.previous_frame && !self.ui_state.mouse_down() {
                    if self
                        .send
                        .send(Message::NewPreviewSettings(image.clone()))
                        .is_ok()
                    {
                        self.last_rendered = image;
                    } else {
                        tracing::warn!("Failed to send image update")
                    }
                } else {
                    self.previous_frame = self.ui_state.image().clone();
                    // we need to force a re-check next frame
                    ctx.request_repaint();
                }
            }
        }
    }
}
