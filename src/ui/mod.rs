/*!

# Corgi UI

This module contains the main UI state struct and its implementation, which
contains the code necessary to update internal state and render the ui.
 */

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use corgi::types::{
    Coloring, ComplexPoint, Image, ImageGenCommand, OptLevel, Status, Viewport, get_precision,
};
use directories::BaseDirs;
use eframe::egui::containers::menu::MenuButton;
use eframe::egui::{
    Button, Color32, CornerRadius, Frame, ScrollArea, Sense, Separator, Stroke, TextStyle,
    UiBuilder, Vec2, WidgetText,
};
use eframe::{egui, egui_wgpu};
use egui_material_icons::icons;
use egui_taffy::{TuiBuilderLogic, tui};
use preview_resources::PaintCallback;
use rug::Float;
use rug::ops::PowAssign;
use taffy::Overflow;
use taffy::prelude::*;
use utils::{TuiExt, collapsible, input_with_label, point_edit, section, selection_with_label};

mod coloring;
mod preview_resources;
mod settings;
mod utils;

pub use preview_resources::PreviewRenderResources;

pub trait EditUI {
    fn render_edit_ui(&mut self, ctx: &egui::Context, tui: &mut egui_taffy::Tui);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
#[derive(Debug)]
pub struct CorgiUI {
    output_settings: Image,
    explore_settings: Image,
    pub rendered_explore_viewport: Viewport,
    pub rendered_output_viewport: Viewport,
    pub output_preview_viewport: Viewport,
    render_zoom_offset: f64,
    setting_probe: bool,
    tab: UITab,
    view_state: ViewState,
    show_camera: bool,
    pub swap: bool,
    pub status: Status,
    command_channel: mpsc::Sender<ImageGenCommand>,
    output_path: PathBuf,
    show_settings: bool,
}

impl CorgiUI {
    /// Create a new state struct; status should be shared with the render thread.
    pub fn new(
        context: &crate::Context,
        image: Image,
        command_channel: mpsc::Sender<ImageGenCommand>,
    ) -> Self {
        let default_output_viewport = Viewport {
            width: 3840,
            height: 2160,
            scaling: image.viewport.scaling,
            zoom: image.viewport.zoom,
            center: image.viewport.center.clone(),
        };
        Self {
            status: Status::default(),
            rendered_explore_viewport: image.viewport.clone(),
            rendered_output_viewport: default_output_viewport.clone(),
            output_preview_viewport: default_output_viewport.clone(),
            view_state: ViewState::OutputLock,
            render_zoom_offset: -0.5,
            explore_settings: Image {
                viewport: Viewport {
                    scaling: 0.5,
                    ..image.viewport.clone()
                },
                external_coloring: Coloring::external_opt_default(),
                internal_coloring: Coloring::internal_opt_default(),
                ..image.clone()
            },
            output_settings: Image {
                viewport: default_output_viewport,
                optimization_level: OptLevel::AccuracyOptimized,
                ..image
            },
            show_camera: false,
            setting_probe: false,
            swap: false,
            command_channel,
            tab: UITab::Explore,
            output_path: context.cache().previous_paths.image.clone(),
            show_settings: false,
        }
    }

    /// Generate the UI and handle any events. This function will do some blocking
    /// to access shared data
    pub fn generate_ui(&mut self, ctx: &egui::Context, context: &mut crate::Context) {
        egui::SidePanel::right("settings_panel")
            .frame(Frame::new().fill(ctx.style().visuals.window_fill))
            .show(ctx, |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                let y = ui.style_mut().spacing.item_spacing.y;
                ui.style_mut().spacing.item_spacing.y = 0.0;
                // Top bar
                ui.horizontal(|ui| {
                    {
                        let style = ui.style_mut();
                        style.spacing.item_spacing.x = 0.0;
                        style.visuals.widgets.inactive.corner_radius = CornerRadius::same(0);
                        style.visuals.widgets.active.corner_radius = CornerRadius::same(0);
                        style.visuals.widgets.hovered.corner_radius = CornerRadius::same(0);
                        style.spacing.button_padding.x *= 2.0;
                        style.override_text_style = Some(TextStyle::Heading);
                    }
                    self.menu(context, ui);
                    ui.selectable_value(
                        &mut self.tab,
                        UITab::Explore,
                        format!("{} Explore", icons::ICON_EXPLORE),
                    );
                    ui.selectable_value(
                        &mut self.tab,
                        UITab::Color,
                        format!("{} Style", icons::ICON_STYLE),
                    );
                    ui.selectable_value(
                        &mut self.tab,
                        UITab::Render,
                        format!("{} Render", icons::ICON_IMAGE),
                    );
                });
                ui.add(
                    Separator::default()
                        .spacing(ui.visuals().widgets.noninteractive.bg_stroke.width),
                );
                ui.style_mut().spacing.item_spacing.y = y;
                ScrollArea::vertical().show(ui, |ui| {
                    tui(ui, ui.id().with("side"))
                        .reserve_available_width()
                        .style(taffy::Style {
                            flex_direction: taffy::FlexDirection::Column,
                            size: percent(1.0),
                            align_items: Some(AlignItems::Stretch),
                            justify_content: Some(AlignContent::Start),
                            gap: length(ctx.style().spacing.item_spacing.y),
                            overflow: taffy::Point {
                                x: Overflow::Hidden,
                                y: Overflow::Scroll,
                            },
                            ..Default::default()
                        })
                        .show(|tui| match self.tab {
                            UITab::Explore => self.explore_tab(tui),
                            UITab::Color => {
                                section(tui, "External", true, |tui| {
                                    self.output_settings
                                        .external_coloring
                                        .render_edit_ui(ctx, tui);
                                });
                                section(tui, "Internal", false, |tui| {
                                    self.output_settings
                                        .internal_coloring
                                        .render_edit_ui(ctx, tui);
                                });
                            }
                            UITab::Render => {
                                section(tui, "Image Settings", true, |tui| {
                                    input_with_label(
                                        tui,
                                        "Image width",
                                        None,
                                        egui::DragValue::new(
                                            &mut self.output_settings.viewport.width,
                                        )
                                        .speed(10.0),
                                    );
                                    input_with_label(
                                        tui,
                                        "Image height",
                                        None,
                                        egui::DragValue::new(
                                            &mut self.output_settings.viewport.height,
                                        )
                                        .speed(10.0),
                                    );
                                    tui.horizontal().add(|tui| {
                                        let mut str_path = self
                                            .output_path
                                            .to_str()
                                            .unwrap_or("Invalid Path")
                                            .to_string();
                                        let home_opt = BaseDirs::new()
                                            .as_ref()
                                            .map(BaseDirs::home_dir)
                                            .and_then(Path::to_str)
                                            .map(ToOwned::to_owned);
                                        if let Some(home_dir) = &home_opt
                                            && str_path.starts_with(&(home_dir.to_owned() + "/"))
                                        {
                                            str_path = str_path.replacen(home_dir, "~", 1);
                                        }
                                        let text_size =
                                            WidgetText::Text(icons::ICON_FOLDER_OPEN.to_owned())
                                                .into_galley(
                                                    tui.egui_ui(),
                                                    None,
                                                    tui.egui_ui().available_width(),
                                                    TextStyle::Button,
                                                )
                                                .size();
                                        let spacing = tui.egui_ui().spacing().clone();
                                        let available_width = tui.egui_ui().available_width();
                                        tui.grow().ui_add(
                                            egui::TextEdit::singleline(&mut str_path)
                                                .desired_width(
                                                    available_width
                                                        - text_size.x
                                                        - spacing.button_padding.x * 2.0
                                                        - spacing.item_spacing.x * 2.0,
                                                ),
                                        );
                                        if let Some(home_dir) = &home_opt
                                            && str_path.starts_with("~/")
                                        {
                                            str_path = str_path.replacen("~", home_dir, 1);
                                        }
                                        self.output_path = str_path.into();

                                        if tui
                                            .ui_add(Button::new(icons::ICON_FOLDER_OPEN))
                                            .clicked()
                                            && let Some(path) = rfd::FileDialog::new()
                                                .set_directory(&self.output_path)
                                                .pick_folder()
                                        {
                                            context.cache_mut().previous_paths.image = path.clone();
                                            self.output_path = path;
                                        }
                                    });
                                });
                                let item_spacing = tui.egui_ui().spacing().item_spacing;
                                tui.style(taffy::Style {
                                    flex_direction: taffy::FlexDirection::Column,
                                    size: percent(1.0),
                                    padding: Rect {
                                        left: length(item_spacing.y * 3.0),
                                        right: length(item_spacing.y * 3.0),
                                        top: length(item_spacing.y * 3.0),
                                        bottom: length(0.0),
                                    },
                                    gap: length(tui.egui_ui().spacing().item_spacing.y * 2.0),
                                    ..Default::default()
                                })
                                .add(|tui| {
                                    if tui.ui_add(Button::new("Render")).clicked() {
                                        let image = self.output_settings.clone();
                                        let _ = self
                                            .command_channel
                                            .send(ImageGenCommand::NewOutputSettings(image));
                                    }
                                    if tui.ui_add(Button::new("Save to file")).clicked()
                                        && let Some(path) = rfd::FileDialog::new()
                                            .set_directory(&self.output_path)
                                            .add_filter(
                                                "image with metadata",
                                                &["avif", "jpg", "jpeg", "webp", "png"],
                                            )
                                            .add_filter(
                                                "image without metadata",
                                                &["gif", "qoi", "tiff", "exr"],
                                            )
                                            .set_file_name(format!(
                                                "fractal.{}",
                                                context.cache().default_image_type
                                            ))
                                            .save_file()
                                    {
                                        if let Some(ext) = path.extension().and_then(OsStr::to_str)
                                        {
                                            context.cache_mut().default_image_type = ext.to_owned();
                                        }
                                        let _ = self
                                            .command_channel
                                            .send(ImageGenCommand::SaveToFile(path.clone()));
                                    }
                                });
                            }
                        });
                });
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(ctx.style().visuals.window_fill))
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                self.viewport(ui, ctx);

                ui.horizontal_centered(|ui| {
                    ui.scope_builder(
                        UiBuilder::new().max_rect(
                            ui.max_rect()
                                .with_max_x(100.0 + ui.spacing().item_spacing.x)
                                .with_min_x(ui.spacing().item_spacing.x),
                        ),
                        |ui| {
                            ui.add_visible(
                                self.status.progress.is_some(),
                                egui::ProgressBar::new(self.status.progress.unwrap_or(0.0) as f32),
                            );
                        },
                    );
                    ui.separator();
                    ui.label(&self.status.message)
                })
            });
        let style = ctx.style().clone();
        egui::Window::new("Settings")
            .open(&mut self.show_settings)
            .show(ctx, |ui| {
                ui.set_style(style);
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);

                tui(ui, ui.id().with("settings"))
                    .reserve_available_width()
                    .style(Style {
                        flex_direction: FlexDirection::Column,
                        size: Size {
                            width: percent(1.0),
                            height: auto(),
                        },
                        ..Default::default()
                    })
                    .show(|tui| {
                        section(tui, "Configuration", true, |tui| {
                            context.config_mut().render_edit_ui(ctx, tui);
                        });
                        section(tui, "Theme", true, |tui| {
                            context.theme_mut().render_edit_ui(ctx, tui);
                        });
                    })
            });
    }

    // Build the menu button
    pub fn menu(&mut self, context: &mut crate::Context, ui: &mut egui::Ui) {
        let spacing = ui.spacing().button_padding.y;
        MenuButton::from_button(Button::new(icons::ICON_MENU)).ui(ui, |ui| {
            {
                let style = ui.style_mut();
                style.spacing.item_spacing = Vec2::splat(spacing);
                style.visuals.widgets.inactive.corner_radius = CornerRadius::same(spacing as u8);
                style.visuals.widgets.active.corner_radius = CornerRadius::same(spacing as u8);
                style.visuals.widgets.hovered.corner_radius = CornerRadius::same(spacing as u8);
                style.spacing.button_padding = Vec2::splat(spacing);
            }
            if ui.add(Button::new("Save Image Settings")).clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .set_directory(context.cache().previous_paths.settings.clone())
                    .set_file_name("saved_fractal.corg")
                    .add_filter("corg", &["corg"])
                    .save_file()
            {
                if let Some(dir) = path.parent() {
                    context.cache_mut().previous_paths.settings = dir.to_owned();
                }
                // write to file
                match self.output_settings.save_to_file(&path) {
                    Err(err) => {
                        tracing::error!("Failed to save image settings: {err:?}");
                        self.status.message = format!("Failed to save image settings: {err:?}")
                    }
                    Ok(_) => self.status.message = "Saved settings".to_string(),
                }
            }
            if ui.add(Button::new("Load Image Settings")).clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .set_directory(context.cache().previous_paths.settings.clone())
                    .add_filter(
                        "settings file or image with metadata",
                        &["corg", "json", "avif", "jpg", "jpeg", "webp", "png"],
                    )
                    .pick_file()
            {
                if let Some(dir) = path.parent() {
                    context.cache_mut().previous_paths.settings = dir.to_owned();
                }
                match Image::load_from_file(&path) {
                    Ok(image) => {
                        self.output_settings = image;
                    }
                    Err(err) => {
                        tracing::error!("Failed to load image settings `{path:?}`: {err}");
                        self.status.message = format!("Failed to load image settings: {err:?}")
                    }
                }
            }
            if ui.add(Button::new("Settings")).clicked() {
                self.show_settings = true;
            }
        });
    }

    /// Get the image settings
    pub fn image(&self) -> Image {
        let mut active_image = self.output_settings.clone();
        match self.tab {
            UITab::Explore | UITab::Color => {
                match self.view_state {
                    ViewState::Viewport => {
                        active_image.viewport = self.explore_settings.viewport.clone();
                        active_image.probe_location = self.explore_settings.probe_location.clone();
                    }
                    ViewState::OutputView | ViewState::OutputLock | ViewState::Output => {
                        active_image.viewport.zoom += self.render_zoom_offset;
                        active_image.viewport.width = self.explore_settings.viewport.width;
                        active_image.viewport.height = self.explore_settings.viewport.height;
                    }
                }
                if self.tab == UITab::Explore {
                    active_image.viewport.scaling = self.explore_settings.viewport.scaling;
                    active_image.external_coloring =
                        self.explore_settings.external_coloring.clone();
                    active_image.internal_coloring =
                        self.explore_settings.internal_coloring.clone();
                    active_image.optimization_level = OptLevel::PerformanceOptimized;
                } else {
                    active_image.viewport.scaling = 1.0;
                    active_image.optimization_level = OptLevel::CacheOptimized;
                }
                active_image
            }
            UITab::Render => {
                active_image.viewport = self.output_preview_viewport.clone();
                active_image
            }
        }
    }

    /// Returns whether the current tab has a viewport that needs
    /// automatic updates when the image settings change.
    pub fn has_active_viewport(&self) -> bool {
        self.tab != UITab::Render
    }

    /// Build the Explore tab UI
    fn explore_tab(&mut self, tui: &mut egui_taffy::Tui) {
        let img = self.image();
        let item_spacing = tui.egui_ui().spacing().item_spacing;
        tui.style(taffy::Style {
            flex_direction: taffy::FlexDirection::Column,
            size: percent(1.0),
            padding: Rect {
                left: length(item_spacing.y * 3.0),
                right: length(item_spacing.y * 3.0),
                top: length(item_spacing.y * 3.0),
                bottom: length(0.0),
            },
            gap: length(tui.egui_ui().spacing().item_spacing.y * 2.0),
            ..Default::default()
        })
        .add(|tui| {
            selection_with_label(
                tui,
                "Fractal Mode",
                Some("Which fractal algorithm to use. Switching from Mandelbrot to Julia will set the Julia parameter to the current view center."),
                &mut self.explore_settings.fractal_kind,
                vec![
                    corgi::types::FractalKind::Mandelbrot,
                    corgi::types::FractalKind::Julia(img.viewport.center.clone()),
                ],
            );
            match &mut self.explore_settings.fractal_kind {
                corgi::types::FractalKind::Mandelbrot => {}
                corgi::types::FractalKind::Julia(pt) => {
                    point_edit(tui, "Julia parameter", Some("The C value used in the Julia equation. picking values from interesting locations in the Mandelbrot set tend to be interesting in the Julia Set."), get_precision(img.viewport.zoom), pt);
                }
            }
            self.output_settings.fractal_kind = self.explore_settings.fractal_kind.clone();
            let mut scaling = (1.0 / self.explore_settings.viewport.scaling) as u32;
            input_with_label(
                tui,
                "Preview Scaling",
                Some("Divides the resolution of the preview image to improve performance."),
                egui::DragValue::new(&mut scaling)
                    .speed(0.01)
                    .range(1..=8)
                    .update_while_editing(false),
            );
            self.explore_settings.viewport.scaling = 1.0 / scaling as f64;
        });
        section(tui, "Viewport", true, |tui| {
            point_edit(
                tui,
                "Image Center",
                Some("The location of the center of the image in the complex plane."),
                get_precision(img.viewport.zoom),
                &mut self.output_settings.viewport.center,
            );
            input_with_label(
                tui,
                "Zoom",
                Some(
                    "Zoom level of the camera. Scales the range of the viewport by 2 raised to the negative of the zoom.",
                ),
                egui::DragValue::new(&mut self.output_settings.viewport.zoom)
                    .speed(0.03)
                    .update_while_editing(false),
            );
            input_with_label(
                tui,
                "Max iteration",
                Some(
                    "The maximum number of iterations to calculate before assuming a point is inside the set. Not all points will run this many iterations, some will quit early.",
                ),
                egui::DragValue::new(&mut self.output_settings.max_iter)
                    .speed(100.0)
                    .range(100..=u32::MAX)
                    .update_while_editing(false),
            );
            collapsible(tui, "Advanced", |tui| {
                point_edit(
                    tui,
                    "Probe Point",
                    Some(
                        "The reference location to use when calculating the fractal using perturbation-based formulas.",
                    ),
                    get_precision(img.viewport.zoom),
                    &mut self.output_settings.probe_location,
                );
                tui.ui_add(Button::new(format!(
                    "{} Pick new probe point",
                    icons::ICON_POINT_SCAN
                )))
                .clicked()
                .then(|| self.setting_probe = !self.setting_probe);
            });
        });
        section(tui, "Camera", false, |tui| {
            tui.ui_add(egui::Checkbox::new(&mut self.show_camera, "Show Camera"));
            if self.show_camera {
                match self.view_state {
                    ViewState::Viewport | ViewState::OutputView => {
                        tui.horizontal().add(|tui| {
                            if tui
                                .ui_add(Button::new(format!(
                                    "{} Move Camera Here",
                                    icons::ICON_CROP_FREE
                                )))
                                .clicked()
                            {
                                self.output_settings.viewport.center =
                                    self.explore_settings.viewport.center.clone();
                                self.output_settings.viewport.zoom =
                                    self.explore_settings.viewport.zoom + 0.5;
                                self.output_settings.update_probe();
                                self.render_zoom_offset = -0.5;
                                self.view_state = ViewState::OutputLock;
                            }
                            if tui
                                .ui_add(Button::new(format!(
                                    "{} Return to Camera",
                                    icons::ICON_BACK_TO_TAB
                                )))
                                .clicked()
                            {
                                self.view_state = ViewState::OutputLock;
                            }
                        });
                    }
                    ViewState::OutputLock => {
                        if tui
                            .ui_add(Button::new(format!("{} Pin Camera", icons::ICON_LOCK)))
                            .clicked()
                        {
                            self.explore_settings.viewport.center =
                                self.output_settings.viewport.center.clone();
                            self.explore_settings.viewport.zoom =
                                self.output_settings.viewport.zoom + self.render_zoom_offset;
                            self.explore_settings.update_probe();
                            self.view_state = ViewState::OutputView;
                        }
                    }
                    ViewState::Output => {}
                }
            }
            input_with_label(
                tui,
                "Image width",
                None,
                egui::DragValue::new(&mut self.output_settings.viewport.width).speed(10.0),
            );
            input_with_label(
                tui,
                "Image height",
                None,
                egui::DragValue::new(&mut self.output_settings.viewport.height).speed(10.0),
            );
        });
    }

    fn viewport(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
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
                if self.setting_probe {
                    // probe setting mode, set the probe location to the mouse position
                    // on click
                    if primary_down
                        && pointer_in_rect
                        && let Some(pos) = pointer_pos
                    {
                        let (x, y) = view_image
                            .viewport
                            .get_real_coords((pos.x) as f64, (size.y - pos.y) as f64);
                        match self.view_state {
                            ViewState::Viewport => {
                                self.explore_settings.probe_location = ComplexPoint { x, y }
                            }
                            ViewState::OutputView | ViewState::OutputLock | ViewState::Output => {
                                self.output_settings.probe_location = ComplexPoint { x, y }
                            }
                        }
                        self.setting_probe = false;
                    }
                } else {
                    self.handle_viewport_input(ui, pointer_in_rect, &view_image);
                }

                self.explore_settings.viewport.width = size.x as usize;
                self.explore_settings.viewport.height = size.y as usize;
                self.output_preview_viewport.width = size.x as usize;
                self.output_preview_viewport.height = size.y as usize;

                // render texture and camera overlay
                let view_image = self.image();
                let mut render_rect = rect.scale_from_center2(
                    egui::Vec2::splat(1.0) / view_image.viewport.aspect_scale(),
                );
                let (x, y) = view_image.viewport.coords_to_px_offset(
                    &self.output_settings.viewport.center.x,
                    &self.output_settings.viewport.center.y,
                );
                render_rect = render_rect.translate(Vec2::new(x as f32, -y as f32));
                render_rect = render_rect.scale_from_center(f32::powf(
                    2.0,
                    -(self.output_settings.viewport.zoom - view_image.viewport.zoom) as f32,
                ));
                render_rect =
                    render_rect.scale_from_center2(self.output_settings.viewport.aspect_scale());
                let cb = PaintCallback {
                    rendered_viewport: if self.tab == UITab::Render {
                        self.rendered_output_viewport.clone()
                    } else {
                        self.rendered_explore_viewport.clone()
                    },
                    view: view_image.viewport,
                    swap: self.swap,
                    output: self.tab == UITab::Render,
                };
                self.swap = false;

                let callback = egui_wgpu::Callback::new_paint_callback(rect, cb);

                ui.painter().add(callback);
                if self.tab != UITab::Render && self.show_camera {
                    ui.painter().rect_stroke(
                        render_rect.intersect(rect),
                        0.0,
                        Stroke::new(2.0, Color32::from_gray(255)),
                        egui::StrokeKind::Outside,
                    );
                }
            },
        );
    }

    fn handle_viewport_input(
        &mut self,
        ui: &mut egui::Ui,
        pointer_in_rect: bool,
        view_image: &Image,
    ) {
        let response = ui.response();
        // get scroll and drag inputs to change the viewport
        let (mut scroll, pixel_scale) = ui.input(|i| (i.smooth_scroll_delta, i.pixels_per_point));
        if !pointer_in_rect {
            scroll = Vec2::ZERO;
        }
        let drag = response.drag_delta();

        // scroll
        let precision = get_precision(view_image.viewport.zoom);
        let mut scale = Float::with_val(precision, 2.0);
        scale.pow_assign(-view_image.viewport.zoom);
        let aspect_scale = view_image.viewport.aspect_scale();
        let x_offset = -(drag.x as f64 / view_image.viewport.width as f64
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
                self.explore_settings.viewport.center.x += x_offset;
                self.explore_settings.viewport.center.y += y_offset;
                self.explore_settings.viewport.zoom += scroll.y as f64 * pixel_scale as f64 * 0.005;
                self.explore_settings.viewport.update_prec();
                self.explore_settings.update_probe();
            }
            ViewState::OutputView => {
                if drag.x != 0.0 || drag.y != 0.0 {
                    self.explore_settings.viewport.center.x =
                        self.output_settings.viewport.center.x.clone() + x_offset;
                    self.explore_settings.viewport.center.y =
                        self.output_settings.viewport.center.y.clone() + y_offset;
                    self.explore_settings.viewport.zoom = view_image.viewport.zoom;
                    self.explore_settings.viewport.update_prec();
                    self.explore_settings.update_probe();
                    self.view_state = ViewState::Viewport;
                }
                self.render_zoom_offset += scroll.y as f64 * pixel_scale as f64 * 0.005;
            }
            ViewState::OutputLock => {
                self.output_settings.viewport.center.x += x_offset;
                self.output_settings.viewport.center.y += y_offset;
                self.output_settings.viewport.zoom += scroll.y as f64 * pixel_scale as f64 * 0.005;
                self.output_settings.viewport.update_prec();
                self.output_settings.update_probe();
            }
            ViewState::Output => {
                self.output_preview_viewport.center.x += x_offset;
                self.output_preview_viewport.center.y += y_offset;
                self.output_preview_viewport.zoom += scroll.y as f64 * pixel_scale as f64 * 0.005;
                self.output_preview_viewport.update_prec();
            }
        }
    }
}
