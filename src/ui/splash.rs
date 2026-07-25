use eframe::egui::{self, Button, Frame};
use egui_material_icons::icons;
use egui_taffy::bg::simple::{TuiBackground, TuiBuilderLogicWithBackground};
use egui_taffy::{TuiBuilderLogic, tui};
use emath::Vec2;
use taffy::prelude::*;

use crate::ui::utils::StyleExt;
use crate::ui::{CorgiUI, resources_directory};

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
            bytes: egui::load::Bytes::Shared(_recolor_svg(include_bytes!($path), $accent_pattern, $bg_pattern, $accent, $bg)),
        }
    };
    ($path:literal, $pattern:literal => $color:expr) => {
        recolor_svg!($path, $pattern => $color, "$000" => egui::Color32::BLACK)
    };
}

impl CorgiUI {
    pub(super) fn splash_screen(
        &mut self,
        context: &mut crate::config::Context,
        ui: &mut egui::Ui,
        size: Vec2,
    ) {
        let splash_path = resources_directory().join("assets/splash.avif");
        let logo_aspect = Vec2::new(244.049, 67.998) / 244.049;
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;

            // Image and overlays
            let image_box = ui
                .image(egui::ImageSource::Uri(
                    format!("file://{}", splash_path.to_string_lossy()).into(),
                ))
                .rect;

            let margin = ui.spacing().indent / 2.0;
            let shadow = eframe::egui::Shadow {
                offset: [0, 0],
                blur: 40,
                spread: 10,
                color: if ui.visuals().dark_mode {
                    egui::Color32::from_black_alpha(80)
                } else {
                    egui::Color32::from_white_alpha(80)
                },
            };
            let text_color = ui.visuals().text_color();
            ui.place(
                emath::Rect::from_min_size(
                    image_box.min,
                    image_box.size().x / 2.0 * logo_aspect + Vec2::splat(margin * 2.0),
                ),
                |ui: &mut egui::Ui| {
                    Frame::new()
                        .shadow(shadow)
                        .inner_margin(margin)
                        .show(ui, |ui| {
                            ui.image(recolor_svg!(
                                "../../assets/logo/full_mono.svg",
                                "#fff" => text_color
                            ));
                        })
                        .response
                },
            );

            let text_size = egui::WidgetText::RichText(std::sync::Arc::new(
                icons::ICON_OPEN_IN_NEW.rich_text().heading(),
            ))
            .into_galley(ui, None, ui.available_width(), egui::TextStyle::Button)
            .size();
            let open_splash_btn_res = ui.place(
                egui::Rect::from_min_max(
                    image_box.max - text_size - Vec2::splat(margin),
                    image_box.max,
                ),
                |ui: &mut egui::Ui| {
                    Frame::new()
                        .shadow(shadow)
                        .show(ui, |ui| {
                            ui.visuals_mut().widgets.inactive.fg_stroke.color = text_color;
                            ui.visuals_mut().widgets.hovered.fg_stroke.color =
                                ui.visuals().weak_text_color();
                            ui.add(
                                Button::new(icons::ICON_OPEN_IN_NEW.rich_text().heading())
                                    .frame(false),
                            )
                            .on_hover_text("Load splash image settings")
                        })
                        .inner
                },
            );
            if open_splash_btn_res.clicked() {
                self.open_file(context, splash_path);
                ui.close_kind(egui::UiKind::Window);
            }

            // Sidebar
            Frame::new()
                .fill(ui.visuals().window_fill)
                .shadow(egui::Shadow {
                    offset: [0, 0],
                    blur: 100,
                    spread: 0,
                    color: egui::Color32::BLACK,
                })
                .show(ui, |ui| {
                    tui(ui, "splash")
                        .reserve_available_space()
                        .style(Style {
                            display: Display::Flex,
                            flex_direction: FlexDirection::Column,
                            size: percent(1.0),
                            align_items: Some(AlignItems::Start),
                            gap: length(margin),
                            padding: length(margin),
                            ..Default::default()
                        })
                        .show(|tui| {
                            tui.heading(format!("{} Recent Files", icons::ICON_FOLDER.codepoint));
                            tui.style(Style {
                                display: Display::Flex,
                                flex_direction: FlexDirection::Column,
                                align_items: Some(AlignItems::Stretch),
                                gap: length(margin / 2.0),
                                padding: length(margin),
                                size: Size {
                                    width: length(size.x - size.y - margin * 2.0),
                                    height: percent(1.0),
                                },
                                overflow: taffy::Point {
                                    x: taffy::Overflow::Clip,
                                    y: taffy::Overflow::Scroll,
                                },
                                ..Default::default()
                            })
                            .bg_add(TuiBackground::new(), |tui| {
                                for path in context.cache().previous_files.iter().rev() {
                                    if tui
                                        .ui_add(
                                            Button::new(
                                                path.file_name()
                                                    .unwrap_or(path.as_os_str())
                                                    .to_string_lossy(),
                                            )
                                            .frame(false)
                                            .right_text(""),
                                        )
                                        .clicked()
                                    {
                                        self.open_file(context, path.clone());
                                        tui.egui_ui().close_kind(egui::UiKind::Window);
                                        break;
                                    }
                                }
                            });
                            tui.separator();

                            if tui.style(Style::row()).ui_add(Button::new("New")).clicked() {
                                tui.egui_ui().close_kind(egui::UiKind::Window);
                            }
                            if tui
                                .style(Style::row())
                                .ui_add(Button::new("Open…"))
                                .clicked()
                                && self.open(context)
                            {
                                tui.egui_ui().close_kind(egui::UiKind::Window);
                            }
                        });
                });
        });
    }
}
