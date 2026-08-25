/*!

# Corgi UI

This module contains the main UI state struct and its implementation, which
contains the code necessary to update internal state and render the ui.
 */

use std::path::{Path, PathBuf};
use std::sync::{LazyLock, mpsc};
use std::time::Duration;

use cargo_packager_resource_resolver::{current_format, resources_dir};
use corgi_lib::image_gen::CompressionParams;
use corgi_lib::types::serde::SafeSaveLoad;
use corgi_lib::types::{ComplexPoint, ImgSpec, OptLevel, Style as ImgStyle, View, get_precision};
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
use rug::ops::CompleteRound;
use taffy::prelude::*;
use tracing::warn;
use utils::{input_with_label, section};

use crate::app::Status;
use crate::config::corgi_project_dirs;
use crate::share::{create_share_string, parse_share_string};
use crate::ui::preview_resources::ThumbPaintCallback;
use crate::ui::tabs::{ExploreTabState, RenderState, RenderTabState, StyleTabState, UITab};
use crate::ui::undo::EditHistory;
use crate::ui::utils::{StyleExt, indent_with_line, raw_selection, ui_with_label};
use crate::worker::{ImageGenCommand, RendererId};

mod coloring;
pub mod debouncer;
mod docs;
mod preset_library;
mod preview_resources;
mod settings;
mod shortcuts;
mod splash;
mod tabs;
mod undo;
mod utils;
mod viewport;

pub use preview_resources::{PreviewRenderResources, ThumbnailRenderResources};

/// Utility trait for rendering UI
pub trait EditUI {
    /// Renders the UI to edit `self`, mutating it in response
    fn render_edit_ui(&mut self, tui: &mut egui_taffy::Tui);
}

fn _recolor_svg<const N: usize>(
    data: &'static [u8; N],
    accent_pattern: &str,
    bg_pattern: &str,
    accent: egui::Color32,
    bg: egui::Color32,
) -> std::sync::Arc<[u8]> {
    let string = String::from_utf8_lossy(data)
        .replace(accent_pattern, &accent.to_hex())
        .replace(bg_pattern, &bg.to_hex());
    string.as_bytes().to_vec().into()
}

macro_rules! recolor_svg {
    ($path:literal, $accent_pattern:literal => $accent:expr, $bg_pattern:literal => $bg:expr) => {
        egui::ImageSource::Bytes {
            uri: ::std::borrow::Cow::Borrowed(concat!("bytes://", $path)),
            bytes: egui::load::Bytes::Shared(crate::ui::_recolor_svg(include_bytes!($path), $accent_pattern, $bg_pattern, $accent, $bg)),
        }
    };
    ($path:literal, $pattern:literal => $color:expr) => {
        recolor_svg!($path, $pattern => $color, "$000" => egui::Color32::BLACK)
    };
}
use recolor_svg;

#[derive(Debug)]
struct ActiveFile {
    path: PathBuf,
    last_saved_spec: ImgSpec,
}

#[expect(clippy::type_complexity)]
struct DynCallback(Box<dyn FnOnce(&mut CorgiUI, &mut crate::Context, &egui::Ui)>);

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
    pub swap: Option<RendererId>,
    pub status: Status,
    command_channel: mpsc::Sender<ImageGenCommand>,
    preset_save_active: Option<UITab>,
    preset_name: String,
    preset_group: String,
    new_group_active: bool,
    share_active: bool,
    docs_state: docs::DocsState,
    active_file: Option<ActiveFile>,
    confirm: Option<(String, Vec<(String, DynCallback)>)>,
    show_splash: bool,
    history: EditHistory<ImgSpec>,
}

static RESOURCE_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        return manifest_dir.into();
    }
    match current_format().and_then(resources_dir) {
        Ok(path) => return path,
        Err(err) => warn!("Failed to load package path: {err}"),
    }
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or("./".into())
});

fn resources_directory() -> &'static Path {
    &RESOURCE_DIR
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
        let base_dirs = [
            dirs.config_dir().join("presets"),
            resources_directory().join("presets"),
        ];

        let active_file = if let Some(path) = input_path {
            ActiveFile::try_new(image.clone(), path)
        } else {
            None
        };
        let default_image = image == ImgSpec::default();
        let root_spec = ImgSpec {
            optimization_level: OptLevel::AccuracyOptimized,
            ..image.clone()
        };
        Self {
            tab: UITab::Explore,
            explore_state: ExploreTabState {
                rendered_view: ImgSpec::invalid().view(),
                style: ImgStyle::opt_default(),
                scaling: 0.5,
                location_presets: PresetLibrary::new(
                    base_dirs.iter().map(|p| p.join("locations")).collect(),
                ),
                opt_level: OptLevel::PerformanceOptimized,
            },
            style_state: StyleTabState {
                rendered_spec: ImgSpec::invalid(),
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
            history: EditHistory::new(root_spec.clone(), Duration::from_millis(300)),
            root_spec,
            show_camera: false,
            setting_probe: false,
            swap: None,
            show_settings: false,
            preset_save_active: None,
            preset_name: "New Preset".into(),
            preset_group: "New Group".into(),
            new_group_active: false,
            share_active: false,
            docs_state: docs::DocsState::default(),
            status: Status::default(),
            command_channel,
            active_file,
            confirm: None,
            show_splash: context.config().show_splash_on_start && default_image,
        }
    }

    /// Generate the UI and handle any events. This function will do some blocking
    /// to access shared data
    pub fn generate_ui(
        &mut self,
        ui: &mut egui::Ui,
        context: &mut crate::Context,
        cancel: impl Fn(RendererId),
    ) {
        let style = ui.style().clone();
        let start_show_splash = self.show_splash;
        let window_rect = ui.available_rect_before_wrap();

        egui::Panel::right("settings_panel")
            .frame(Frame::new().fill(style.visuals.window_fill))
            .show(ui, |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);

                self.menu(context, ui);
                ScrollArea::vertical().show(ui, |ui| {
                    tui(ui, ui.id().with("side"))
                        .reserve_available_width()
                        .style(Style::col())
                        .show(|tui| match self.tab {
                            UITab::Explore => self.explore_tab(context, tui),
                            UITab::Style => self.style_tab(context, tui),
                            UITab::Render(_) => self.render_tab(context, &cancel, tui),
                        });
                });
            });

        let res = egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(style.visuals.window_fill))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let origin_rect = ui.available_rect_before_wrap();
                let hover_pt = self.viewport(ui);
                self.render_widgets(ui, context);
                self.footer(ui, context, hover_pt);
                crate::app_log::logs_ui(ui, origin_rect);
            });

        let size = Vec2::new(600.0, 360.0);
        let mut show_splash = self.show_splash;
        let splash_res = egui::Window::new("Splash")
            .open(&mut show_splash)
            .title_bar(false)
            .collapsible(false)
            .fixed_size(size)
            .pivot(egui::Align2::CENTER_CENTER)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .frame(Frame::new().shadow(egui::Shadow {
                offset: [-10, 10],
                blur: 100,
                spread: 20,
                color: egui::Color32::BLACK,
            }))
            .show(ui, |ui| {
                ui.set_style(style.clone());
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);

                self.splash_screen(context, ui, size);
            });
        self.show_splash = show_splash;
        if start_show_splash
            && let Some(res) = splash_res
            && res.response.clicked_elsewhere()
        {
            self.show_splash = false;
        }

        let mut show_settings = self.show_settings;
        egui::Window::new("Settings")
            .open(&mut show_settings)
            .default_pos(res.response.rect.center())
            .pivot(egui::Align2::CENTER_BOTTOM)
            .show(ui, |ui| {
                ui.set_style(style.clone());
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);

                let previous_config = context.config().clone();
                tui(ui, ui.id().with("settings"))
                    .reserve_available_width()
                    .style(Style::col())
                    .show(|tui| {
                        section(tui, "Configuration", true, |tui| {
                            context.config_mut().render_edit_ui(tui);
                        });
                        section(tui, "Theme", true, |tui| {
                            context.theme_mut().render_edit_ui(tui);
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

        let mut open = self.preset_save_active.is_some();
        egui::Window::new("Save Preset")
            .open(&mut open)
            .default_size(Vec2::splat(300.0))
            .default_pos(res.response.rect.center())
            .pivot(egui::Align2::CENTER_BOTTOM)
            .show(ui, |ui| {
                ui.set_style(style.clone());
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                self.save_preset_window(context, ui);
            });
        if !open {
            self.preset_save_active = None;
        }

        egui::Window::new("New Group")
            .open(&mut self.new_group_active)
            .default_pos(res.response.rect.center())
            .pivot(egui::Align2::CENTER_TOP)
            .show(ui, |ui| {
                ui.set_style(style.clone());
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

        egui::Window::new("Share Text")
            .open(&mut self.share_active)
            .default_pos(res.response.rect.center())
            .pivot(egui::Align2::CENTER_BOTTOM)
            .show(ui, |ui| {
                ui.set_style(style.clone());
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);

                let text = self
                    .root_spec
                    .stringify_compact()
                    .map(|bytes| create_share_string(&bytes))
                    .unwrap_or("Failed to create share string".into());

                let gap = ui.spacing().item_spacing.y * 3.0;
                tui(ui, "share")
                    .reserve_available_width()
                    .style(Style::col().center().gap(gap))
                    .show(|tui| {
                        tui.label("Share or save this text, then paste it anywhere in the viewport to load the settings");
                        tui.ui_add_manual(|ui| ui.code(text), |res, _ui| res);
                        if tui.ui_add(Button::new("Close")).clicked() {
                            tui.egui_ui().close_kind(egui::UiKind::Window);
                        }
                    });
            });

        let mut open = self.docs_state.active;
        egui::Window::new("Documentation")
            .open(&mut open)
            .default_size(window_rect.size() / 2.0)
            .default_pos(window_rect.center() - window_rect.size() / 4.0)
            .pivot(egui::Align2::LEFT_TOP)
            .show(ui, |ui| {
                ui.set_style(style.clone());
                self.docs_state.render_docs(ui);
            });
        self.docs_state.active = open;

        if ui.input(|i| i.viewport().close_requested()) {
            // check if we need to save
            if context.config().show_save_prompt
                && self
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
                ui.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
        }

        self.show_confirm_dialog(ui, context, res);
        self.handle_shortcuts(context, ui, cancel);

        if !ui.will_discard() {
            self.swap = None;
        }
        self.history.update(&self.root_spec);
    }

    fn show_confirm_dialog(
        &mut self,
        ui: &egui::Ui,
        context: &mut crate::config::Context,
        res: egui::InnerResponse<()>,
    ) {
        if let Some(mut confirm) = self.confirm.take() {
            let style = ui.style().clone();
            egui::Window::new(&confirm.0)
                .collapsible(false)
                .default_pos(res.response.rect.center())
                .pivot(egui::Align2::CENTER_TOP)
                .show(ui, |ui| {
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
                            confirm.1.remove(selection).1.0(self, context, ui);
                        } else {
                            self.confirm = Some(confirm);
                        }
                    });
                });
        }
    }

    fn menu(&mut self, context: &mut crate::config::Context, ui: &mut egui::Ui) {
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
                    if ui
                        .add_enabled(
                            self.history.has_history(),
                            Button::new("Undo")
                                .shortcut_text(ui.ctx().format_shortcut(&shortcuts::UNDO)),
                        )
                        .clicked()
                    {
                        self.undo();
                    }
                    if ui
                        .add_enabled(
                            self.history.has_future(),
                            Button::new("Redo")
                                .shortcut_text(ui.ctx().format_shortcut(&shortcuts::REDO)),
                        )
                        .clicked()
                    {
                        self.redo();
                    }
                    ui.separator();
                    if ui.add(Button::new("Share…")).clicked() {
                        self.share_active = true;
                    }
                    if ui.add(Button::new("Show Splash")).clicked() {
                        self.show_splash = true;
                    }
                    if ui.add(Button::new("Show Docs")).clicked() {
                        self.docs_state.active = true;
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
                        ui.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            };
            ui.selectable_value(
                &mut self.tab,
                UITab::Explore,
                format!("{} Explore", icons::ICON_EXPLORE.codepoint),
            );
            ui.selectable_value(
                &mut self.tab,
                UITab::Style,
                format!("{} Style", icons::ICON_STYLE.codepoint),
            );
            let mut is_render = matches!(self.tab, UITab::Render(_));
            ui.selectable_value(
                &mut is_render,
                true,
                format!("{} Render", icons::ICON_IMAGE.codepoint),
            );
            if is_render && !matches!(self.tab, UITab::Render(_)) {
                self.tab = UITab::Render(false);
            }
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
            let margin = ui.spacing().item_spacing.x / 2.0;
            Frame::new()
                .inner_margin(margin)
                .fill(ui.style().visuals.selection.bg_fill)
                .show(ui, |ui| {
                    ui.scope_builder(
                        UiBuilder::new()
                            .max_rect(egui::Rect::from_min_size(
                                ui.max_rect().left_top(),
                                Vec2::splat(ui.max_rect().height()),
                            ))
                            .layout(egui::Layout::centered_and_justified(
                                egui::Direction::TopDown,
                            )),
                        |ui| {
                            ui.add(
                                egui::Image::new(egui::include_image!(
                                    "../../assets/logo/icon_mono.svg"
                                ))
                                .tint(ui.style().visuals.window_fill),
                            );
                        },
                    );
                });
            let progress_start = ui.spacing().item_spacing.x + ui.max_rect().height();
            ui.scope_builder(
                UiBuilder::new().max_rect(
                    ui.max_rect()
                        .with_max_x(100.0 + progress_start)
                        .with_min_x(progress_start),
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
                    let text = if self.tab == UITab::Explore {
                        if context.config().show_debug_options {
                            // the format used in tests
                            format!("(\"{}\", \"{}\")", hover_pt.x, hover_pt.y)
                        } else {
                            format!("{} + {}i", hover_pt.x, hover_pt.y)
                        }
                    } else {
                        match self.root_spec.stringify_compact() {
                            Ok(bytes) => create_share_string(&bytes),
                            Err(err) => {
                                tracing::error!("Could not copy image: {err}");
                                return;
                            }
                        }
                    };

                    ui.ctx().copy_text(text);
                    if self.tab == UITab::Explore {
                        self.status.message = "Copied location to clipboard".into();
                    } else {
                        self.status.message = "Copied share string to clipboard".into();
                    }
                    self.status.progress = None;
                }

                if let Some(paste_text) = ui.input(|inp| {
                    inp.events.iter().find_map(|ev| match ev {
                        egui::Event::Paste(t) => Some(t.clone()),
                        _ => None,
                    })
                }) {
                    if let Some(img) = parse_share_string(&paste_text)
                        .ok()
                        .and_then(|b| ImgSpec::from_bytes(&b).ok())
                    {
                        self.root_spec = img;
                        tracing::info!("Loaded image settings from clipboard");
                        self.status.message = "Loaded image settings from clipboard".into();
                        self.status.progress = None;
                    } else if let Some((Ok(x), Ok(y))) = paste_text
                        .strip_suffix('i')
                        .and_then(|t| t.split_once(" + "))
                        .map(|(x_str, y_str)| {
                            let prec = get_precision(self.root_spec.location.zoom);
                            (
                                Float::parse(x_str).map(|val| val.complete(prec)),
                                Float::parse(y_str).map(|val| val.complete(prec)),
                            )
                        })
                    {
                        self.root_spec.location.center = ComplexPoint::new(x, y)
                    } else {
                        tracing::warn!("Clipboard content is not valid");
                    }
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
                                        swap: self
                                            .swap
                                            .is_some_and(|id| id == RendererId::Thumbnail),
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
        match self.tab {
            UITab::Explore => self.image_to_render(RendererId::Explore),
            UITab::Style => self.image_to_render(RendererId::Style),
            UITab::Render(false) => self.style_state.rendered_spec.clone(),
            UITab::Render(true) => {
                let mut active_image = self.root_spec.clone();
                active_image.set_view(self.current_view.clone());
                active_image
            }
        }
    }

    pub fn image_to_render(&self, id: RendererId) -> ImgSpec {
        let mut active_image = self.root_spec.clone();
        match id {
            RendererId::Explore => {
                active_image.style = self.explore_state.style.clone();
                active_image.width = self.current_view.width;
                active_image.height = self.current_view.height;
                active_image.location.zoom -=
                    self.current_view.zoom_offset_from(&self.root_spec.view()) + 0.1;
                active_image.scale(self.explore_state.scaling);
                active_image.optimization_level = self.explore_state.opt_level;
                active_image
            }
            RendererId::Style => {
                active_image.width = self.current_view.width;
                active_image.height = self.current_view.height;
                active_image.location.zoom -=
                    self.current_view.zoom_offset_from(&self.root_spec.view()) + 0.1;
                active_image.scale(self.style_state.scaling);
                active_image.optimization_level = OptLevel::CacheOptimized;
                active_image
            }
            RendererId::Render => {
                let mut image = self.root_spec.clone();
                if self.render_state.exr_mode {
                    image.optimization_level = OptLevel::CacheOptimized;
                }
                image
            }
            // Intentionally not implemented since this requires knowing
            // the configured thumbnail size
            RendererId::Thumbnail => unimplemented!(),
        }
    }

    /// Returns whether the current tab has a viewport that needs
    /// automatic updates when the image settings change.
    pub fn needs_viewport_refresh(&self) -> bool {
        match self.tab {
            UITab::Explore | UITab::Style => true,
            UITab::Render(false) => {
                // render tab preview only needs to change if the camera changed to somewhere
                // outside of the style viewport or the style changed
                let current_img = self.image();
                let new_img = self.image_to_render(RendererId::Style);

                if current_img.style != new_img.style {
                    return true;
                }

                let offset = current_img
                    .view()
                    .complex_to_px_delta(&new_img.location.center);
                let zoom_diff = current_img.location.zoom - new_img.location.zoom;
                let valid_range = -6.0f32..=1.0;
                let buffer = 2.0f32.powf(zoom_diff.clamp(-6.0, 1.0)) * 100.0;

                if offset.x.abs() > current_img.width as f32 * 0.5 + buffer
                    || offset.y.abs() > current_img.height as f32 * 0.5 + buffer
                    || !valid_range.contains(&zoom_diff)
                {
                    return true;
                }

                false
            }
            UITab::Render(true) => false,
        }
    }

    /// Sends a render command for the current tab to the worker thread
    pub fn send_render(&self) -> color_eyre::Result<()> {
        let renderer_id = match self.tab {
            UITab::Explore => RendererId::Explore,
            UITab::Style => RendererId::Style,
            UITab::Render(false) => RendererId::Style,
            UITab::Render(true) => RendererId::Render,
        };
        Ok(self.command_channel.send(ImageGenCommand::Render(
            renderer_id,
            Box::new(self.image_to_render(renderer_id)),
        ))?)
    }

    pub fn update_rendered_view(&mut self, id: RendererId, spec: ImgSpec) {
        match id {
            RendererId::Explore => {
                self.explore_state.rendered_view = spec.view();
            }
            RendererId::Style => {
                self.style_state.rendered_spec = spec;
            }
            RendererId::Render => {
                self.render_state.rendered_view = spec.view();
                let mut view = spec.view();
                view.width = self.current_view.width;
                view.height = self.current_view.height;
                self.current_view = view;
                self.current_view.zoom_to_fit(&spec.view());
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

    pub fn tab(&self) -> UITab {
        self.tab
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
    fn render_edit_ui(&mut self, tui: &mut egui_taffy::Tui) {
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
