/*!

# Corgi UI

This module contains the main UI state struct and its implementation, which
contains the code necessary to render the ui when it needs updating.


 */

use std::sync::{Arc, Mutex};

use egui::PointerButton;
use rug::{ops::PowAssign, Float};

use crate::types::{get_precision, Image, PreviewRenderResources, Status, Transform};

pub struct CorgiUI {
    image_settings: Image,
    status: Arc<Mutex<Status>>,
    x_text_buff: String,
    y_text_buff: String,
    x_probe_buff: String,
    y_probe_buff: String,
    previous_cursor_pos: Option<egui::Pos2>,
    setting_probe: bool,
    pub mouse_down: bool,
}

impl CorgiUI {
    pub fn new(status: Arc<Mutex<Status>>) -> Self {
        Self {
            image_settings: Image::default(),
            status,
            x_text_buff: String::from("-0.5"),
            y_text_buff: String::from("0.0"),
            x_probe_buff: String::from("-0.5"),
            y_probe_buff: String::from("0.0"),
            previous_cursor_pos: None,
            setting_probe: false,
            mouse_down: false,
        }
    }
    pub fn generate_ui(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("settings_panel").show(ctx, |ui| {
            ui.heading("Corgi");
            ui.separator();
            ui.label("Viewport");
            ui.horizontal(|ui| {
                ui.label("real offset");
                ui.add(egui::TextEdit::singleline(&mut self.x_text_buff));
            });
            ui.horizontal(|ui| {
                ui.label("imaginary offset");
                ui.add(egui::TextEdit::singleline(&mut self.y_text_buff));
            });
            // probe location
            ui.horizontal(|ui| {
                ui.label("probe real");
                ui.add(egui::TextEdit::singleline(&mut self.x_probe_buff));
            });
            ui.horizontal(|ui| {
                ui.label("probe imaginary");
                ui.add(egui::TextEdit::singleline(&mut self.y_probe_buff));
            });
            ui.button("Set probe")
                .clicked()
                .then(|| self.setting_probe = !self.setting_probe);
            ui.add(
                egui::Slider::new(&mut self.image_settings.viewport.zoom, -2.0..=500.0)
                    .text("Zoom"),
            );
            ui.add(
                egui::Slider::new(&mut self.image_settings.max_iter, 100..=100000)
                    .text("Max iterations"),
            );
            ui.separator();
            ui.label("Coloring");
            ui.add(
                egui::Slider::new(&mut self.image_settings.coloring.saturation, 0.0..=2.0)
                    .text("Saturation"),
            );
            ui.add(
                egui::Slider::new(
                    &mut self.image_settings.coloring.color_frequency,
                    0.0..=10.0,
                )
                .text("Color frequency"),
            );
            ui.add(
                egui::Slider::new(&mut self.image_settings.coloring.color_offset, 0.0..=1.0)
                    .text("Color offset"),
            );
            ui.add(
                egui::Slider::new(&mut self.image_settings.coloring.glow_spread, -10.0..=10.0)
                    .text("Glow spread"),
            );
            ui.add(
                egui::Slider::new(&mut self.image_settings.coloring.glow_intensity, 0.0..=10.0)
                    .text("Glow intensity"),
            );
            ui.add(
                egui::Slider::new(&mut self.image_settings.coloring.brightness, 0.0..=2.0)
                    .text("Brightness"),
            );
            ui.add(
                egui::Slider::new(
                    &mut self.image_settings.coloring.internal_brightness,
                    0.0..=1000.0,
                )
                .text("Internal brightness"),
            );
            ui.separator();
            ui.add(
                egui::Slider::new(&mut self.image_settings.misc, -1000.0..=1000.0)
                    .text("Debug parameter"),
            );
            ui.separator();
            ui.label("Status");
            ui.label(format!("Status: {:?}", self.status.lock().unwrap().message));
            if let Some(progress) = self.status.lock().unwrap().progress {
                ui.add(egui::ProgressBar::new(progress as f32));
            }
        });

        let precision = get_precision(self.image().viewport.zoom);
        if let Ok(res) = Float::parse(&self.x_text_buff) {
            self.image_settings.viewport.x = Float::with_val(precision, res)
        }
        if let Ok(res) = Float::parse(&self.y_text_buff) {
            self.image_settings.viewport.y = Float::with_val(precision, res)
        }
        // probe
        if let Ok(res) = Float::parse(&self.x_probe_buff) {
            self.image_settings.probe_location.0 = Float::with_val(precision, res)
        }
        if let Ok(res) = Float::parse(&self.y_probe_buff) {
            self.image_settings.probe_location.1 = Float::with_val(precision, res)
        }

        let status = self.status.clone();
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::Frame::canvas(ui.style()).show(ui, |ui| {
                let size = ui.available_size();
                let (_id, rect) = ui.allocate_space(size);

                let pointer_in_rect = ui.rect_contains_pointer(rect);

                self.mouse_down =
                    ctx.input().pointer.button_down(PointerButton::Primary) && pointer_in_rect;
                if pointer_in_rect {
                    if self.setting_probe {
                        if ui.input().pointer.button_clicked(PointerButton::Primary) {
                            if let Some(pos) = ui.input().pointer.interact_pos() {
                                let (x, y) = self
                                    .image_settings
                                    .viewport
                                    .get_real_coords((pos.x) as f64, (size.y - pos.y) as f64);
                                self.image_settings.probe_location = (x.clone(), y.clone());
                                self.x_probe_buff = x.to_string_radix(10, None);
                                self.y_probe_buff = y.to_string_radix(10, None);
                                self.setting_probe = false;
                            }
                        }
                    } else {
                        // get scroll and drag inputs to change the viewport
                        let scroll = ui.input().scroll_delta;
                        let pixel_scale = ui.input().pixels_per_point;
                        let mut drag = egui::Vec2::new(0.0, 0.0);
                        if let Some(new_pos) = ui.input().pointer.interact_pos() {
                            if let Some(old_pos) = self.previous_cursor_pos {
                                drag = new_pos - old_pos;
                            }
                            if ui.input().pointer.button_down(PointerButton::Primary) {
                                self.previous_cursor_pos = Some(new_pos)
                            } else {
                                self.previous_cursor_pos = None;
                            }
                        } else {
                            self.previous_cursor_pos = None;
                        }
                        let precision = get_precision(self.image_settings.viewport.zoom);
                        let mut scale = Float::with_val(precision, 2.0);
                        scale.pow_assign(-self.image_settings.viewport.zoom);
                        self.image_settings.viewport.x -= (drag.x as f64
                            / self.image_settings.viewport.width as f64
                            * pixel_scale as f64
                            * 2.0)
                            * scale.clone();
                        self.image_settings.viewport.y += (drag.y as f64
                            / self.image_settings.viewport.height as f64
                            * pixel_scale as f64
                            * 2.0)
                            * scale;
                        self.x_text_buff = self.image_settings.viewport.x.to_string_radix(10, None);
                        self.y_text_buff = self.image_settings.viewport.y.to_string_radix(10, None);
                        self.image_settings.viewport.zoom += scroll.y as f64 * 0.002;
                    }
                }

                self.image_settings.viewport.width = size.x as usize;
                self.image_settings.viewport.height = size.y as usize;
                // The callback function for WGPU is in two stages: prepare, and paint.
                //
                // The prepare callback is called every frame before paint and is given access to the wgpu
                // Device and Queue, which can be used, for instance, to update buffers and uniforms before
                // rendering.
                //
                // The paint callback is called after prepare and is given access to the render pass, which
                // can be used to issue draw commands.
                let view_ref = self.image_settings.viewport.clone();
                let cb = egui_wgpu::CallbackFn::new()
                    .prepare(move |device, queue, _encoder, type_map| {
                        let res = type_map.get_mut::<PreviewRenderResources>().unwrap();

                        let transforms = if let Some(viewport) = {
                            status
                                .lock()
                                .unwrap()
                                .rendered_image
                                .as_ref()
                                .map(|x| &x.viewport)
                        } {
                            let size = (viewport.width, viewport.height);
                            if size != *res.size() {
                                res.resize(device, size);
                            }
                            viewport.transforms_from(&view_ref)
                        } else {
                            Transform::default()
                        };

                        res.prepare(device, queue, transforms);
                        Vec::new()
                    })
                    .paint(move |_info, rpass, type_map| {
                        type_map
                            .get::<PreviewRenderResources>()
                            .unwrap()
                            .paint(rpass);
                    });

                let callback = egui::PaintCallback {
                    rect,
                    callback: Arc::new(cb),
                };

                ui.painter().add(callback);
            });
        });
    }

    pub fn image(&self) -> &Image {
        &self.image_settings
    }
}
