/*!

# Corgi UI

This module contains the main UI state struct and its implementation, which
contains the code necessary to update internal state and render the ui.
 */

use std::path::PathBuf;
use std::sync::mpsc;

use corgi_lib::image_gen::CompressionParams;
use corgi_lib::types::{ComplexPoint, ImgSpec, OptLevel, Style as ImgStyle, View};
use documented::DocumentedFields;
use eframe::egui::containers::menu::MenuButton;
use eframe::egui::{
    Button, CornerRadius, Frame, ScrollArea, Sense, Separator, TextEdit, TextStyle, UiBuilder, Vec2,
};
use eframe::{egui, egui_wgpu};
use egui_material_icons::icons;
use egui_taffy::{TuiBuilderLogic, tui};
use preset_library::PresetLibrary;
use rug::Float;
use taffy::prelude::*;
use utils::{input_with_label, section};

use crate::app::Status;
use crate::config::corgi_project_dirs;
use crate::ui::preview_resources::ThumbPaintCallback;
use crate::ui::tabs::{ExploreTabState, RenderState, RenderTabState, StyleTabState, UITab};
use crate::ui::utils::{StyleExt, indent_with_line, raw_selection, ui_with_label};
use crate::worker::{ImageGenCommand, RendererId};

mod coloring;
pub mod debouncer;
mod preset_library;
mod preview_resources;
mod settings;
mod shortcuts;
mod tabs;
mod utils;
mod viewport;

pub use preview_resources::{PreviewRenderResources, ThumbnailRenderResources};

/// Utility trait for rendering UI
pub trait EditUI {
    /// Renders the UI to edit `self`, mutating it in response
    fn render_edit_ui(&mut self, ctx: &egui::Context, tui: &mut egui_taffy::Tui);
}

#[derive(Debug)]
struct ActiveFile {
    path: PathBuf,
    last_saved_spec: ImgSpec,
}

#[expect(clippy::type_complexity)]
struct DynCallback(Box<dyn FnOnce(&mut CorgiUI, &mut crate::Context, &egui::Context)>);

impl std::fmt::Debug for DynCallback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Dyncallback")
    }
}

/// The main UI state struct.
#[derive(Debug)]
pub struct CorgiUI {
    tab: UITab,
    explore_state: ExploreTabState,
    style_state: StyleTabState,
    render_state: RenderTabState,
    /// The current target image to render
    root_spec: ImgSpec,
    current_view: View,
    setting_probe: bool,
    show_camera: bool,
    show_settings: bool,
    /// Whether the viewport should load the current image from
    /// the rendering thread into the UI view.
    pub swap: bool,
    pub status: Status,
    command_channel: mpsc::Sender<ImageGenCommand>,
    preset_save_active: Option<UITab>,
    preset_name: String,
    preset_group: String,
    new_group_active: bool,
    active_file: Option<ActiveFile>,
    confirm: Option<(String, Vec<(String, DynCallback)>)>,
}

impl CorgiUI {
    /// Create a new state struct; status should be shared with the render thread.
    pub fn new(
        context: &crate::Context,
        image: ImgSpec,
        input_path: Option<PathBuf>,
        command_channel: mpsc::Sender<ImageGenCommand>,
    ) -> Self {
        let dirs = corgi_project_dirs();
        let install_dir = std::env::var("CARGO_MANIFEST_DIR")
            .map(PathBuf::from)
            .unwrap_or(
                std::env::current_exe()
                    .ok()
                    .and_then(|p| p.parent().map(|p| p.to_path_buf()))
                    .unwrap_or("./".into()),
            );
        let base_dirs = [
            dirs.config_dir().join("presets"),
            install_dir.join("presets"),
        ];

        let active_file = if let Some(path) = input_path {
            ActiveFile::try_new(image.clone(), path)
        } else {
            None
        };
        Self {
            tab: UITab::Explore,
            explore_state: ExploreTabState {
                rendered_view: image.view(),
                style: ImgStyle::opt_default(),
                scaling: 0.5,
                location_presets: PresetLibrary::new(
                    base_dirs.iter().map(|p| p.join("locations")).collect(),
                ),
                opt_level: OptLevel::PerformanceOptimized,
            },
            style_state: StyleTabState {
                rendered_view: image.view(),
                scaling: 1.0,
                style_presets: PresetLibrary::new(
                    base_dirs.iter().map(|p| p.join("styles")).collect(),
                ),
            },
            render_state: RenderTabState {
                exr_mode: false,
                state: RenderState::Init,
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
            preset_save_active: None,
            preset_name: "New Preset".into(),
            preset_group: "New Group".into(),
            new_group_active: false,
            status: Status::default(),
            command_channel,
            active_file,
            confirm: None,
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

                self.menu(context, ctx, ui);
                ScrollArea::vertical().show(ui, |ui| {
                    tui(ui, ui.id().with("side"))
                        .reserve_available_width()
                        .style(Style::col())
                        .show(|tui| match self.tab {
                            UITab::Explore => self.explore_tab(context, tui),
                            UITab::Style => self.style_tab(ctx, context, tui),
                            UITab::Render => self.render_tab(ctx, context, cancel, tui),
                        });
                });
            });

        let res = egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(ctx.style().visuals.window_fill))
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let origin_rect = ui.available_rect_before_wrap();
                let hover_pt = self.viewport(ui, ctx);
                self.render_widgets(ui, ctx, context);
                self.footer(ui, context, hover_pt);
                crate::app_log::logs_ui(ui, origin_rect);
            });

        let style = ctx.style().clone();
        let mut show_settings = self.show_settings;
        egui::Window::new("Settings")
            .open(&mut show_settings)
            .default_pos(res.response.rect.center())
            .pivot(egui::Align2::CENTER_BOTTOM)
            .show(ctx, |ui| {
                ui.set_style(style);
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);

                let previous_config = context.config().clone();
                tui(ui, ui.id().with("settings"))
                    .reserve_available_width()
                    .style(Style::col())
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
                    self.send(ImageGenCommand::UpdateConstants(
                        RendererId::Explore,
                        corgi_lib::image_gen::Constants {
                            iter_batch_size: context.config().ui_max_shader_batch_iters,
                        },
                    ));
                    self.send(ImageGenCommand::UpdateConstants(
                        RendererId::Style,
                        corgi_lib::image_gen::Constants {
                            iter_batch_size: context.config().ui_max_shader_batch_iters,
                        },
                    ));
                }
                if context.config().max_shader_batch_iters != previous_config.max_shader_batch_iters
                {
                    self.send(ImageGenCommand::UpdateConstants(
                        RendererId::Render,
                        corgi_lib::image_gen::Constants {
                            iter_batch_size: context.config().max_shader_batch_iters,
                        },
                    ));
                }
            });
        self.show_settings = show_settings;

        let style = ctx.style().clone();
        let mut open = self.preset_save_active.is_some();
        egui::Window::new("Save Preset")
            .open(&mut open)
            .default_size(Vec2::splat(300.0))
            .default_pos(res.response.rect.center())
            .pivot(egui::Align2::CENTER_BOTTOM)
            .show(ctx, |ui| {
                ui.set_style(style);
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                self.save_preset_window(context, ui);
            });
        if !open {
            self.preset_save_active = None;
        }

        let style = ctx.style().clone();
        egui::Window::new("New Group")
            .open(&mut self.new_group_active)
            .default_pos(res.response.rect.center())
            .pivot(egui::Align2::CENTER_TOP)
            .show(ctx, |ui| {
                ui.set_style(style);
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);

                ui.add(
                    egui::TextEdit::singleline(&mut self.preset_group)
                        .desired_width(ui.available_width() - ui.spacing().button_padding.x * 2.0)
                        .margin(ui.spacing().button_padding),
                );
                ui.horizontal(|ui| {
                    if ui.button("Create").clicked() {
                        let preset_library = match self.preset_save_active {
                            Some(UITab::Explore) => &mut self.explore_state.location_presets,
                            Some(UITab::Style) => &mut self.style_state.style_presets,
                            _ => {
                                ui.close_kind(egui::UiKind::Window);
                                return;
                            }
                        };
                        if let Err(err) = preset_library.create_group(&self.preset_group) {
                            tracing::error!("Failed to create group: {err}");
                        }
                        ui.close_kind(egui::UiKind::Window);
                    }
                    if ui.button("Cancel").clicked() {
                        ui.close_kind(egui::UiKind::Window);
                    }
                });
            });

        if ctx.input(|i| i.viewport().close_requested()) {
            // check if we need to save
            if self
                .active_file
                .as_ref()
                .is_none_or(|af| af.last_saved_spec != self.root_spec)
                && self.root_spec != ImgSpec::default()
            {
                self.confirm = Some((
                    format!(
                        "Save {}?",
                        self.active_file
                            .as_ref()
                            .map(|af| af.path.to_string_lossy().to_string())
                            .unwrap_or("unsaved settings".into())
                    ),
                    vec![
                        (
                            "Save".into(),
                            DynCallback(Box::new(|ui_state, context, ctx| {
                                if ui_state.save(context) {
                                    ui_state.force_close(ctx);
                                }
                            })),
                        ),
                        (
                            "Don't Save".into(),
                            DynCallback(Box::new(|ui_state, _, ctx| {
                                ui_state.force_close(ctx);
                            })),
                        ),
                        ("Cancel".into(), DynCallback(Box::new(|_, _, _| {}))),
                    ],
                ));
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
        }

        self.show_confirm_dialog(ctx, context, res);
        self.handle_shortcuts(context, ctx);

        self.swap = false;
    }

    fn show_confirm_dialog(
        &mut self,
        ctx: &egui::Context,
        context: &mut crate::config::Context,
        res: egui::InnerResponse<()>,
    ) {
        if let Some(mut confirm) = self.confirm.take() {
            let style = ctx.style().clone();
            egui::Window::new(&confirm.0)
                .collapsible(false)
                .default_pos(res.response.rect.center())
                .pivot(egui::Align2::CENTER_TOP)
                .show(ctx, |ui| {
                    ui.set_style(style);
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);

                    ui.horizontal(|ui| {
                        let mut selection = None;
                        for (index, (label, _)) in confirm.1.iter().enumerate() {
                            if ui.button(label).clicked() {
                                selection = Some(index)
                            }
                        }
                        if let Some(selection) = selection {
                            confirm.1.remove(selection).1.0(self, context, ctx);
                        } else {
                            self.confirm = Some(confirm);
                        }
                    });
                });
        }
    }

    fn menu(
        &mut self,
        context: &mut crate::config::Context,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
    ) {
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
            {
                let spacing = ui.spacing().button_padding.y;
                MenuButton::from_button(Button::new(icons::ICON_MENU)).ui(ui, |ui| {
                    {
                        let style = ui.style_mut();
                        style.spacing.item_spacing = Vec2::splat(spacing);
                        style.visuals.widgets.inactive.corner_radius =
                            CornerRadius::same(spacing as u8);
                        style.visuals.widgets.active.corner_radius =
                            CornerRadius::same(spacing as u8);
                        style.visuals.widgets.hovered.corner_radius =
                            CornerRadius::same(spacing as u8);
                        style.spacing.button_padding = Vec2::splat(spacing);
                    }

                    if ui
                        .add(
                            Button::new(if self.active_file.is_some() {
                                "Save"
                            } else {
                                "Save…"
                            })
                            .shortcut_text(ui.ctx().format_shortcut(&shortcuts::SAVE)),
                        )
                        .clicked()
                    {
                        self.save(context);
                    }
                    if ui
                        .add(
                            Button::new("Save As…")
                                .shortcut_text(ui.ctx().format_shortcut(&shortcuts::SAVE_AS)),
                        )
                        .clicked()
                    {
                        self.save_as(context);
                    }
                    if ui
                        .add(
                            Button::new("Open…")
                                .shortcut_text(ui.ctx().format_shortcut(&shortcuts::OPEN)),
                        )
                        .clicked()
                    {
                        self.open(context);
                    }
                    ui.separator();
                    if ui.add(Button::new("Settings")).clicked() {
                        self.show_settings = true;
                    }
                    ui.separator();
                    if ui
                        .add(
                            Button::new(
                                if self
                                    .active_file
                                    .as_ref()
                                    .is_some_and(|af| af.last_saved_spec == self.root_spec)
                                    || self.root_spec == ImgSpec::default()
                                {
                                    "Exit"
                                } else {
                                    "Exit…"
                                },
                            )
                            .shortcut_text(ui.ctx().format_shortcut(&shortcuts::CLOSE)),
                        )
                        .clicked()
                    {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            };
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
        ui.add(Separator::default().spacing(ui.visuals().widgets.noninteractive.bg_stroke.width));
        ui.style_mut().spacing.item_spacing.y = y;
    }

    fn footer(
        &mut self,
        ui: &mut egui::Ui,
        context: &crate::Context,
        hover_pt: Option<ComplexPoint>,
    ) {
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
            ui.label(&self.status.message);
            if let Some(af) = self.active_file.as_ref() {
                ui.separator();
                ui.label(format!(
                    "Editing {}{}",
                    if af.last_saved_spec == self.root_spec {
                        ""
                    } else {
                        "* "
                    },
                    af.path.file_name().unwrap_or_default().to_string_lossy()
                ));
            }
            if let Some(hover_pt) = hover_pt {
                fn format_float(mut x: Float, zoom: f32) -> String {
                    x.set_prec(zoom.max(53. - 9.) as u32 + 9);
                    let mut x = format!("{}", x);
                    if x.len() > 25 {
                        let chars: Vec<_> = x.chars().collect();
                        let start: String = chars[..5].iter().collect();
                        let end: String = chars[(chars.len() - 10)..].iter().collect();
                        let digits = chars.len() - 15;
                        x = format!("{}...[{}]...{}", start, digits, end);
                    }
                    x
                }

                let zoom = self.image().view().zoom;
                ui.separator();
                ui.label(format!(
                    "{} + {}i",
                    format_float(hover_pt.x.clone(), zoom),
                    format_float(hover_pt.y.clone(), zoom)
                ));

                if ui.input(|inp| inp.events.iter().any(|ev| matches!(ev, egui::Event::Copy))) {
                    let text = if context.config().show_debug_options {
                        // the format used in tests
                        format!("(\"{}\", \"{}\")", hover_pt.x, hover_pt.y)
                    } else {
                        format!("{} + {}i", hover_pt.x, hover_pt.y)
                    };
                    ui.ctx().copy_text(text);
                    self.status.message = "Copied location to clipboard".into();
                    self.status.progress = None;
                }
            }
        });
    }

    fn save_preset_window(&mut self, context: &mut crate::config::Context, ui: &mut egui::Ui) {
        let item_spacing = ui.spacing().item_spacing;

        tui(ui, ui.id().with("presets"))
            .reserve_available_space()
            .style(Style::col().center().gap(item_spacing.y))
            .show(|tui| {
                tui.ui_add_manual(
                    |ui| {
                        ui.scope_builder(
                            UiBuilder::new().max_rect(egui::Rect::from_min_size(
                                ui.cursor().min,
                                Vec2::splat(context.config().thumbnail_size as f32),
                            )),
                            |ui| {
                                ui.painter().add(egui_wgpu::Callback::new_paint_callback(
                                    ui.available_rect_before_wrap(),
                                    ThumbPaintCallback {
                                        size: (
                                            context.config().thumbnail_size,
                                            context.config().thumbnail_size,
                                        ),
                                        swap: self.swap,
                                    },
                                ));
                                ui.allocate_rect(ui.available_rect_before_wrap(), Sense::empty());
                            },
                        )
                        .response
                    },
                    |res, _| res,
                );
                ui_with_label(
                    tui,
                    "Name",
                    Some("The name to save the preset under"),
                    |tui| {
                        let margin = tui.egui_ui().spacing().button_padding;
                        let width =
                            tui.egui_ui().available_width() - margin.x * 2.0 - item_spacing.x;
                        tui.style(Style::grow()).ui_add(
                            TextEdit::singleline(&mut self.preset_name)
                                .desired_width(width)
                                .margin(margin)
                                .horizontal_align(egui::Align::Max),
                        )
                    },
                );
                let preset_library = match self.preset_save_active {
                    Some(UITab::Explore) => &mut self.explore_state.location_presets,
                    Some(UITab::Style) => &mut self.style_state.style_presets,
                    _ => {
                        tui.egui_ui().close_kind(egui::UiKind::Window);
                        return;
                    }
                };

                tui.style(Style::row().gap(item_spacing.x)).add(|tui| {
                    ui_with_label(
                        tui,
                        "Group",
                        Some("The group the preset will be saved in"),
                        |tui| {
                            raw_selection(
                                tui,
                                "Group",
                                None,
                                &mut self.preset_group,
                                preset_library.group_names(),
                            )
                        },
                    );
                    if tui.ui_add(Button::new("New Group")).clicked() {
                        self.new_group_active = true;
                    }
                });
                tui.style(Style::row().gap(item_spacing.x)).add(|tui| {
                    if tui
                        .style(Style::grow())
                        .ui_add(Button::new("Save"))
                        .clicked()
                    {
                        match preset_library.create_preset(&self.preset_name, &self.preset_group) {
                            Ok(path) => {
                                if self
                                    .command_channel
                                    .send(ImageGenCommand::SaveToFile(
                                        RendererId::Thumbnail,
                                        path,
                                        CompressionParams {
                                            speed: 1,
                                            quality: 50,
                                        },
                                        Some(self.preset_name.clone()),
                                    ))
                                    .is_err()
                                {
                                    tracing::error!(
                                        "Worker thread has stopped; please restart the app."
                                    );
                                }
                                tui.egui_ui().close_kind(egui::UiKind::Window);
                            }
                            Err(err) => tracing::error!("Failed to create preset: {err}"),
                        }
                    }
                    if tui
                        .style(Style::grow())
                        .ui_add(Button::new("Cancel"))
                        .clicked()
                    {
                        tui.egui_ui().close_kind(egui::UiKind::Window);
                    }
                })
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
                active_image.optimization_level = self.explore_state.opt_level;
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

    /// Sends a render command for the current tab to the worker thread
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
                self.render_state.state = RenderState::Rendered;
            }
            RendererId::Thumbnail => {}
        }
    }

    fn send(&self, cmd: ImageGenCommand) {
        if self.command_channel.send(cmd).is_err() {
            tracing::error!("Worker thread has stopped; please restart the app.");
        }
    }
}

impl ActiveFile {
    fn try_new(image: ImgSpec, path: PathBuf) -> Option<ActiveFile> {
        if path.exists()
            && path.metadata().is_ok_and(|m| !m.permissions().readonly())
            && path
                .extension()
                .is_some_and(|ext| ["corg", "json"].contains(&ext.to_str().unwrap_or_default()))
        {
            Some(ActiveFile {
                path,
                last_saved_spec: image,
            })
        } else {
            None
        }
    }
}

impl EditUI for CompressionParams {
    fn render_edit_ui(&mut self, _ctx: &egui::Context, tui: &mut egui_taffy::Tui) {
        tui.label("Compression");
        indent_with_line(tui, |tui| {
            input_with_label(
                tui,
                "Speed",
                Some(Self::get_field_docs("speed").unwrap()),
                egui::DragValue::new(&mut self.speed)
                    .speed(0.1)
                    .range(1..=100),
            );
            input_with_label(
                tui,
                "Quality",
                Some(Self::get_field_docs("quality").unwrap()),
                egui::DragValue::new(&mut self.quality)
                    .speed(0.1)
                    .range(1..=100),
            );
        });
    }
}
