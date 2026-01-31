/*!

# Corgi UI

This module contains the main UI state struct and its implementation, which
contains the code necessary to update internal state and render the ui.
 */

use std::f32::consts::PI;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use corgi::types::serde::SafeSaveLoad;
use corgi::types::{
    ComplexPoint, ImgSpec, OptLevel, Rotate, Style as ImgStyle, View, get_precision,
};
use directories::BaseDirs;
use eframe::egui::containers::menu::MenuButton;
use eframe::egui::{
    Button, Color32, CornerRadius, Frame, Pos2, ScrollArea, Sense, Separator, Stroke, TextStyle,
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

use crate::app::Status;
use crate::worker::{ImageGenCommand, RendererId};

mod coloring;
pub mod debouncer;
mod preview_resources;
mod settings;
mod utils;

pub use preview_resources::PreviewRenderResources;

pub trait EditUI {
    fn render_edit_ui(&mut self, ctx: &egui::Context, tui: &mut egui_taffy::Tui);
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum UITab {
    Explore,
    Style,
    Render,
}

#[derive(Debug)]
struct ExploreTabState {
    rendered_view: View,
    style: ImgStyle,
    scaling: f32,
}
#[derive(Debug)]
struct StyleTabState {
    rendered_view: View,
    scaling: f32,
}
#[derive(Debug)]
struct RenderTabState {
    rendered_view: View,
    save_path: PathBuf,
}

/// The main UI state struct.
#[derive(Debug)]
pub struct CorgiUI {
    tab: UITab,
    explore_state: ExploreTabState,
    style_state: StyleTabState,
    render_state: RenderTabState,
    root_spec: ImgSpec,
    current_view: View,
    setting_probe: bool,
    show_camera: bool,
    show_settings: bool,
    rendering_output: bool,
    pub swap: bool,
    pub status: Status,
    command_channel: mpsc::Sender<ImageGenCommand>,
}

impl CorgiUI {
    /// Create a new state struct; status should be shared with the render thread.
    pub fn new(
        context: &crate::Context,
        image: ImgSpec,
        command_channel: mpsc::Sender<ImageGenCommand>,
    ) -> Self {
        Self {
            tab: UITab::Explore,
            explore_state: ExploreTabState {
                rendered_view: image.view(),
                style: ImgStyle::opt_default(),
                scaling: 0.5,
            },
            style_state: StyleTabState {
                rendered_view: image.view(),
                scaling: 1.0,
            },
            render_state: RenderTabState {
                rendered_view: image.view(),
                save_path: context.cache().previous_paths.image.clone(),
            },
            current_view: image.view(),
            root_spec: ImgSpec {
                optimization_level: OptLevel::AccuracyOptimized,
                ..image
            },
            show_camera: false,
            setting_probe: false,
            swap: false,
            show_settings: false,
            rendering_output: false,
            status: Status::default(),
            command_channel,
        }
    }

    /// Generate the UI and handle any events. This function will do some blocking
    /// to access shared data
    pub fn generate_ui(
        &mut self,
        ctx: &egui::Context,
        context: &mut crate::Context,
        cancel: impl FnOnce(),
    ) {
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
                        UITab::Style,
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
                            UITab::Style => {
                                section(tui, "External", true, |tui| {
                                    self.root_spec
                                        .style
                                        .external_coloring
                                        .render_edit_ui(ctx, tui);
                                });
                                section(tui, "Internal", false, |tui| {
                                    self.root_spec
                                        .style
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
                                        egui::DragValue::new(&mut self.root_spec.width).speed(10.0),
                                    );
                                    input_with_label(
                                        tui,
                                        "Image height",
                                        None,
                                        egui::DragValue::new(&mut self.root_spec.height)
                                            .speed(10.0),
                                    );
                                    tui.horizontal().add(|tui| {
                                        let mut str_path = self
                                            .render_state
                                            .save_path
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
                                        self.render_state.save_path = str_path.into();

                                        if tui
                                            .ui_add(Button::new(icons::ICON_FOLDER_OPEN))
                                            .clicked()
                                            && let Some(path) = rfd::FileDialog::new()
                                                .set_directory(&self.render_state.save_path)
                                                .pick_folder()
                                        {
                                            context.cache_mut().previous_paths.image = path.clone();
                                            self.render_state.save_path = path;
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
                                    if !self.rendering_output {
                                        if tui.ui_add(Button::new("Render")).clicked() {
                                            let image = self.root_spec.clone();
                                            let _ =
                                                self.command_channel.send(ImageGenCommand::Render(
                                                    RendererId::Render,
                                                    Box::new(image),
                                                ));
                                            self.rendering_output = true;
                                        }
                                    } else if tui.ui_add(Button::new("Cancel Render")).clicked() {
                                        self.rendering_output = false;
                                        cancel();
                                    }
                                    if tui.ui_add(Button::new("Save to file")).clicked()
                                        && let Some(path) = rfd::FileDialog::new()
                                            .set_directory(&self.render_state.save_path)
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
                                        let _ =
                                            self.command_channel.send(ImageGenCommand::SaveToFile(
                                                RendererId::Render,
                                                path.clone(),
                                            ));
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
                self.render_widgets(ui, ctx, context);

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

                let previous_config = context.config().clone();
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
                    });
                if context.config().ui_max_shader_batch_iters
                    != previous_config.ui_max_shader_batch_iters
                {
                    let _ = self.command_channel.send(ImageGenCommand::UpdateConstants(
                        RendererId::Explore,
                        corgi::image_gen::Constants {
                            iter_batch_size: context.config().ui_max_shader_batch_iters,
                        },
                    ));
                    let _ = self.command_channel.send(ImageGenCommand::UpdateConstants(
                        RendererId::Style,
                        corgi::image_gen::Constants {
                            iter_batch_size: context.config().ui_max_shader_batch_iters,
                        },
                    ));
                }
                if context.config().max_shader_batch_iters != previous_config.max_shader_batch_iters
                {
                    let _ = self.command_channel.send(ImageGenCommand::UpdateConstants(
                        RendererId::Render,
                        corgi::image_gen::Constants {
                            iter_batch_size: context.config().max_shader_batch_iters,
                        },
                    ));
                }
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
                match self.root_spec.save(&path) {
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
                match ImgSpec::load(&path) {
                    Ok(image) => {
                        self.root_spec = image;
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
    pub fn image(&self) -> ImgSpec {
        let mut active_image = self.root_spec.clone();
        match self.tab {
            UITab::Explore => {
                active_image.style = self.explore_state.style.clone();
                active_image.width = self.current_view.width;
                active_image.height = self.current_view.height;
                active_image.location.zoom -=
                    self.current_view.zoom_offset_from(&self.root_spec.view()) + 0.1;
                active_image.scale(self.explore_state.scaling);
                active_image.optimization_level = OptLevel::PerformanceOptimized;
                active_image
            }
            UITab::Style => {
                active_image.width = self.current_view.width;
                active_image.height = self.current_view.height;
                active_image.location.zoom -=
                    self.current_view.zoom_offset_from(&self.root_spec.view()) + 0.1;
                active_image.scale(self.style_state.scaling);
                active_image.optimization_level = OptLevel::CacheOptimized;
                active_image
            }
            UITab::Render => {
                active_image.set_view(self.current_view.clone());
                active_image
            }
        }
    }

    /// Returns whether the current tab has a viewport that needs
    /// automatic updates when the image settings change.
    pub fn has_active_viewport(&self) -> bool {
        self.tab != UITab::Render
    }
    pub fn send_render(&self) -> color_eyre::Result<()> {
        Ok(self.command_channel.send(ImageGenCommand::Render(
            match self.tab {
                UITab::Explore => RendererId::Explore,
                UITab::Style => RendererId::Style,
                UITab::Render => unreachable!(),
            },
            Box::new(self.image()),
        ))?)
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
                &mut self.root_spec.location.fractal_kind,
                vec![
                    corgi::types::FractalKind::Mandelbrot,
                    corgi::types::FractalKind::Julia(img.location.center.clone()),
                ],
            );
            match &mut self.root_spec.location.fractal_kind {
                corgi::types::FractalKind::Mandelbrot => {}
                corgi::types::FractalKind::Julia(pt) => {
                    point_edit(tui, "Julia parameter", Some("The C value used in the Julia equation. picking values from interesting locations in the Mandelbrot set tend to be interesting in the Julia Set."), get_precision(img.location.zoom), pt);
                }
            }
            input_with_label(
                tui,
                "Preview Scaling",
                Some("Scales the resolution of the preview image to improve performance."),
                egui::DragValue::new(&mut self.explore_state.scaling)
                    .speed(0.01)
                    .range(0.1..=1.0)
                    .max_decimals(2)
                    .update_while_editing(false),
            );
        });
        section(tui, "Viewport", true, |tui| {
            point_edit(
                tui,
                "Image Center",
                Some("The location of the center of the image in the complex plane."),
                get_precision(img.location.zoom),
                &mut self.root_spec.location.center,
            );
            input_with_label(
                tui,
                "Zoom",
                Some(
                    "Zoom level of the camera. Scales the range of the viewport by 2 raised to the negative of the zoom.",
                ),
                egui::DragValue::new(&mut self.root_spec.location.zoom)
                    .speed(0.03)
                    .update_while_editing(false),
            );
            input_with_label(
                tui,
                "Max iteration",
                Some(
                    "The maximum number of iterations to calculate before assuming a point is inside the set. Not all points will run this many iterations, some will quit early.",
                ),
                egui::DragValue::new(&mut self.root_spec.location.max_iter)
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
                    get_precision(img.location.zoom),
                    &mut self.root_spec.location.probe_location,
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
            input_with_label(
                tui,
                "Image width",
                None,
                egui::DragValue::new(&mut self.root_spec.width).speed(10.0),
            );
            input_with_label(
                tui,
                "Image height",
                None,
                egui::DragValue::new(&mut self.root_spec.height).speed(10.0),
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
                        self.root_spec.location.probe_location = view_image
                            .view()
                            .px_to_complex(pos, self.explore_state.scaling);
                        self.setting_probe = false;
                    }
                } else {
                    self.handle_viewport_input(ui, pointer_in_rect, &view_image);
                }

                self.current_view.width = size.x as u32;
                self.current_view.height = size.y as u32;

                let cb = PaintCallback {
                    rendered_viewport: match self.tab {
                        UITab::Render => self.render_state.rendered_view.clone(),
                        UITab::Explore => self.explore_state.rendered_view.clone(),
                        UITab::Style => self.style_state.rendered_view.clone(),
                    },
                    view: view_image.view(),
                    swap: self.swap,
                    tab: self.tab,
                };
                self.swap = false;

                let callback = egui_wgpu::Callback::new_paint_callback(rect, cb);

                // this paint call must be before others for some reason
                ui.painter().add(callback);
            },
        );
    }

    fn render_widgets(&self, ui: &mut egui::Ui, ctx: &egui::Context, context: &mut crate::Context) {
        fn paint_crosshair(
            painter: &egui::Painter,
            center: egui::Pos2,
            radius: f32,
            stroke: Stroke,
        ) {
            painter.line_segment(
                [
                    center + Vec2::new(0.0, -radius),
                    center + Vec2::new(0.0, radius),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    center + Vec2::new(-radius, 0.0),
                    center + Vec2::new(radius, 0.0),
                ],
                stroke,
            );
        }
        fn rotated_text(
            painter: &egui::Painter,
            ctx: &egui::Context,
            theme: &crate::config::Theme,
            pos: Pos2,
            anchor: egui::Align2,
            text: String,
            angle: f32,
        ) {
            let (t, r) = ctx.fonts_mut(|f| {
                let mut t = egui::Shape::text(
                    f,
                    pos,
                    anchor,
                    text,
                    egui::FontId::proportional(theme.base_rem),
                    Color32::WHITE,
                );

                let mut rect = egui::Rect::ZERO;
                let mut pos = Pos2::ZERO;
                if let egui::epaint::Shape::Text(ts) = &mut t {
                    *ts = ts.clone().with_angle_and_anchor(angle, anchor);
                    rect = ts.galley.rect.expand(theme.spacing / 2.0);
                    pos = ts.pos;
                };
                let rotator = egui::emath::Rot2::from_angle(angle);

                let r = egui::Shape::Path(egui::epaint::PathShape {
                    points: vec![
                        rect.left_top(),
                        rect.right_top(),
                        rect.right_bottom(),
                        rect.left_bottom(),
                    ]
                    .into_iter()
                    .map(|p| pos + rotator * p.to_vec2())
                    .collect(),
                    closed: true,
                    fill: Color32::from_black_alpha(150),
                    stroke: egui::epaint::PathStroke::NONE,
                });

                (t, r)
            });
            painter.add(r);
            painter.add(t);
        }
        let view_image = self.image();
        let camera_view = if self.tab == UITab::Render {
            self.render_state.rendered_view.clone()
        } else {
            self.root_spec.view()
        };
        let rect =
            egui::Rect::from_min_size(Pos2::ZERO, view_image.size() / self.viewport_scaling());
        let mut render_rect =
            rect.scale_from_center2(egui::Vec2::splat(1.0) / view_image.view().aspect_scale());
        let mut offset = view_image.view().complex_to_px_delta(&camera_view.center);
        offset.y *= -1.0;
        render_rect = render_rect.translate(offset * self.viewport_scaling());
        render_rect = render_rect.scale_from_center(f32::powf(
            2.0,
            -(camera_view.zoom - view_image.location.zoom),
        ));
        render_rect = render_rect.scale_from_center2(camera_view.aspect_scale());
        let painter = ui.painter();
        let simple_stroke = Stroke::new(2.0, Color32::WHITE);
        let center = egui::Pos2::ZERO + self.current_view.size() / 2.0;
        let current_angle = if self.tab == UITab::Render {
            self.current_view.angle - self.render_state.rendered_view.angle
        } else {
            self.root_spec.location.angle
        };
        let (pointer, modifiers, scroll) =
            ui.input(|i| (i.pointer.clone(), i.modifiers, i.smooth_scroll_delta));
        let theme = context.theme();
        if pointer.secondary_down()
            && let Some(pos) = pointer.latest_pos()
        {
            // render rotation guide line
            let current_angle = if modifiers.ctrl {
                (current_angle / (PI / 12.0)).round() * (PI / 12.0)
            } else {
                current_angle
            };
            let radius = (pos - center).length();
            painter.circle_stroke(center, radius, simple_stroke);
            let guideline = Vec2::new(radius, 0.0).rotated(current_angle);
            painter.line_segment([center, center + guideline], simple_stroke);
            rotated_text(
                painter,
                ctx,
                theme,
                center + guideline / 2.0 + Vec2::new(0.0, -theme.spacing).rotated(current_angle),
                egui::Align2::CENTER_BOTTOM,
                format!("{:.2}°", current_angle.to_degrees()),
                current_angle,
            );
        }
        if pointer.middle_down() {
            paint_crosshair(painter, center, 10.0, simple_stroke);
        }
        if (scroll.length() > 0.0 || self.setting_probe)
            && let Some(pos) = pointer.latest_pos()
        {
            paint_crosshair(painter, pos, 10.0, simple_stroke);
        }
        if self.show_camera || self.tab == UITab::Render {
            if self.tab != UITab::Render {
                painter.rect_stroke(
                    render_rect.intersect(rect),
                    0.0,
                    simple_stroke,
                    egui::StrokeKind::Outside,
                );
            }
            let current_angle = if self.tab == UITab::Render {
                current_angle
            } else {
                0.0
            };
            let text_anchor = render_rect.center()
                + (render_rect.center_bottom() - render_rect.center()
                    + Vec2::new(0.0, theme.spacing))
                .rotated(current_angle);

            rotated_text(
                painter,
                ctx,
                theme,
                text_anchor,
                egui::Align2::CENTER_TOP,
                format!("{}x{}", camera_view.width, camera_view.height),
                current_angle,
            );
        }
    }

    fn handle_viewport_input(
        &mut self,
        ui: &mut egui::Ui,
        pointer_in_rect: bool,
        view_image: &ImgSpec,
    ) {
        let response = ui.response();
        // get inputs to change the viewport
        let (mut scroll, pixel_scale, mouse, modifiers) = ui.input(|i| {
            (
                i.smooth_scroll_delta,
                i.pixels_per_point,
                i.pointer.clone(),
                i.modifiers,
            )
        });
        if !pointer_in_rect {
            scroll = Vec2::ZERO;
        }
        let drag = response.drag_delta();

        // calculate deltas
        let precision = get_precision(view_image.location.zoom);
        let mut scale = Float::with_val(precision, 2.0);
        scale.pow_assign(-view_image.location.zoom);
        let viewport_scaling = self.viewport_scaling();
        let ComplexPoint {
            x: x_offset,
            y: y_offset,
        } = view_image
            .view()
            .px_delta_to_complex_delta(drag, viewport_scaling);
        let mut scroll_zoom =
            if modifiers.shift { scroll.x } else { scroll.y } * pixel_scale * 0.005;
        let mut drag_zoom = (drag.x + drag.y) * pixel_scale * 0.01;
        let mut drag_rotation = if let Some(pos) = mouse.latest_pos() {
            let center = response.rect.size() / 2.0;
            let start_point = (pos - drag).to_vec2();
            let end_point = pos.to_vec2();
            (end_point - center).angle() - (start_point - center).angle()
        } else {
            0.0
        };

        // adjust behavior if modifiers are pressed
        if modifiers.shift {
            scroll_zoom *= 0.1;
            drag_zoom *= 0.1;
            drag_rotation *= 0.1;
        }
        if modifiers.alt {
            scroll_zoom *= 3.0;
            drag_zoom *= 3.0;
            drag_rotation *= 3.0;
        }

        // apply deltas to relevant view or location
        if self.tab == UITab::Render {
            self.current_view.zoom += scroll_zoom;
            if scroll.y != 0.0
                && let Some(pos) = mouse.latest_pos()
            {
                let unzoomed_real = view_image.view().px_to_complex(pos, viewport_scaling);
                let zoomed_real = self.current_view.px_to_complex(pos, viewport_scaling);
                self.current_view.center.x -= zoomed_real.x - unzoomed_real.x;
                self.current_view.center.y -= zoomed_real.y - unzoomed_real.y;
            }
            if mouse.primary_down() {
                self.current_view.center.x -= x_offset;
                self.current_view.center.y -= y_offset;
            }
            if mouse.secondary_down() {
                self.current_view.angle += drag_rotation;
                self.current_view.angle %= PI * 2.0;
            }
            if mouse.middle_down() {
                self.current_view.zoom += drag_zoom;
            }
            if modifiers.ctrl && mouse.secondary_released() {
                let reference_angle = self.render_state.rendered_view.angle;
                // round values
                self.current_view.angle =
                    ((self.current_view.angle - reference_angle) / (PI / 12.0)).round()
                        * (PI / 12.0)
                        + reference_angle;
            }
        } else {
            self.root_spec.location.zoom += scroll_zoom;
            if scroll.y != 0.0
                && let Some(pos) = mouse.latest_pos()
            {
                let unzoomed_real = view_image.view().px_to_complex(pos, viewport_scaling);
                let zoomed_real = self.image().view().px_to_complex(pos, viewport_scaling);
                self.root_spec.location.center.x -= zoomed_real.x - unzoomed_real.x;
                self.root_spec.location.center.y -= zoomed_real.y - unzoomed_real.y;
            }
            // including "none" down to support touch
            if mouse.primary_down() || !mouse.any_down() {
                self.root_spec.location.center.x -= x_offset;
                self.root_spec.location.center.y -= y_offset;
            }
            if mouse.secondary_down() {
                self.root_spec.location.angle += drag_rotation;
                self.root_spec.location.angle %= PI * 2.0;
            }
            if mouse.middle_down() {
                self.root_spec.location.zoom += drag_zoom;
            }
            if modifiers.ctrl && mouse.secondary_released() {
                // round values
                self.root_spec.location.angle =
                    (self.root_spec.location.angle / (PI / 12.0)).round() * (PI / 12.0);
            }
            self.root_spec.location.update_prec();
            self.root_spec.update_probe();
        }
    }

    fn viewport_scaling(&self) -> f32 {
        match self.tab {
            UITab::Explore => self.explore_state.scaling,
            UITab::Style => self.style_state.scaling,
            UITab::Render => 1.0,
        }
    }

    pub fn update_rendered_view(&mut self, id: RendererId, viewport: View) {
        match id {
            RendererId::Explore => {
                self.explore_state.rendered_view = viewport;
            }
            RendererId::Style => {
                self.style_state.rendered_view = viewport;
            }
            RendererId::Render => {
                self.render_state.rendered_view = viewport.clone();
                let mut view = viewport.clone();
                view.width = self.current_view.width;
                view.height = self.current_view.height;
                self.current_view = view;
                self.current_view.zoom_to_fit(&viewport);
                self.rendering_output = false;
            }
        }
    }

    pub fn renderer(&self) -> RendererId {
        match self.tab {
            UITab::Explore => RendererId::Explore,
            UITab::Style => RendererId::Style,
            UITab::Render => RendererId::Render,
        }
    }
}
