/*!
# Worker Thread
*/
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, mpsc};

use corgi_lib::image_gen::{Constants, Engine, ProgressUpdate, SharedState};
use corgi_lib::types::ImgSpec;
use eframe::egui::ahash::{HashMap, HashMapExt};
use eframe::{egui, egui_wgpu, wgpu};
use parking_lot::RwLock;

use crate::app::StatusMessage;

/// Associated state for the worker thread
pub struct WorkerState {
    renderers: HashMap<RendererId, Engine>,
    command_channel: mpsc::Receiver<ImageGenCommand>,
    status_channel: mpsc::Sender<StatusMessage>,
    cancelled: Arc<AtomicBool>,
    ctx: egui::Context,
}

/// Message type for incoming commands to the worker thread
#[derive(Debug)]
pub enum ImageGenCommand {
    /// Render the given image spec in the given renderer
    Render(RendererId, Box<ImgSpec>),
    /// Update the constant settings passed to the given renderer
    UpdateConstants(RendererId, Constants),
    /// Save the image from the given renderer to the given path
    SaveToFile(RendererId, PathBuf),
    /// End the worker thread
    ShutDown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RendererId {
    Explore,
    Style,
    Render,
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
                        shared.clone(),
                        "Explore",
                        preview_settings.extents(),
                        preview_settings.location.max_iter as usize,
                        corgi_lib::image_gen::Constants {
                            iter_batch_size: context.config().ui_max_shader_batch_iters,
                        },
                        cancelled.clone(),
                    ),
                ),
                (
                    RendererId::Style,
                    Engine::init(
                        shared.clone(),
                        "Style",
                        preview_settings.extents(),
                        preview_settings.location.max_iter as usize,
                        corgi_lib::image_gen::Constants {
                            iter_batch_size: context.config().ui_max_shader_batch_iters,
                        },
                        cancelled.clone(),
                    ),
                ),
                (
                    RendererId::Render,
                    Engine::init(
                        shared.clone(),
                        "Render",
                        output_settings.extents(),
                        output_settings.location.max_iter as usize,
                        corgi_lib::image_gen::Constants {
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
                ImageGenCommand::UpdateConstants(id, c) => {
                    self.renderers.get_mut(&id).unwrap().update_constants(c);
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
                    Ok(ImageGenCommand::UpdateConstants(id, c)) => {
                        self.renderers.get_mut(&id).unwrap().update_constants(c);
                    }
                    Ok(ImageGenCommand::ShutDown) => return,
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => return,
                }
            }
            for (id, image) in render_commands {
                // if possible, copy cached probes from other renderers instead of
                // recalculating it
                if self
                    .renderers
                    .get_mut(&id)
                    .unwrap()
                    .get_probe_cache(&image)
                    .is_none()
                {
                    let mut cache = None;
                    for (id_inner, renderer) in self.renderers.iter() {
                        if id_inner == &id {
                            continue;
                        }
                        if let Some(cached) = renderer.get_probe_cache(&image) {
                            cache = Some(cached.to_vec());
                            break;
                        }
                    }
                    if let Some(cache) = cache {
                        self.renderers
                            .get_mut(&id)
                            .unwrap()
                            .pre_cache_probe(cache, &image);
                    }
                }

                let result = self
                    .renderers
                    .get_mut(&id)
                    .unwrap()
                    .render_image(&image, &mut |pu| {
                        let _ = self.status_channel.send(StatusMessage::Progress(pu));
                        self.ctx.request_repaint();
                    });
                match result {
                    Ok(timings) => {
                        let _ = self.status_channel.send(StatusMessage::RenderFinished(
                            id,
                            timings,
                            image.view(),
                        ));
                    }
                    Err(corgi_lib::image_gen::RenderingError::Cancelled) => {
                        let _ = self
                            .status_channel
                            .send(StatusMessage::Progress(ProgressUpdate {
                                message: "Generation Cancelled",
                                progress: None,
                            }));
                    }
                    Err(err) => {
                        let _ = self.status_channel.send(StatusMessage::Error(err.into()));
                    }
                }
                self.ctx.request_repaint();
            }
            for (id, path) in save_commands {
                if let Err(err) = self.renderers.get(&id).unwrap().save_to_file(
                    &path,
                    corgi_lib::types::serde::is_metadata_supported(&path),
                    &mut |pu| {
                        let _ = self.status_channel.send(StatusMessage::Progress(pu));
                        self.ctx.request_repaint();
                    },
                ) {
                    let _ = self.status_channel.send(StatusMessage::Error(err.into()));
                } else {
                    let _ = self
                        .status_channel
                        .send(StatusMessage::Progress(ProgressUpdate::msg(
                            "Image Save Complete",
                        )));
                }
            }
        }
    }

    pub fn texture(&self, id: RendererId) -> Arc<RwLock<wgpu::Texture>> {
        self.renderers.get(&id).unwrap().texture()
    }
}
