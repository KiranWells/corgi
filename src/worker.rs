use eframe::egui::mutex::RwLock;
use eframe::wgpu::{self};
use eframe::{egui, egui_wgpu};
use image::ImageBuffer;
use little_exif::exif_tag::ExifTag;
use little_exif::metadata::Metadata;
use nanoserde::SerJson;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::time::Instant;

use corgi::image_gen::{GPUData, SharedState, is_metadata_supported, render_image};
use corgi::types::{Image, ImageGenCommand, StatusMessage};

pub struct WorkerState {
    preview_state: GPUData,
    output_state: GPUData,
    probe_buffer: (Vec<[f32; 2]>, Vec<[f32; 2]>),
    preview_settings: Option<Image>,
    output_settings: Option<Image>,
    command_channel: mpsc::Receiver<ImageGenCommand>,
    status_channel: mpsc::Sender<StatusMessage>,
    cancelled: Arc<AtomicBool>,
    ctx: egui::Context,
}

impl WorkerState {
    /// Create state for a new worker thread to render images.
    pub fn new(
        wgpu: &egui_wgpu::RenderState,
        preview_settings: Image,
        output_settings: Image,
        recv: mpsc::Receiver<ImageGenCommand>,
        send: mpsc::Sender<StatusMessage>,
        cancelled: Arc<AtomicBool>,
        ctx: egui::Context,
    ) -> Self {
        let shared = SharedState::new(wgpu.device.clone(), wgpu.queue.clone());

        WorkerState {
            preview_state: GPUData::init(
                &preview_settings.viewport,
                preview_settings.max_iter as usize,
                shared.clone(),
                "Preview",
            ),
            output_state: GPUData::init(
                &output_settings.viewport,
                output_settings.max_iter as usize,
                shared,
                "Output",
            ),
            probe_buffer: (vec![], vec![]),
            preview_settings: None,
            output_settings: None,
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
            let mut new_preview = None;
            let mut new_output = None;
            let mut file_save = None;
            match msg {
                ImageGenCommand::NewPreviewSettings(image) => {
                    new_preview = Some(image);
                }
                ImageGenCommand::NewOutputSettings(image) => {
                    new_output = Some(image);
                }
                ImageGenCommand::SaveToFile(path) => {
                    file_save = Some(path);
                }
            }
            loop {
                let next = self.command_channel.try_recv();
                match next {
                    Ok(ImageGenCommand::NewPreviewSettings(image)) => {
                        new_preview = Some(image);
                    }
                    Ok(ImageGenCommand::NewOutputSettings(image)) => {
                        new_output = Some(image);
                    }
                    Ok(ImageGenCommand::SaveToFile(path)) => {
                        file_save = Some(path);
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => return,
                }
            }
            if let Some(image) = new_preview {
                let start = Instant::now();
                render_image(
                    &mut self.preview_state,
                    &mut self.probe_buffer,
                    &image,
                    self.preview_settings.as_ref(),
                    self.cancelled.clone(), // TODO: should this be the same cancel??
                    |sm| {
                        let _ = self.status_channel.send(sm);
                        self.ctx.request_repaint();
                    },
                );
                let _ = self.status_channel.send(StatusMessage::NewPreviewViewport(
                    Instant::now() - start,
                    image.viewport.clone(),
                ));
                self.preview_settings = Some(image);
                self.ctx.request_repaint();
            }
            if let Some(image) = new_output {
                let start = Instant::now();
                // TODO: move to separate thread
                render_image(
                    &mut self.output_state,
                    &mut self.probe_buffer,
                    &image,
                    self.output_settings.as_ref(),
                    self.cancelled.clone(),
                    |sm| {
                        let _ = self.status_channel.send(sm);
                        self.ctx.request_repaint();
                    },
                );
                let _ = self.status_channel.send(StatusMessage::NewOutputViewport(
                    Instant::now() - start,
                    image.viewport.clone(),
                ));
                self.output_settings = Some(image);
                self.ctx.request_repaint();
            }
            if let Some(path) = file_save {
                // copy output buffer to CPU
                if let Some(output_settings) = self.output_settings.as_ref() {
                    let _ = self
                        .status_channel
                        .send(StatusMessage::Progress("Fetching image data".into(), 0.0));
                    self.ctx.request_repaint();
                    if let Some(data) = self.output_state.get_texture_data() {
                        let _ = self
                            .status_channel
                            .send(StatusMessage::Progress("Saving image".into(), 0.0));
                        self.ctx.request_repaint();
                        let mut img = image::DynamicImage::ImageRgba8(
                            ImageBuffer::from_raw(
                                output_settings.viewport.width as u32,
                                output_settings.viewport.height as u32,
                                data,
                            )
                            .expect("image data to be properly formatted"),
                        );
                        img = image::DynamicImage::ImageRgb8(img.flipv().into_rgb8());
                        if let Err(err) = img.save(path.clone()) {
                            tracing::error!("Failed to save image: {err}");
                            let _ = self.status_channel.send(StatusMessage::Progress(
                                format!("Failed to save image: {err}"),
                                0.0,
                            ));
                            self.ctx.request_repaint();
                        } else {
                            // add metadata
                            if is_metadata_supported(&path) {
                                let mut meta = Metadata::new();
                                meta.set_tag(ExifTag::ImageDescription(
                                    self.output_settings.as_ref().unwrap().serialize_json(),
                                ));
                                meta.set_tag(ExifTag::Software("Corgi".into()));
                                if let Err(err) = meta.write_to_file(&path) {
                                    tracing::error!("Failed to write metadata to file: {err:?}");
                                }
                            }
                            let _ = self
                                .status_channel
                                .send(StatusMessage::Progress("Image save complete".into(), 100.0));
                            self.ctx.request_repaint();
                        }
                    }
                }
            }
        }
    }

    pub fn preview_texture(&self) -> Arc<RwLock<wgpu::Texture>> {
        self.preview_state.texture.clone()
    }

    pub fn output_texture(&self) -> Arc<RwLock<wgpu::Texture>> {
        self.output_state.texture.clone()
    }
}
