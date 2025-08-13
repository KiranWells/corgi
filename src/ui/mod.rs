/*!

# Corgi UI

This module contains the main UI state struct and its implementation, which
contains the code necessary to update internal state and render the ui.
 */

use eframe::egui::{Button, Color32, Sense, Stroke, UiBuilder, Vec2};
use egui_taffy::{TuiBuilderLogic, TuiWidget, tui};
use nanoserde::{DeJson, SerJson};
use std::fs::{OpenOptions, read_to_string};
use std::io::Write;
use taffy::prelude::*;

use eframe::egui_wgpu::CallbackTrait;
use eframe::{egui, egui_wgpu};
use rug::{Float, ops::PowAssign};

use crate::types::{Image, PreviewRenderResources, ProbeLocation, Status, Viewport, get_precision};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ViewState {
    Viewport,
    OutputView,
    OutputLock,
    Output,
}

/// The main UI state struct.
pub struct CorgiUI {
    image_settings: Image,
    pub status: Status,
    pub rendered_viewport: Viewport,
    pub output_viewport: Viewport,
    view_state: ViewState,
    render_zoom_offset: f64,
    x_text_buff: String,
    y_text_buff: String,
    x_probe_buff: String,
    y_probe_buff: String,
    setting_probe: bool,
    mouse_down: bool,
    pub swap: bool,
}

fn input_with_label(tui: &mut egui_taffy::Tui, label: &str, widget: impl TuiWidget) {
    tui.style(taffy::Style {
        flex_direction: taffy::FlexDirection::Row,
        // flex_grow: 1.0,
        justify_content: Some(taffy::AlignContent::Stretch),
        ..Default::default()
    })
    .add(|tui| {
        tui.style(taffy::Style {
            flex_grow: 1.0,
            ..Default::default()
        })
        .label(label);
        tui.ui_add(widget);
    });
}

impl CorgiUI {
    /// Create a new state struct; status should be shared with the render thread.
    pub fn new(image: Image) -> Self {
        Self {
            status: Status::default(),
            rendered_viewport: image.viewport.clone(),
            output_viewport: Viewport {
                width: 1920,
                height: 1080,
                zoom: image.viewport.zoom,
                x: image.viewport.x.clone(),
                y: image.viewport.y.clone(),
            },
            view_state: ViewState::Viewport,
            render_zoom_offset: -1.0,
            x_text_buff: image.viewport.x.to_string_radix(10, None),
            y_text_buff: image.viewport.y.to_string_radix(10, None),
            x_probe_buff: image.probe_location.x.to_string_radix(10, None),
            y_probe_buff: image.probe_location.y.to_string_radix(10, None),
            image_settings: image,
            setting_probe: false,
            mouse_down: false,
            swap: false,
        }
    }

    /// Generate the UI and handle any events. This function will do some blocking
    /// to access shared data
    pub fn generate_ui(&mut self, ctx: &egui::Context) {
        // update the image settings from the text buffers
        let precision = get_precision(self.image().viewport.zoom);
        if let Ok(res) = Float::parse(&self.x_text_buff) {
            self.image_settings.viewport.x = Float::with_val(precision, res)
        }
        if let Ok(res) = Float::parse(&self.y_text_buff) {
            self.image_settings.viewport.y = Float::with_val(precision, res)
        }
        // probe
        if let Ok(res) = Float::parse(&self.x_probe_buff) {
            self.image_settings.probe_location.x = Float::with_val(precision, res)
        }
        if let Ok(res) = Float::parse(&self.y_probe_buff) {
            self.image_settings.probe_location.y = Float::with_val(precision, res)
        }

        ctx.style_mut(|style| {
            style.wrap_mode = Some(egui::TextWrapMode::Extend);
        });
        // create the right side settings panel
        egui::SidePanel::right("settings_panel").show(ctx, |ui| {
            tui(ui, ui.id().with("side"))
                .reserve_available_space()
                // .reserve_width(300.0)
                .style(taffy::Style {
                    flex_direction: taffy::FlexDirection::Column,
                    // min_size: taffy::Size {
                    //     width: percent(1.0),
                    //     height: auto(),
                    // },
                    size: percent(1.0),
                    // flex_wrap: FlexWrap::NoWrap,
                    flex_grow: 1.0,
                    // align_items: Some(taffy::AlignItems::Stretch),
                    // max_size: percent(1.),
                    gap: length(8.),
                    ..Default::default()
                })
                .show(|tui| {
                    tui.heading("Corgi");
                    tui.separator();
                    tui.label("Viewport");
                    if tui
                        .enabled_ui(self.view_state == ViewState::Viewport)
                        .ui_add(Button::new("Set Camera to View"))
                        .clicked()
                    {
                        self.output_viewport.x = self.image_settings.viewport.x.clone();
                        self.output_viewport.y = self.image_settings.viewport.y.clone();
                        self.output_viewport.zoom = self.image_settings.viewport.zoom + 0.5;
                        self.render_zoom_offset = -0.5;
                        if self.view_state == ViewState::Viewport {
                            self.view_state = ViewState::OutputView;
                        }
                    }
                    if tui
                        .ui_add(Button::new(if self.view_state == ViewState::Viewport {
                            "Preview Camera"
                        } else {
                            "Exit Preview"
                        }))
                        .clicked()
                    {
                        if self.view_state == ViewState::Viewport {
                            self.view_state = ViewState::OutputView;
                        } else {
                            self.view_state = ViewState::Viewport;
                        }
                    }
                    if tui
                        .ui_add(Button::new(if self.view_state != ViewState::OutputLock {
                            "Lock Camera to View"
                        } else {
                            "Unlock Camera"
                        }))
                        .clicked()
                    {
                        if self.view_state == ViewState::OutputLock {
                            self.view_state = ViewState::OutputView;
                        } else {
                            self.view_state = ViewState::OutputLock;
                        }
                    }
                    tui.style(taffy::Style {
                        // overflow: taffy::Point {
                        //     x: taffy::Overflow::Scroll,
                        //     y: taffy::Overflow::Scroll,
                        // },
                        size: Size {
                            width: percent(1.0),
                            height: auto(),
                        },
                        display: taffy::Display::Grid,
                        align_items: Some(taffy::AlignItems::Stretch),
                        justify_items: Some(taffy::AlignItems::Stretch),
                        grid_template_rows: vec![min_content(); 4],
                        grid_template_columns: vec![auto(), auto()],
                        gap: length(8.),
                        ..Default::default()
                    })
                    .add(|tui| {
                        for (label, reference) in [
                            ("real offset", &mut self.x_text_buff),
                            ("imaginary offset", &mut self.y_text_buff),
                            ("probe real", &mut self.x_probe_buff),
                            ("probe imaginary", &mut self.y_probe_buff),
                        ] {
                            tui.label(label);
                            tui.ui_add(egui::TextEdit::singleline(reference));
                        }
                    });
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
                    tui.separator();
                    tui.label("Coloring");
                    input_with_label(
                        tui,
                        "Saturation",
                        egui::DragValue::new(&mut self.image_settings.coloring.saturation)
                            .speed(0.003)
                            .range(0.0..=f32::MAX),
                    );
                    input_with_label(
                        tui,
                        "Color frequency",
                        egui::DragValue::new(&mut self.image_settings.coloring.color_frequency)
                            .speed(0.01),
                    );

                    input_with_label(
                        tui,
                        "Color offset",
                        egui::DragValue::new(&mut self.image_settings.coloring.color_offset)
                            .speed(0.003)
                            .range(0.0..=1.0),
                    );

                    input_with_label(
                        tui,
                        "Glow spread",
                        egui::DragValue::new(&mut self.image_settings.coloring.glow_spread)
                            .speed(0.01)
                            .range(0.0..=f32::MAX),
                    );

                    input_with_label(
                        tui,
                        "Glow intensity",
                        egui::DragValue::new(&mut self.image_settings.coloring.glow_intensity)
                            .speed(0.01),
                    );

                    input_with_label(
                        tui,
                        "Brightness",
                        egui::DragValue::new(&mut self.image_settings.coloring.brightness)
                            .speed(0.003)
                            .range(0.0..=f32::MAX),
                    );

                    input_with_label(
                        tui,
                        "Internal brightness",
                        egui::DragValue::new(&mut self.image_settings.coloring.internal_brightness)
                            .speed(0.03)
                            .range(0.0..=f32::MAX),
                    );

                    tui.separator();
                    input_with_label(
                        tui,
                        "Debug parameter",
                        egui::DragValue::new(&mut self.image_settings.misc),
                    );

                    input_with_label(
                        tui,
                        "Debug shutter",
                        egui::DragValue::new(&mut self.image_settings.debug_shutter)
                            .speed(0.003)
                            .range(0.0..=1.0),
                    );

                    tui.separator();
                    if tui.ui_add(Button::new("Save Image Settings")).clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .set_file_name("saved_fractal.corg")
                            .add_filter("corg", &["corg"])
                            .save_file()
                        {
                            // write to file
                            let file = OpenOptions::new()
                                .create(true)
                                .truncate(true)
                                .write(true)
                                .open(path);
                            match file {
                                Err(err) => {
                                    self.status.message =
                                        format!("Failed to save image settings: {err:?}")
                                }
                                Ok(mut file) => {
                                    if let Err(err) =
                                        file.write(self.image_settings.serialize_json().as_bytes())
                                    {
                                        self.status.message =
                                            format!("Failed to write image settings: {err:?}")
                                    }
                                }
                            }
                        }
                    }
                    if tui.ui_add(Button::new("Load Image Settings")).clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("corg", &["corg"])
                            .pick_file()
                        {
                            // write to file
                            let contents = read_to_string(path);
                            match contents {
                                Err(err) => {
                                    self.status.message =
                                        format!("Failed to load image settings: {err:?}")
                                }
                                Ok(file) => match Image::deserialize_json(file.as_ref()) {
                                    Ok(image) => {
                                        self.x_text_buff =
                                            image.viewport.x.to_string_radix(10, None);
                                        self.y_text_buff =
                                            image.viewport.y.to_string_radix(10, None);
                                        self.x_probe_buff =
                                            image.probe_location.x.to_string_radix(10, None);
                                        self.y_probe_buff =
                                            image.probe_location.y.to_string_radix(10, None);
                                        self.image_settings = image;
                                    }
                                    Err(err) => {
                                        self.status.message =
                                            format!("Failed to parse image settings: {err:?}")
                                    }
                                },
                            }
                        }
                    }
                    tui.separator();
                    tui.label("Status");
                    tui.label(format!("Status: {:?}", self.status.message));
                    if let Some(progress) = self.status.progress {
                        tui.ui_add(egui::ProgressBar::new(progress as f32));
                    }
                });
        });

        // create the main canvas
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.scope_builder(UiBuilder::new().sense(Sense::drag()), |ui| {
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
                self.mouse_down = response.dragged();
                if self.setting_probe {
                    // probe setting mode, set the probe location to the mouse position
                    // on click
                    if primary_down && pointer_in_rect {
                        if let Some(pos) = pointer_pos {
                            let (x, y) = view_image
                                .viewport
                                .get_real_coords((pos.x) as f64, (size.y - pos.y) as f64);
                            self.x_probe_buff = x.to_string_radix(10, None);
                            self.y_probe_buff = y.to_string_radix(10, None);
                            self.image_settings.probe_location = ProbeLocation { x, y };
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
                    match self.view_state {
                        ViewState::Viewport => {
                            self.image_settings.viewport.x += x_offset;
                            self.image_settings.viewport.y += y_offset;
                            self.image_settings.viewport.zoom +=
                                scroll.y as f64 * pixel_scale as f64 * 0.005;
                        }
                        ViewState::OutputView => {
                            if drag.x != 0.0 || drag.y != 0.0 {
                                self.image_settings.viewport.x =
                                    self.output_viewport.x.clone() + x_offset;
                                self.image_settings.viewport.y =
                                    self.output_viewport.y.clone() + y_offset;
                                self.image_settings.viewport.zoom = view_image.viewport.zoom;
                                self.view_state = ViewState::Viewport;
                            }
                            self.render_zoom_offset += scroll.y as f64 * pixel_scale as f64 * 0.005;
                        }
                        ViewState::OutputLock => {
                            self.output_viewport.x += x_offset;
                            self.output_viewport.y += y_offset;
                            self.output_viewport.zoom +=
                                scroll.y as f64 * pixel_scale as f64 * 0.005;
                        }
                        ViewState::Output => {
                            self.render_zoom_offset += scroll.y as f64 * pixel_scale as f64 * 0.005;
                        }
                    }
                    self.x_text_buff = self.image_settings.viewport.x.to_string_radix(10, None);
                    self.y_text_buff = self.image_settings.viewport.y.to_string_radix(10, None);
                }

                self.image_settings.viewport.width = size.x as usize;
                self.image_settings.viewport.height = size.y as usize;

                // render the image

                let view_image = self.image();
                let mut render_rect = rect.scale_from_center2(
                    egui::Vec2::splat(1.0) / view_image.viewport.aspect_scale(),
                );
                let (x, y) = view_image
                    .viewport
                    .coords_to_px_offset(&self.output_viewport.x, &self.output_viewport.y);
                render_rect = render_rect.translate(Vec2::new(x as f32, -y as f32));
                render_rect = render_rect.scale_from_center(f32::powf(
                    2.0,
                    -(self.output_viewport.zoom - view_image.viewport.zoom) as f32,
                ));
                render_rect = render_rect.scale_from_center2(self.output_viewport.aspect_scale());
                let cb = PaintCallback {
                    rendered_viewport: self.rendered_viewport.clone(),
                    view: view_image.viewport,
                    swap: self.swap,
                };
                self.swap = false;

                let callback = egui_wgpu::Callback::new_paint_callback(rect, cb);

                ui.painter().add(callback);
                ui.painter().rect_stroke(
                    render_rect.intersect(rect),
                    0.0,
                    Stroke::new(2.0, Color32::from_gray(255)),
                    egui::StrokeKind::Outside,
                );
            });
        });
    }

    /// Get the image settings
    pub fn image(&self) -> Image {
        match self.view_state {
            ViewState::Viewport => self.image_settings.clone(),
            ViewState::OutputView | ViewState::OutputLock | ViewState::Output => {
                let mut output = self.image_settings.clone();
                output.viewport.x = self.output_viewport.x.clone();
                output.viewport.y = self.output_viewport.y.clone();
                output.viewport.zoom = self.output_viewport.zoom + self.render_zoom_offset;
                output
            }
        }
    }

    /// Get the mouse down state
    pub fn mouse_down(&self) -> bool {
        self.mouse_down
    }
}

struct PaintCallback {
    rendered_viewport: crate::types::Viewport,
    view: crate::types::Viewport,
    swap: bool,
}

impl CallbackTrait for PaintCallback {
    fn prepare(
        &self,
        device: &eframe::wgpu::Device,
        queue: &eframe::wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _egui_encoder: &mut eframe::wgpu::CommandEncoder,
        callback_resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<eframe::wgpu::CommandBuffer> {
        let res = callback_resources
            .get_mut::<PreviewRenderResources>()
            .expect("Failed to get render resources");
        if self.swap {
            // copy the preview texture to the used texture
            tracing::debug!("Swapping");
            res.swap(device, queue);
        }

        let size = (self.rendered_viewport.width, self.rendered_viewport.height);
        if size != *res.size() {
            // resize the render resources, refreshing the texture reference
            res.resize(device, queue, size)
                .expect("Failed to resize render resources");
        }
        let transforms = self.rendered_viewport.transforms_from(&self.view);

        res.prepare(device, queue, transforms);
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut eframe::wgpu::RenderPass<'static>,
        callback_resources: &egui_wgpu::CallbackResources,
    ) {
        callback_resources
            .get::<PreviewRenderResources>()
            .expect("Failed to get render resources")
            .paint(render_pass);
    }
}
