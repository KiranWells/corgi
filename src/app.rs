use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use clap::Parser;
use corgi::types::serde::SafeSaveLoad;
use corgi::types::{
    Debouncer, ImageGenCommand, ImageTimings, ImgSpec, ProgressUpdate, StatusMessage,
};

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
    debouncer: ImgDebouncer,
    cancel_worker: std::sync::Arc<AtomicBool>,
    worker_handle: Option<std::thread::JoinHandle<()>>,
}

#[derive(Debug)]
struct ImgDebouncer {
    debouncer: Debouncer,
    last_rendered: ImgSpec,
    previous_frame: ImgSpec,
    timings: ImageTimings,
}

enum PollState {
    Trigger,
    Repoll,
    Inactive,
}

impl ImgDebouncer {
    pub fn new(initial_duration: Duration, image: ImgSpec) -> Self {
        Self {
            debouncer: Debouncer::new(initial_duration),
            last_rendered: image.clone(),
            previous_frame: image,
            timings: ImageTimings::default(),
        }
    }
    pub fn poll(&mut self, image: ImgSpec, mouse_down: bool) -> PollState {
        //  sanity check on image size
        if image.width < 10 || image.height < 10 || image.width * image.height > 20_000_000 {
            return PollState::Inactive;
        }
        // send the new image to the render thread, but only if
        // - the image is different
        // - the image has not changed for a full frame
        let poll_state = if self.last_rendered != image {
            let diff = image.comp(&self.last_rendered);
            let calc_time = self.timings.estimate_time(diff);
            let do_send = match calc_time {
                x if x < Duration::from_millis(30) => true,
                x if x < Duration::from_millis(500) => image == self.previous_frame && !mouse_down,
                _ => {
                    self.debouncer.wait_time = (calc_time / 2).max(Duration::from_millis(300));
                    image == self.previous_frame && !mouse_down && self.debouncer.poll()
                }
            };
            if do_send {
                self.debouncer.reset();
                self.last_rendered = image.clone();
                PollState::Trigger
            } else {
                self.debouncer.trigger();
                PollState::Repoll
            }
        } else {
            PollState::Inactive
        };
        self.previous_frame = image;
        poll_state
    }

    pub fn update_timings(&mut self, new_timings: &ImageTimings) {
        self.timings.merge(new_timings);
    }
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
        let mut initial_image = ImgSpec::default();
        let output_image = ImgSpec::default();
        let ctx = cc.egui_ctx.clone();
        eframe::egui::Visuals::default();
        ctx.set_style(context.theme().style());

        if let Some(image_file) = &cli_options.settings_file {
            initial_image = ImgSpec::load(image_file)?
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
            cancelled.clone(),
            ctx,
            &context,
        );
        let extents = initial_image.extents();
        let resources = PreviewRenderResources::init(
            &wgpu.device,
            wgpu.target_format,
            worker_state.texture(corgi::types::RendererId::Explore),
            worker_state.texture(corgi::types::RendererId::Style),
            worker_state.texture(corgi::types::RendererId::Render),
            (extents.width, extents.height),
            (output_image.width, output_image.height),
        )?;
        let ui_state = CorgiUI::new(&context, initial_image, ui_send.clone());

        wgpu.renderer.write().callback_resources.insert(resources);
        let handle = thread::spawn(move || {
            worker_state.run();
        });

        Ok(Box::new(CorgiApp {
            command_channel: ui_send,
            status_channel: ui_recv,
            debouncer: ImgDebouncer::new(
                std::time::Duration::from_millis(300),
                ui_state.image().clone(),
            ),
            ui_state,
            context,
            last_save_time: Instant::now(),
            cancel_worker: cancelled,
            worker_handle: Some(handle),
        }))
    }
}

impl eframe::App for CorgiApp {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        for msg in self.status_channel.try_iter() {
            match msg {
                StatusMessage::Progress(ProgressUpdate { message, progress }) => {
                    self.ui_state.status.message = message.into();
                    self.ui_state.status.progress = progress;
                }
                StatusMessage::RenderFinished(id, timings, viewport) => {
                    tracing::debug!("Image finished. {timings}");
                    self.ui_state.status.message = "Finished rendering".into();
                    self.ui_state.status.progress = None;
                    self.ui_state.swap = true;
                    self.debouncer.update_timings(&timings);
                    self.ui_state.update_rendered_view(id, viewport);
                }
                StatusMessage::Error(report) => {
                    tracing::warn!("Error in worker: {report}");
                    self.ui_state.status.message = report.to_string();
                    self.ui_state.status.progress = None;
                }
            }
        }
        self.ui_state.generate_ui(ctx, &mut self.context, || {
            self.cancel_worker
                .store(true, std::sync::atomic::Ordering::Relaxed)
        });
        if self.ui_state.has_active_viewport() {
            let image = self.ui_state.image();
            match self
                .debouncer
                .poll(image.clone(), ctx.input(|is| is.pointer.any_down()))
            {
                PollState::Trigger => {
                    self.cancel_worker
                        .store(true, std::sync::atomic::Ordering::Relaxed);

                    if let Err(err) = self.ui_state.send_render() {
                        tracing::warn!("Failed to send image update: {err}")
                    }
                }
                PollState::Repoll => {
                    // we need to force a re-check next frame
                    ctx.request_repaint();
                }
                PollState::Inactive => {}
            }
        }
        if Instant::now() - self.last_save_time > Duration::from_secs(10) {
            self.context.save();
            self.last_save_time = Instant::now();
        }
    }
    fn on_exit(&mut self) {
        self.context.save();
        let _ = self.command_channel.send(ImageGenCommand::ShutDown);
        let _ = self.worker_handle.take().unwrap().join();
    }
}
