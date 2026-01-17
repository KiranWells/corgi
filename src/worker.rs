use std::sync::atomic::AtomicBool;
use std::sync::{Arc, mpsc};

use corgi::image_gen::{Engine, SharedState};
use corgi::types::{ImageGenCommand, ImgSpec, RenderResult, RendererId, StatusMessage};
use eframe::egui::ahash::{HashMap, HashMapExt};
use eframe::egui::mutex::RwLock;
use eframe::{egui, egui_wgpu, wgpu};

pub struct WorkerState {
    renderers: HashMap<RendererId, Engine>,
    command_channel: mpsc::Receiver<ImageGenCommand>,
    status_channel: mpsc::Sender<StatusMessage>,
    cancelled: Arc<AtomicBool>,
    ctx: egui::Context,
}

impl WorkerState {
    /// Create state for a new worker thread to render images.
    #[expect(clippy::too_many_arguments)]
    pub fn new(
        wgpu: &egui_wgpu::RenderState,
        preview_settings: ImgSpec,
        output_settings: ImgSpec,
        recv: mpsc::Receiver<ImageGenCommand>,
        send: mpsc::Sender<StatusMessage>,
        cancelled: Arc<AtomicBool>,
        ctx: egui::Context,
        context: &crate::Context,
    ) -> Self {
        let shared = SharedState::new(wgpu.device.clone(), wgpu.queue.clone());

        WorkerState {
            renderers: [
                (
                    RendererId::Explore,
                    Engine::init(
                        preview_settings.extents(),
                        preview_settings.location.max_iter as usize,
                        shared.clone(),
                        "Explore",
                        corgi::image_gen::Constants {
                            iter_batch_size: context.config().max_shader_batch_iters,
                        },
                        cancelled.clone(),
                    ),
                ),
                (
                    RendererId::Style,
                    Engine::init(
                        preview_settings.extents(),
                        preview_settings.location.max_iter as usize,
                        shared.clone(),
                        "Style",
                        corgi::image_gen::Constants {
                            iter_batch_size: context.config().max_shader_batch_iters,
                        },
                        cancelled.clone(),
                    ),
                ),
                (
                    RendererId::Render,
                    Engine::init(
                        output_settings.extents(),
                        output_settings.location.max_iter as usize,
                        shared.clone(),
                        "Render",
                        corgi::image_gen::Constants {
                            iter_batch_size: context.config().max_shader_batch_iters,
                        },
                        cancelled.clone(),
                    ),
                ),
            ]
            .into_iter()
            .collect(),
            command_channel: recv,
            status_channel: send,
            cancelled,
            ctx,
        }
    }

    /// Main entry point for the image generation process. This should be called in a separate thread,
    /// and will run until the given message channel is closed. `status` is used to communicate the
    /// current status of the render process to the main thread.
    pub fn run(&mut self) {
        while let Ok(msg) = self.command_channel.recv() {
            self.cancelled
                .store(false, std::sync::atomic::Ordering::Release);
            let mut render_commands = HashMap::new();
            let mut save_commands = HashMap::new();
            match msg {
                ImageGenCommand::Render(id, image) => {
                    render_commands.insert(id, image);
                }
                ImageGenCommand::SaveToFile(id, path) => {
                    save_commands.insert(id, path);
                }
                ImageGenCommand::ShutDown => return,
            }
            loop {
                let next = self.command_channel.try_recv();
                match next {
                    Ok(ImageGenCommand::Render(id, image)) => {
                        render_commands.insert(id, image);
                    }
                    Ok(ImageGenCommand::SaveToFile(id, path)) => {
                        save_commands.insert(id, path);
                    }
                    Ok(ImageGenCommand::ShutDown) => return,
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => return,
                }
            }
            for (id, image) in render_commands {
                let result = self
                    .renderers
                    .get_mut(&id)
                    .unwrap()
                    .render_image(&image, |pu| {
                        let _ = self.status_channel.send(StatusMessage::Progress(pu));
                        self.ctx.request_repaint();
                    });
                if let RenderResult::Finished(timings) = result {
                    let _ = self.status_channel.send(StatusMessage::RenderFinished(
                        id,
                        timings,
                        image.view(),
                    ));
                }
                self.ctx.request_repaint();
            }
            for (id, path) in save_commands {
                let _ = self.renderers.get(&id).unwrap().save_to_file(&path, |pu| {
                    let _ = self.status_channel.send(StatusMessage::Progress(pu));
                    self.ctx.request_repaint();
                });
            }
        }
    }

    pub fn texture(&self, id: RendererId) -> Arc<RwLock<wgpu::Texture>> {
        self.renderers.get(&id).unwrap().texture()
    }
}
