/*!

# Corgi UI

This module contains the main UI state struct and its implementation, which
contains the code necessary to update internal state and render the ui.
 */

use eframe::egui::{Button, Color32, ScrollArea, Sense, Stroke, UiBuilder, Vec2};
use eframe::{egui, egui_wgpu};
use egui_taffy::{TuiBuilderLogic, tui};
use rug::{Float, ops::PowAssign};
use std::sync::mpsc;
use taffy::{Overflow, prelude::*};

use crate::types::{
    Coloring2, ComplexPoint, Image, Message, PaintCallback, Status, Viewport, get_precision,
};
use utils::{collapsible, input_with_label, point_edit};

mod coloring;
mod utils;

pub trait EditUI {
    fn render_edit_ui(&mut self, ctx: &egui::Context, tui: &mut egui_taffy::Tui);
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ViewState {
    Viewport,
    OutputView,
    OutputLock,
    Output,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum UITab {
    Explore,
    Color,
    Render,
}

/// The main UI state struct.
pub struct CorgiUI {
    image_settings: Image,
    pub status: Status,
    pub rendered_viewport: Viewport,
    pub rendered_output_viewport: Viewport,
    pub output_viewport: Viewport,
    output_preview_viewport: Viewport,
    view_state: ViewState,
    render_zoom_offset: f64,
    setting_probe: bool,
    mouse_down: bool,
    pub swap: bool,
    send: mpsc::Sender<Message>,
    tab: UITab,
    optimized_settings: Image,
}

impl CorgiUI {
    /// Create a new state struct; status should be shared with the render thread.
    pub fn new(image: Image, send: mpsc::Sender<Message>) -> Self {
        let default_output_viewport = Viewport {
            width: 1920,
            height: 1080,
            scaling: image.viewport.scaling,
            zoom: image.viewport.zoom,
            center: image.viewport.center.clone(),
        };
        Self {
            status: Status::default(),
            rendered_viewport: image.viewport.clone(),
            rendered_output_viewport: default_output_viewport.clone(),
            output_viewport: default_output_viewport.clone(),
            output_preview_viewport: default_output_viewport.clone(),
            view_state: ViewState::Viewport,
            render_zoom_offset: -1.0,
            image_settings: image,
            setting_probe: false,
            mouse_down: false,
            swap: false,
            send,
            tab: UITab::Explore,
            optimized_settings: Image {
                viewport: Viewport {
                    scaling: 0.5,
                    ..Default::default()
                },
                external_coloring: Coloring2::external_opt_default(),
                internal_coloring: Coloring2::internal_opt_default(),
                ..Default::default()
            },
        }
    }

    /// Generate the UI and handle any events. This function will do some blocking
    /// to access shared data
    pub fn generate_ui(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("settings_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.menu_button("=", |ui| {
                    if ui.add(Button::new("Save Image Settings")).clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .set_file_name("saved_fractal.corg")
                            .add_filter("corg", &["corg"])
                            .save_file()
                        {
                            // write to file
                            match self.image_settings.save_to_file(&path) {
                                Err(err) => {
                                    self.status.message =
                                        format!("Failed to save image settings: {err:?}")
                                }
                                Ok(_) => self.status.message = "Saved settings".to_string(),
                            }
                        }
                    }
                    if ui.add(Button::new("Load Image Settings")).clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("corg", &["corg"])
                            .add_filter("image with metadata", &["jpg", "jpeg", "webp", "png"])
                            .pick_file()
                        {
                            match Image::load_from_file(&path) {
                                Ok(image) => {
                                    self.image_settings = image;
                                }
                                Err(err) => {
                                    self.status.message =
                                        format!("Failed to load image settings: {err:?}")
                                }
                            }
                        }
                    }
                });
                ui.separator();
                ui.selectable_value(&mut self.tab, UITab::Explore, "Explore");
                ui.selectable_value(&mut self.tab, UITab::Color, "Color");
                ui.selectable_value(&mut self.tab, UITab::Render, "Render");
            });
            ScrollArea::vertical().show(ui, |ui| {
                tui(ui, ui.id().with("side"))
                    .reserve_width(250.0)
                    .style(taffy::Style {
                        flex_direction: taffy::FlexDirection::Column,
                        size: percent(1.0),
                        align_items: Some(AlignItems::Stretch),
                        justify_content: Some(AlignContent::Start),
                        gap: length(8.),
                        overflow: taffy::Point {
                            x: Overflow::Hidden,
                            y: Overflow::Scroll,
                        },
                        ..Default::default()
                    })
                    .show(|tui| match self.tab {
                        UITab::Explore => {
                            collapsible(tui, "Viewport", |tui| {
                                point_edit(
                                    tui,
                                    "Image Center",
                                    get_precision(self.image().viewport.zoom),
                                    &mut self.image_settings.viewport.center,
                                );
                                point_edit(
                                    tui,
                                    "Probe Location",
                                    get_precision(self.image().viewport.zoom),
                                    &mut self.image_settings.probe_location,
                                );
                                tui.ui_add(Button::new("Set probe"))
                                    .clicked()
                                    .then(|| self.setting_probe = !self.setting_probe);
                                input_with_label(
                                    tui,
                                    "Zoom",
                                    egui::DragValue::new(&mut self.image_settings.viewport.zoom)
                                        .speed(0.03)
                                        .update_while_editing(false),
                                );
                                input_with_label(
                                    tui,
                                    "Max iteration",
                                    egui::DragValue::new(&mut self.image_settings.max_iter)
                                        .speed(100.0)
                                        .range(100..=u32::MAX)
                                        .update_while_editing(false),
                                );
                                let mut scaling =
                                    (1.0 / self.optimized_settings.viewport.scaling) as u32;
                                input_with_label(
                                    tui,
                                    "Viewport Downscaling",
                                    egui::DragValue::new(&mut scaling)
                                        .speed(0.01)
                                        .range(1..=8)
                                        .update_while_editing(false),
                                );
                                self.optimized_settings.viewport.scaling = 1.0 / scaling as f64;
                            });
                            collapsible(tui, "Camera", |tui| {
                                if tui
                                    .enabled_ui(self.view_state == ViewState::Viewport)
                                    .ui_add(Button::new("Set Camera to View"))
                                    .clicked()
                                {
                                    self.output_viewport.center =
                                        self.image_settings.viewport.center.clone();
                                    self.output_viewport.zoom =
                                        self.image_settings.viewport.zoom + 0.5;
                                    self.render_zoom_offset = -0.5;
                                    if self.view_state == ViewState::Viewport {
                                        self.view_state = ViewState::OutputView;
                                    }
                                }
                                if tui
                                    .ui_add(Button::new(
                                        if self.view_state == ViewState::Viewport {
                                            "Preview Camera"
                                        } else {
                                            "Exit Preview"
                                        },
                                    ))
                                    .clicked()
                                {
                                    if self.view_state == ViewState::Viewport {
                                        self.view_state = ViewState::OutputView;
                                    } else {
                                        self.view_state = ViewState::Viewport;
                                    }
                                }
                                if tui
                                    .ui_add(Button::new(
                                        if self.view_state != ViewState::OutputLock {
                                            "Lock Camera to View"
                                        } else {
                                            "Unlock Camera"
                                        },
                                    ))
                                    .clicked()
                                {
                                    if self.view_state == ViewState::OutputLock {
                                        self.view_state = ViewState::OutputView;
                                    } else {
                                        if self.view_state == ViewState::Viewport {
                                            self.output_viewport.center =
                                                self.image_settings.viewport.center.clone();
                                            self.output_viewport.zoom =
                                                self.image_settings.viewport.zoom + 0.5;
                                            self.render_zoom_offset = -0.5;
                                        }
                                        self.view_state = ViewState::OutputLock;
                                    }
                                }
                                input_with_label(
                                    tui,
                                    "Camera width",
                                    egui::DragValue::new(&mut self.output_viewport.width)
                                        .speed(10.0),
                                );
                                input_with_label(
                                    tui,
                                    "Camera height",
                                    egui::DragValue::new(&mut self.output_viewport.height)
                                        .speed(10.0),
                                );
                            });
                        }
                        UITab::Color => {
                            collapsible(tui, "External", |tui| {
                                self.image_settings
                                    .external_coloring
                                    .render_edit_ui(ctx, tui);
                            });
                            collapsible(tui, "Internal", |tui| {
                                self.image_settings
                                    .internal_coloring
                                    .render_edit_ui(ctx, tui);
                            });
                        }
                        UITab::Render => {
                            input_with_label(
                                tui,
                                "Camera width",
                                egui::DragValue::new(&mut self.output_viewport.width).speed(10.0),
                            );
                            input_with_label(
                                tui,
                                "Camera height",
                                egui::DragValue::new(&mut self.output_viewport.height).speed(10.0),
                            );
                            if tui.ui_add(Button::new("Render")).clicked() {
                                let mut image = self.image_settings.clone();
                                image.viewport = self.output_viewport.clone();
                                let _ = self.send.send(Message::NewOutputSettings(image));
                            }
                            if tui.ui_add(Button::new("Save to file")).clicked() {
                                if let Some(path) = rfd::FileDialog::new()
                                    .add_filter(
                                        "image with metadata",
                                        &["jpg", "jpeg", "webp", "png"],
                                    )
                                    .add_filter(
                                        "image without metadata",
                                        &["avif", "gif", "qoi", "tiff", "exr"],
                                    )
                                    .set_file_name("fractal.png")
                                    .save_file()
                                {
                                    let _ = self.send.send(Message::SaveToFile(path));
                                }
                            }
                        }
                    });
            });
        });
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(ctx.style().visuals.window_fill))
            .show(ctx, |ui| {
                let mut new_max_rect = ui.max_rect();
                new_max_rect.set_height(new_max_rect.height() - 20.0);
                ui.scope_builder(
                    UiBuilder::new().sense(Sense::drag()).max_rect(new_max_rect),
                    |ui| {
                        let size = ui.available_size();
                        let (_id, rect) = ui.allocate_space(size);

                        // handle mouse events

                        // get input beforehand
                        let pointer_in_rect = ui.rect_contains_pointer(rect);
                        let (primary_down, pointer_pos) = ctx.input(|i| {
                            (
                                i.pointer.button_down(egui::PointerButton::Primary),
                                i.pointer.interact_pos(),
                            )
                        });

                        let view_image = self.image();
                        // update image settings
                        let response = ui.response();
                        if self.setting_probe {
                            // probe setting mode, set the probe location to the mouse position
                            // on click
                            if primary_down && pointer_in_rect {
                                if let Some(pos) = pointer_pos {
                                    let (x, y) = view_image
                                        .viewport
                                        .get_real_coords((pos.x) as f64, (size.y - pos.y) as f64);
                                    self.image_settings.probe_location = ComplexPoint { x, y };
                                    self.setting_probe = false;
                                }
                            }
                        } else {
                            // get scroll and drag inputs to change the viewport
                            let (mut scroll, pixel_scale) =
                                ui.input(|i| (i.smooth_scroll_delta, i.pixels_per_point));
                            if !pointer_in_rect {
                                scroll = Vec2::ZERO;
                            }
                            let drag = response.drag_delta();

                            // scroll
                            let precision = get_precision(view_image.viewport.zoom);
                            let mut scale = Float::with_val(precision, 2.0);
                            scale.pow_assign(-view_image.viewport.zoom);
                            let aspect_scale = view_image.viewport.aspect_scale();
                            let x_offset = -(drag.x as f64
                            / view_image.viewport.width as f64
                            * aspect_scale.x as f64
                            * pixel_scale as f64
                            * 1.715) // TODO: why this value? and does this work on other screens?
                            * scale.clone();
                            let y_offset = (drag.y as f64 / view_image.viewport.height as f64
                                * aspect_scale.y as f64
                                * pixel_scale as f64
                                * 1.715)
                                * scale;
                            match if self.tab == UITab::Render {
                                ViewState::Output
                            } else {
                                self.view_state
                            } {
                                ViewState::Viewport => {
                                    self.image_settings.viewport.center.x += x_offset;
                                    self.image_settings.viewport.center.y += y_offset;
                                    self.image_settings.viewport.zoom +=
                                        scroll.y as f64 * pixel_scale as f64 * 0.005;
                                }
                                ViewState::OutputView => {
                                    if drag.x != 0.0 || drag.y != 0.0 {
                                        self.image_settings.viewport.center.x =
                                            self.output_viewport.center.x.clone() + x_offset;
                                        self.image_settings.viewport.center.y =
                                            self.output_viewport.center.y.clone() + y_offset;
                                        self.image_settings.viewport.zoom =
                                            view_image.viewport.zoom;
                                        self.view_state = ViewState::Viewport;
                                    }
                                    self.render_zoom_offset +=
                                        scroll.y as f64 * pixel_scale as f64 * 0.005;
                                }
                                ViewState::OutputLock => {
                                    self.output_viewport.center.x += x_offset;
                                    self.output_viewport.center.y += y_offset;
                                    self.output_viewport.zoom +=
                                        scroll.y as f64 * pixel_scale as f64 * 0.005;
                                }
                                ViewState::Output => {
                                    self.output_preview_viewport.center.x += x_offset;
                                    self.output_preview_viewport.center.y += y_offset;
                                    self.output_preview_viewport.zoom +=
                                        scroll.y as f64 * pixel_scale as f64 * 0.005;
                                }
                            }
                        }

                        self.image_settings.viewport.width = size.x as usize;
                        self.image_settings.viewport.height = size.y as usize;

                        // render the image

                        let view_image = self.image();
                        let mut render_rect = rect.scale_from_center2(
                            egui::Vec2::splat(1.0) / view_image.viewport.aspect_scale(),
                        );
                        let (x, y) = view_image.viewport.coords_to_px_offset(
                            &self.output_viewport.center.x,
                            &self.output_viewport.center.y,
                        );
                        render_rect = render_rect.translate(Vec2::new(x as f32, -y as f32));
                        render_rect = render_rect.scale_from_center(f32::powf(
                            2.0,
                            -(self.output_viewport.zoom - view_image.viewport.zoom) as f32,
                        ));
                        render_rect =
                            render_rect.scale_from_center2(self.output_viewport.aspect_scale());
                        let cb = PaintCallback {
                            rendered_viewport: if self.tab == UITab::Render {
                                self.rendered_output_viewport.clone()
                            } else {
                                self.rendered_viewport.clone()
                            },
                            view: view_image.viewport,
                            swap: self.swap,
                            output: self.tab == UITab::Render,
                        };
                        self.swap = false;

                        let callback = egui_wgpu::Callback::new_paint_callback(rect, cb);

                        ui.painter().add(callback);
                        if self.tab != UITab::Render {
                            ui.painter().rect_stroke(
                                render_rect.intersect(rect),
                                0.0,
                                Stroke::new(2.0, Color32::from_gray(255)),
                                egui::StrokeKind::Outside,
                            );
                        }
                    },
                );
                ui.horizontal_centered(|ui| {
                    ui.scope_builder(
                        UiBuilder::new().max_rect(ui.max_rect().with_max_x(100.0)),
                        |ui| {
                            if let Some(progress) = self.status.progress {
                                ui.add(egui::ProgressBar::new(progress as f32));
                            } else {
                                ui.add_space(
                                    ui.available_width() + ui.spacing().item_spacing.x / 2.0,
                                );
                            }
                        },
                    );
                    ui.separator();
                    ui.label(&self.status.message)
                })
            });
    }

    /// Get the image settings
    pub fn image(&self) -> Image {
        match self.tab {
            UITab::Explore | UITab::Color => {
                let mut output = match self.view_state {
                    ViewState::Viewport => self.image_settings.clone(),
                    ViewState::OutputView | ViewState::OutputLock | ViewState::Output => {
                        let mut output = self.image_settings.clone();
                        output.viewport.center = self.output_viewport.center.clone();
                        output.viewport.zoom = self.output_viewport.zoom + self.render_zoom_offset;
                        output
                    }
                };
                if self.tab == UITab::Explore {
                    output.viewport.scaling = self.optimized_settings.viewport.scaling;
                    output.external_coloring = self.optimized_settings.external_coloring.clone();
                    output.internal_coloring = self.optimized_settings.internal_coloring.clone();
                }
                output
            }
            UITab::Render => {
                let mut output = self.image_settings.clone();
                output.viewport.center = self.output_preview_viewport.center.clone();
                output.viewport.zoom = self.output_preview_viewport.zoom;
                output
            }
        }
    }

    /// Get the mouse down state
    pub fn mouse_down(&self) -> bool {
        self.mouse_down
    }

    pub fn is_render(&self) -> bool {
        self.tab == UITab::Render
    }
}
