use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use clap::Parser;
use corgi::types::{Debouncer, Image, ImageGenCommand, StatusMessage};
use wgpu::Extent3d;

use crate::config::Context;
use crate::ui::{CorgiUI, PreviewRenderResources};
use crate::worker::WorkerState;

/// Command line options for the application
#[derive(Parser, Debug)]
#[command(
    version,
    about,
    long_about = r"
Corgi - high-precision accelerated fractal renderer.

Corgi generates fractal images using high-precision calculation methods that
allow for super deep zooms. By default, Corgi will open a UI for exploring
fractals and rendering the selected locations. It also supports directly
rendering images given image settings defined in a JSON file."
)]
pub struct CorgiCliOptions {
    /// Optional image settings file to start with. Supported formats include
    /// JSON (.json or .corg) and image files containing the necessary metadata.
    pub settings_file: Option<PathBuf>,
    /// Optional output image location. If specified, Corgi will not launch a UI.
    /// If the image format supports metadata, the generations settings will be
    /// written into the finished file.
    #[arg(short, long, value_name = "FILE")]
    pub output_file: Option<PathBuf>,
}

/// The App State management struct
#[derive(Debug)]
pub struct CorgiApp {
    ui_state: CorgiUI,
    context: Context,
    last_save_time: Instant,
    command_channel: mpsc::Sender<ImageGenCommand>,
    status_channel: mpsc::Receiver<StatusMessage>,
    last_rendered: Image,
    previous_frame: Image,
    last_send_time: Instant,
    last_calc_time: Duration,
    debouncer: Debouncer,
}

impl CorgiApp {
    pub fn create(
        cc: &eframe::CreationContext<'_>,
        cli_options: CorgiCliOptions,
        context: Context,
    ) -> std::result::Result<Box<dyn eframe::App>, Box<dyn std::error::Error + Send + Sync>> {
        let wgpu = cc
            .wgpu_render_state
            .as_ref()
            .expect("Eframe must be launched with the wgpu backend");
        let (ui_send, worker_recv) = mpsc::channel::<ImageGenCommand>();
        let (worker_send, ui_recv) = mpsc::channel::<StatusMessage>();
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut initial_image = Image::default();
        let output_image = Image::default();
        let ctx = cc.egui_ctx.clone();
        eframe::egui::Visuals::default();
        ctx.set_style(context.theme().style());

        if let Some(image_file) = &cli_options.settings_file {
            initial_image = Image::load_from_file(image_file)?
        }

        egui_material_icons::initialize(&cc.egui_ctx);
        ctx.options_mut(|options| {
            options.max_passes = std::num::NonZeroUsize::new(1).unwrap();
        });

        let mut worker_state = WorkerState::new(
            wgpu,
            initial_image.clone(),
            output_image.clone(),
            worker_recv,
            worker_send,
            cancelled,
            ctx,
            &context,
        );
        let extents = Extent3d::from(&initial_image.viewport);
        let resources = PreviewRenderResources::init(
            &wgpu.device,
            wgpu.target_format,
            worker_state.preview_texture(),
            worker_state.output_texture(),
            (extents.width, extents.height),
            (
                output_image.viewport.width as u32,
                output_image.viewport.height as u32,
            ),
        )?;
        let ui_state = CorgiUI::new(&context, initial_image, ui_send.clone());

        wgpu.renderer.write().callback_resources.insert(resources);
        thread::spawn(move || {
            worker_state.run();
        });

        Ok(Box::new(CorgiApp {
            command_channel: ui_send,
            status_channel: ui_recv,
            debouncer: Debouncer::new(std::time::Duration::from_millis(300)),
            last_rendered: ui_state.image().clone(),
            previous_frame: ui_state.image().clone(),
            last_send_time: Instant::now(),
            last_calc_time: Duration::from_millis(16),
            ui_state,
            context,
            last_save_time: Instant::now(),
        }))
    }
}

impl eframe::App for CorgiApp {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        for msg in self.status_channel.try_iter() {
            match msg {
                StatusMessage::Progress(message, progress) => {
                    self.ui_state.status.message = message;
                    self.ui_state.status.progress = Some(progress);
                }
                StatusMessage::NewPreviewViewport(new_calc_time, viewport) => {
                    self.ui_state.status.message = "Finished rendering".into();
                    self.ui_state.status.progress = None;
                    self.ui_state.rendered_explore_viewport = viewport;
                    self.ui_state.swap = true;
                    // use a running average
                    self.last_calc_time = (self.last_calc_time + new_calc_time) / 2;
                    tracing::debug!(
                        "Ready for display in {:?}",
                        Instant::now() - self.last_send_time
                    );
                }
                StatusMessage::NewOutputViewport(calc_time, viewport) => {
                    self.ui_state.status.message = "Finished rendering output".into();
                    self.ui_state.status.progress = None;
                    self.ui_state.rendered_output_viewport = viewport.clone();
                    self.ui_state.output_preview_viewport = viewport;
                    self.ui_state.output_preview_viewport.zoom -= 1.0;
                    self.ui_state.swap = true;
                    tracing::debug!("Finished in {calc_time:?}");
                }
            }
        }
        self.ui_state.generate_ui(ctx, &mut self.context);
        let image = self.ui_state.image();
        //  sanity check on image size
        if !(image.viewport.width < 10
            || image.viewport.height < 10
            || image.viewport.width * image.viewport.height > 20_000_000)
        {
            // send the new image to the render thread, but only if
            // - the image is different
            // - the image has not changed for a full frame
            let mouse_down = ctx.input(|is| is.pointer.primary_down());
            if self.ui_state.has_active_viewport() && self.last_rendered != image {
                let diff = image.comp(&self.last_rendered);
                let calc_time = if diff.reprobe || diff.recompute {
                    self.last_calc_time
                } else {
                    // if the image just needs recoloring, we assume it will be fast
                    Duration::from_millis(1)
                };
                let do_send = match calc_time {
                    x if x < Duration::from_millis(30) => true,
                    x if x < Duration::from_millis(500) => {
                        image == self.previous_frame && !mouse_down
                    }
                    _ => {
                        self.debouncer.wait_time = (calc_time / 2).max(Duration::from_millis(500));
                        image == self.previous_frame && !mouse_down && self.debouncer.poll()
                    }
                };
                if do_send {
                    if self
                        .command_channel
                        .send(ImageGenCommand::NewPreviewSettings(image.clone()))
                        .is_ok()
                    {
                        self.last_send_time = Instant::now();
                        self.last_rendered = image.clone();
                        self.debouncer.reset();
                        if calc_time < Duration::from_millis(16) {
                            ctx.request_repaint();
                        }
                    } else {
                        tracing::warn!("Failed to send image update")
                    }
                } else {
                    if self.previous_frame != image {
                        self.debouncer.trigger();
                    }
                    // we need to force a re-check next frame
                    ctx.request_repaint();
                }
            }
            self.previous_frame = image;
        }
        if Instant::now() - self.last_save_time > Duration::from_secs(10) {
            self.context.save();
            self.last_save_time = Instant::now();
        }
    }
    fn on_exit(&mut self) {
        self.context.save();
    }
}
