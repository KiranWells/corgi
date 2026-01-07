use std::sync::atomic::AtomicBool;
use std::sync::{Arc, mpsc};

use corgi::image_gen::{GPUData, SharedState, render_image, save_to_file};
use corgi::types::{Image, ImageGenCommand, RenderResult, RendererId, StatusMessage};
use eframe::egui::ahash::{HashMap, HashMapExt};
use eframe::egui::mutex::RwLock;
use eframe::{egui, egui_wgpu, wgpu};

pub struct WorkerState {
    renderers: HashMap<RendererId, GPUData>,
    last_images: HashMap<RendererId, Image>,
    probe_buffer: Vec<[f32; 2]>,
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
        preview_settings: Image,
        output_settings: Image,
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
                    GPUData::init(
                        preview_settings.extents(),
                        preview_settings.parameters.max_iter as usize,
                        shared.clone(),
                        "Explore",
                        corgi::image_gen::Constants {
                            iter_batch_size: context.config().max_shader_batch_iters,
                        },
                    ),
                ),
                (
                    RendererId::Style,
                    GPUData::init(
                        preview_settings.extents(),
                        preview_settings.parameters.max_iter as usize,
                        shared.clone(),
                        "Style",
                        corgi::image_gen::Constants {
                            iter_batch_size: context.config().max_shader_batch_iters,
                        },
                    ),
                ),
                (
                    RendererId::Render,
                    GPUData::init(
                        output_settings.extents(),
                        output_settings.parameters.max_iter as usize,
                        shared.clone(),
                        "Render",
                        corgi::image_gen::Constants {
                            iter_batch_size: context.config().max_shader_batch_iters,
                        },
                    ),
                ),
            ]
            .into_iter()
            .collect(),
            last_images: HashMap::new(),
            probe_buffer: vec![],
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
                let result = render_image(
                    self.renderers.get_mut(&id).unwrap(),
                    &mut self.probe_buffer,
                    &image,
                    self.last_images.get(&id),
                    self.cancelled.clone(),
                    |sm| {
                        let _ = self.status_channel.send(sm);
                        self.ctx.request_repaint();
                    },
                );
                if let RenderResult::Finished(timings) = result {
                    let _ = self.status_channel.send(StatusMessage::RenderFinished(
                        id,
                        timings,
                        image.view(),
                    ));
                    self.last_images.insert(id, *image);
                }
                self.ctx.request_repaint();
            }
            for (id, path) in save_commands {
                if let Some(image_settings) = self.last_images.get(&id) {
                    save_to_file(
                        self.renderers.get(&id).unwrap(),
                        image_settings,
                        &path,
                        |sm| {
                            let _ = self.status_channel.send(sm);
                            self.ctx.request_repaint();
                        },
                    );
                }
            }
        }
    }

    pub fn texture(&self, id: RendererId) -> Arc<RwLock<wgpu::Texture>> {
        self.renderers.get(&id).unwrap().texture.clone()
    }
}
