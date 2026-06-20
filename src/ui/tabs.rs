use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use corgi_lib::types::{OptLevel, Style as ImgStyle, View, get_precision};
use directories::BaseDirs;
use eframe::egui::{self, Button, CornerRadius, TextStyle, WidgetText};
use egui_material_icons::icons;
use egui_taffy::TuiBuilderLogic;
use taffy::prelude::*;

use super::preset_library::PresetLibrary;
use super::utils::{collapsible, input_with_label, point_edit, section};
use crate::ui::EditUI;
use crate::ui::utils::{StyleExt, selection_with_label};
use crate::worker::{ImageGenCommand, RendererId};

#[derive(Debug)]
pub struct ExploreTabState {
    pub rendered_view: View,
    pub style: ImgStyle,
    pub scaling: f32,
    pub location_presets: PresetLibrary,
    pub opt_level: OptLevel,
}

#[derive(Debug)]
pub struct StyleTabState {
    pub rendered_view: View,
    pub scaling: f32,
    pub style_presets: PresetLibrary,
}

#[derive(Debug)]
pub struct RenderTabState {
    pub exr_mode: bool,
    pub state: RenderState,
    pub rendered_view: View,
    pub save_path: PathBuf,
}

#[derive(Debug, PartialEq)]
pub enum RenderState {
    Init,
    Rendering,
    Cancelled,
    Rendered,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UITab {
    Explore,
    Style,
    Render,
}

impl super::CorgiUI {
    pub(super) fn explore_tab(&mut self, context: &crate::Context, tui: &mut egui_taffy::Tui) {
        let img = self.image();
        let item_spacing = tui.egui_ui().spacing().item_spacing;

        let mut activate_preset = false;
        self.explore_state.location_presets.render_ui(
            |new_spec| self.root_spec.location = new_spec.location.clone(),
            &mut activate_preset,
            tui,
            true,
        );
        if activate_preset {
            self.preset_save_active = Some(UITab::Explore);
            self.preset_group = self
                .explore_state
                .location_presets
                .group_names()
                .first()
                .unwrap_or(&self.preset_group)
                .clone();
            let mut img = self.image().clone();
            img.location.zoom = self.root_spec.location.zoom;
            img.width = context.config().thumbnail_size;
            img.height = context.config().thumbnail_size;
            self.send(ImageGenCommand::Render(
                RendererId::Thumbnail,
                Box::new(img),
            ));
        }
        tui.style(Style::col().top(item_spacing.y * 3.0).side(item_spacing.y * 3.0).gap(item_spacing.y * 2.0))
        .add(|tui| {
            selection_with_label(
                tui,
                "Fractal Mode",
                Some("Which fractal algorithm to use. Switching from Mandelbrot to Julia will set the Julia parameter to the current view center."),
                &mut self.root_spec.location.fractal_kind,
                vec![
                    corgi_lib::types::FractalKind::Mandelbrot,
                    corgi_lib::types::FractalKind::Julia(img.location.center.clone()),
                ],
            );
            match &mut self.root_spec.location.fractal_kind {
                corgi_lib::types::FractalKind::Mandelbrot => {}
                corgi_lib::types::FractalKind::Julia(pt) => {
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
        section(tui, "Camera", true, |tui| {
            tui.egui_style_mut().spacing.icon_width = tui.egui_ui().spacing().icon_width * 1.5;
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
        if context.config().show_debug_options {
            section(tui, "Debug", true, |tui| {
                tui.egui_style_mut().spacing.icon_width = tui.egui_ui().spacing().icon_width * 1.5;
                selection_with_label(
                    tui,
                    "Opt Level",
                    Some("Determines which algorithm to use"),
                    &mut self.explore_state.opt_level,
                    vec![
                        OptLevel::PerformanceOptimized,
                        OptLevel::CacheOptimized,
                        OptLevel::CPUOnly,
                        OptLevel::HighPrecisionFloat,
                    ],
                );
            });
        }
    }

    pub(super) fn style_tab(
        &mut self,
        ctx: &egui::Context,
        context: &mut crate::config::Context,
        tui: &mut egui_taffy::Tui,
    ) {
        let mut activate_preset = false;
        self.style_state.style_presets.render_ui(
            |new_spec| self.root_spec.style = new_spec.style.clone(),
            &mut activate_preset,
            tui,
            false,
        );
        if activate_preset {
            self.preset_save_active = Some(UITab::Style);
            self.preset_group = self
                .style_state
                .style_presets
                .group_names()
                .first()
                .unwrap_or(&self.preset_group)
                .clone();
            let mut img = self.image().clone();
            img.location.zoom = self.root_spec.location.zoom;
            img.width = context.config().thumbnail_size;
            img.height = context.config().thumbnail_size;
            self.send(ImageGenCommand::Render(
                RendererId::Thumbnail,
                Box::new(img),
            ));
        }
        section(tui, "External", true, |tui| {
            self.root_spec
                .style
                .external_coloring
                .render_edit_ui(ctx, tui);
        });
        section(tui, "Internal", true, |tui| {
            self.root_spec
                .style
                .internal_coloring
                .render_edit_ui(ctx, tui);
        });
    }

    pub(super) fn render_tab(
        &mut self,
        ctx: &egui::Context,
        context: &mut crate::config::Context,
        cancel: impl FnOnce(),
        tui: &mut egui_taffy::Tui,
    ) {
        tui.style(Style::row()).add(|tui| {
            tui.egui_style_mut().visuals.widgets.inactive.corner_radius = CornerRadius::ZERO;
            tui.egui_style_mut().visuals.widgets.active.corner_radius = CornerRadius::ZERO;
            tui.egui_style_mut().visuals.widgets.hovered.corner_radius = CornerRadius::ZERO;
            if tui
                .style(Style::grow().center())
                .selectable(!self.render_state.exr_mode, |tui| {
                    let fill = if !self.render_state.exr_mode {
                        tui.egui_ui().style().visuals.window_fill
                    } else {
                        tui.egui_ui().visuals().text_color()
                    };
                    tui.label(egui::RichText::new("Save Image").heading().color(fill));
                })
                .clicked()
            {
                self.render_state.exr_mode = false;
                self.render_state.state = RenderState::Init;
            }
            if tui
                .style(Style::grow().center())
                .selectable(self.render_state.exr_mode, |tui| {
                    let fill = if self.render_state.exr_mode {
                        tui.egui_ui().style().visuals.window_fill
                    } else {
                        tui.egui_ui().visuals().text_color()
                    };
                    tui.label(egui::RichText::new("Export Data").heading().color(fill));
                })
                .clicked()
            {
                self.render_state.exr_mode = true;
                self.render_state.state = RenderState::Init;
            }
        });
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
                egui::DragValue::new(&mut self.root_spec.height).speed(10.0),
            );
            tui.style(Style::row()).add(|tui| {
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
                let text_size = WidgetText::Text(icons::ICON_FOLDER_OPEN.to_owned())
                    .into_galley(
                        tui.egui_ui(),
                        None,
                        tui.egui_ui().available_width(),
                        TextStyle::Button,
                    )
                    .size();
                let spacing = tui.egui_ui().spacing().clone();
                let available_width = tui.egui_ui().available_width();
                tui.style(Style::grow()).ui_add(
                    egui::TextEdit::singleline(&mut str_path)
                        .margin(spacing.button_padding)
                        .desired_width(
                            available_width
                                - text_size.x
                                - spacing.button_padding.x * 4.0
                                - spacing.item_spacing.x,
                        ),
                );
                if let Some(home_dir) = &home_opt
                    && str_path.starts_with("~/")
                {
                    str_path = str_path.replacen("~", home_dir, 1);
                }
                self.render_state.save_path = str_path.into();

                if tui.ui_add(Button::new(icons::ICON_FOLDER_OPEN)).clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .set_directory(&self.render_state.save_path)
                        .pick_folder()
                {
                    context.cache_mut().previous_paths.image = path.clone();
                    self.render_state.save_path = path;
                }
            });
            if self.render_state.exr_mode {
                // TODO: add layer selection
            } else {
                let mut compression_params = context.cache().compression_params;
                compression_params.render_edit_ui(ctx, tui);
                if context.cache().compression_params != compression_params {
                    context.cache_mut().compression_params = compression_params;
                }
            }
        });
        let item_spacing = tui.egui_ui().spacing().item_spacing;
        tui.style(
            Style::col()
                .pad(item_spacing.y * 3.0)
                .gap(item_spacing.y * 2.0),
        )
        .add(|tui| {
            if self.render_state.state != RenderState::Rendering {
                if tui.ui_add(Button::new("Render")).clicked() {
                    let mut image = self.root_spec.clone();
                    if self.render_state.exr_mode {
                        image.optimization_level = OptLevel::CacheOptimized;
                    }
                    self.send(ImageGenCommand::Render(RendererId::Render, Box::new(image)));
                    self.render_state.state = RenderState::Rendering;
                }
            } else if tui.ui_add(Button::new("Cancel Render")).clicked() {
                self.render_state.state = RenderState::Cancelled;
                cancel();
            }
            if tui
                .enabled_ui(self.render_state.state == RenderState::Rendered)
                .ui_add(Button::new(if self.render_state.exr_mode {
                    "Save as EXR"
                } else {
                    "Save Image"
                }))
                .clicked()
            {
                let mut dialog = rfd::FileDialog::new().set_directory(&self.render_state.save_path);
                if self.render_state.exr_mode {
                    dialog = dialog.add_filter("EXR", &["exr"]);
                } else {
                    dialog = dialog
                        .add_filter(
                            "image with metadata",
                            &["avif", "jpg", "jpeg", "webp", "png"],
                        )
                        .add_filter("image without metadata", &["gif", "qoi", "tiff"])
                }
                let extension = if self.render_state.exr_mode {
                    "exr"
                } else {
                    &context.cache().default_image_type
                };
                if let Some(path) = dialog
                    .set_file_name(format!("fractal.{}", extension))
                    .save_file()
                {
                    if let Some(ext) = path.extension().and_then(OsStr::to_str)
                        && !self.render_state.exr_mode
                    {
                        context.cache_mut().default_image_type = ext.to_owned();
                    }
                    self.send(ImageGenCommand::SaveToFile(
                        RendererId::Render,
                        path.clone(),
                        context.cache().compression_params,
                        None,
                    ));
                }
            }
        });
    }

    pub(super) fn viewport_scaling(&self) -> f32 {
        match self.tab {
            UITab::Explore => self.explore_state.scaling,
            UITab::Style => self.style_state.scaling,
            UITab::Render => 1.0,
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
