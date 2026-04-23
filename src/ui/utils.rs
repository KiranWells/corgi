use std::mem::discriminant;

use corgi_lib::types::{ComplexPoint, FractalKind, Gradient, LayerKind, LightingKind};
use eframe::egui::{self, Color32, RichText, Sense, WidgetText};
use egui_taffy::{Tui, TuiBuilderLogic, TuiWidget};
use rug::Float;
use taffy::prelude::*;

use super::coloring::{OrbitType, StripeType};

pub trait StyleExt {
    fn row() -> Self;
    fn col() -> Self;
    fn grow() -> Self;
    fn pad(self, padding: f32) -> Self;
    fn top(self, padding: f32) -> Self;
    fn side(self, padding: f32) -> Self;
    fn pad2(self, top_bottom: f32, left_right: f32) -> Self;
    fn gap(self, gap: f32) -> Self;
    fn center(self) -> Self;
}

impl StyleExt for Style {
    fn row() -> Self {
        Self {
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            size: Size {
                width: percent(1.0),
                height: auto(),
            },
            justify_content: Some(AlignContent::SpaceBetween),
            align_items: Some(AlignItems::Center),
            ..Default::default()
        }
    }
    fn col() -> Self {
        Self {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            size: Size {
                width: percent(1.0),
                height: auto(),
            },
            ..Default::default()
        }
    }

    fn grow() -> Self {
        Self {
            flex_grow: 1.0,
            ..Default::default()
        }
    }

    fn pad(self, padding: f32) -> Self {
        Self {
            padding: Rect::length(padding),
            ..self
        }
    }

    fn pad2(self, top_bottom: f32, left_right: f32) -> Self {
        Self {
            padding: Rect {
                left: length(left_right),
                right: length(left_right),
                top: length(top_bottom),
                bottom: length(top_bottom),
            },
            ..self
        }
    }

    fn gap(self, gap: f32) -> Self {
        Self {
            gap: length(gap),
            ..self
        }
    }
    fn center(self) -> Self {
        Self {
            display: Display::Flex,
            align_items: Some(AlignItems::Center),
            justify_content: Some(JustifyContent::Center),
            ..self
        }
    }

    fn top(self, padding: f32) -> Self {
        Self {
            padding: Rect {
                left: self.padding.left,
                right: self.padding.right,
                top: length(padding),
                bottom: self.padding.bottom,
            },
            ..self
        }
    }

    fn side(self, padding: f32) -> Self {
        Self {
            padding: Rect {
                left: length(padding),
                right: length(padding),
                top: self.padding.top,
                bottom: self.padding.bottom,
            },
            ..self
        }
    }
}

/// Utility trait for getting UI labels for enum variants
pub trait ToLabel {
    fn label(&self) -> &'static str;
}

impl ToLabel for FractalKind {
    fn label(&self) -> &'static str {
        match &self {
            FractalKind::Mandelbrot => "Mandelbrot",
            FractalKind::Julia(_) => "Julia",
        }
    }
}

impl ToLabel for Gradient {
    fn label(&self) -> &'static str {
        let flat = discriminant(&Gradient::Flat(Default::default()));
        let procedural = discriminant(&Gradient::Procedural(Default::default()));
        let manual = discriminant(&Gradient::Manual(Default::default()));
        let hue = discriminant(&Gradient::Hsv(0.0, 0.0));
        let oklch = discriminant(&Gradient::Oklch(0.0, 0.0));
        match discriminant(self) {
            x if x == flat => "Flat",
            x if x == procedural => "Procedural",
            x if x == manual => "Manual",
            x if x == hue => "Hue",
            x if x == oklch => "OkLCh",
            _ => unreachable!(),
        }
    }
}

impl ToLabel for LightingKind {
    fn label(&self) -> &'static str {
        match self {
            LightingKind::Flat => "Flat",
            LightingKind::Gradient => "Gradient",
            LightingKind::RepeatingGradient => "Repeating Gradient",
            LightingKind::Shaded => "Shaded",
        }
    }
}

impl ToLabel for OrbitType {
    fn label(&self) -> &'static str {
        match self.0 {
            1 => "Center",
            2 => "Circle",
            3 => "Axes",
            4 => "Box",
            _ => unreachable!(),
        }
    }
}

impl ToLabel for StripeType {
    fn label(&self) -> &'static str {
        match self.0 {
            1 => "Angle",
            2 => "Real",
            3 => "Imaginary",
            _ => unreachable!(),
        }
    }
}

impl ToLabel for LayerKind {
    fn label(&self) -> &'static str {
        match self {
            LayerKind::Step => "Step",
            LayerKind::SmoothStep => "Smooth Step",
            LayerKind::Distance => "Distance",
            LayerKind::OrbitTrap => "Orbit Trap",
            LayerKind::Stripe => "Stripe Average",
        }
    }
}

/// Utility trait for getting help text from enum types
pub trait ToHelpText {
    fn help_text(&self) -> &'static str;
}

impl<T> ToHelpText for T
where
    T: documented::DocumentedVariants,
{
    fn help_text(&self) -> &'static str {
        self.get_variant_docs()
    }
}

impl ToHelpText for OrbitType {
    fn help_text(&self) -> &'static str {
        match self.0 {
            1 => "a dot",
            2 => "rounded lines",
            3 => "spikes off of points",
            4 => "angled lines",
            _ => unreachable!(),
        }
    }
}

impl ToHelpText for StripeType {
    fn help_text(&self) -> &'static str {
        match self.0 {
            1 => "thinner lines from each point",
            2 => "rounded effect in the vertical direction",
            3 => "rounded effect in the horizontal direction",
            _ => unreachable!(),
        }
    }
}

/// Adds a CollapsingHeader in a Tui context
pub fn collapsible(tui: &mut egui_taffy::Tui, summary: &str, add_contents: impl FnOnce(&mut Tui)) {
    tui.ui_add_manual(
        |ui| {
            let cr = egui::CollapsingHeader::new(summary)
                .default_open(false)
                .show(ui, |ui| {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                    let gap = ui.style().spacing.item_spacing.y * 2.0;
                    egui_taffy::tui(ui, ui.id().with("ext"))
                        .reserve_available_width()
                        .style(Style::col().gap(gap))
                        .show(add_contents)
                });
            let res = cr.header_response.clone();
            if let Some(br) = cr.body_response {
                res.union(br)
            } else {
                res
            }
        },
        |res, _ui| res,
    );
}

/// Adds a large header with drawn decoration
pub fn fancy_header(ui: &mut egui::Ui, text: impl Into<WidgetText>) -> egui::Response {
    let item_spacing = ui.spacing().item_spacing;
    let text = text.into();
    let galley = text.clone().into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        ui.available_width(),
        egui::TextStyle::Heading,
    );
    ui.horizontal(|ui| {
        let available = (ui.available_width() - galley.size().x - item_spacing.x * 2.0)
            .max(0.0)
            .floor();
        let (rect, _res) = ui.allocate_at_least(
            egui::Vec2::new(available / 2.0, galley.size().y),
            Sense::hover(),
        );
        let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
        let painter = ui.painter();
        painter.hline(
            (rect.left() + item_spacing.x * 3.0)..=(rect.right() - item_spacing.x),
            rect.center().y,
            stroke,
        );
        ui.label(text);
        let (rect, _res) = ui.allocate_at_least(
            egui::Vec2::new(available / 2.0, galley.size().y),
            Sense::hover(),
        );
        let painter = ui.painter();
        painter.hline(
            (rect.left() + item_spacing.x)..=(rect.right() - item_spacing.x * 3.0),
            rect.center().y,
            stroke,
        );
    })
    .response
}

/// Adds a fancy header in Tui context
pub fn fancy_header_tui(tui: &mut Tui, text: impl Into<WidgetText>) {
    tui.ui_add_manual(|ui| fancy_header(ui, text), |res, _ui| res);
}

/// Adds a custom collapsible element representing a UI section
pub fn section(tui: &mut Tui, title: &str, expand: bool, add_contents: impl FnOnce(&mut Tui)) {
    tui.ui_add_manual(
        |ui| {
            let item_spacing = ui.spacing().item_spacing;
            let id = ui.make_persistent_id(title);
            let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
                ui.ctx(),
                id,
                expand,
            );
            let (_rect, pre_res) =
                ui.allocate_at_least(egui::Vec2::new(0.0, item_spacing.y), Sense::hover());
            let header_res = fancy_header(ui, RichText::new(title).heading());
            ui.style_mut().spacing.item_spacing.y = 0.0;
            let (_rect, res) = ui.allocate_at_least(egui::Vec2::new(0.0, 4.0), Sense::hover());
            ui.style_mut().spacing.item_spacing.y = item_spacing.y;
            let parent_rect = header_res.rect;
            let target_rect = egui::Rect::from_center_size(
                parent_rect.center_bottom() + egui::Vec2::new(0.0, item_spacing.y),
                egui::Vec2::new(10.0, 4.0),
            );
            let target_rect = target_rect
                .scale_from_center2(egui::Vec2::new(1.0, 1.0 - state.openness(ui.ctx()) * 2.0));
            ui.painter().add(egui::Shape::Path(egui::epaint::PathShape {
                points: vec![
                    target_rect.left_top(),
                    target_rect.center_bottom(),
                    target_rect.right_top(),
                ],
                closed: false,
                fill: Color32::TRANSPARENT,
                stroke: egui::epaint::PathStroke {
                    width: 2.0,
                    color: egui::epaint::ColorMode::Solid(
                        ui.visuals().widgets.noninteractive.fg_stroke.color,
                    ),
                    kind: egui::StrokeKind::Middle,
                },
            }));
            let header_res = header_res.union(res).union(pre_res);
            let clickable_res = ui.interact(header_res.rect, id, Sense::click());
            if clickable_res.clicked() {
                state.toggle(ui);
            }
            let res = state.show_body_unindented(ui, |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                let gap = length(ui.style().spacing.item_spacing.y * 2.0);
                let indent = ui.spacing().indent;
                egui_taffy::tui(ui, ui.id().with("ext"))
                    .reserve_available_width()
                    .style(taffy::Style {
                        flex_direction: taffy::FlexDirection::Column,
                        size: percent(1.0),
                        flex_grow: 1.0,
                        gap,
                        padding: Rect {
                            left: length(indent),
                            right: length(indent),
                            top: length(item_spacing.y),
                            bottom: length(0.0),
                        },
                        ..Default::default()
                    })
                    .show(add_contents)
            });
            if let Some(res) = res {
                header_res.union(res.response)
            } else {
                header_res
            }
        },
        |res, _ui| res,
    );
}

/// Creates a labeled input from the given widget
pub fn input_with_label(
    tui: &mut egui_taffy::Tui,
    label: &str,
    help_text: Option<&str>,
    widget: impl TuiWidget,
) {
    ui_with_label(tui, label, help_text, |tui| {
        tui.ui_add(widget);
    });
}

/// Adds a selectable ComboBox in a Tui context
pub fn selection<T: PartialEq + ToLabel + ToHelpText>(
    tui: &mut egui_taffy::Tui,
    label: &str,
    help_text: Option<&str>,
    current_value: &mut T,
    options: Vec<T>,
) {
    tui.ui_add_manual(
        |ui| {
            let res = egui::ComboBox::from_id_salt(label)
                .selected_text(current_value.label())
                .show_ui(ui, |ui| {
                    for selected_value in options {
                        let text = selected_value.label();
                        let help = selected_value.help_text();
                        let res = ui.selectable_value(current_value, selected_value, text);
                        if !help.is_empty() {
                            res.on_hover_text(help);
                        }
                    }
                })
                .response;
            if let Some(help_text) = help_text
                && !help_text.is_empty()
            {
                res.on_hover_text(help_text)
            } else {
                res
            }
        },
        |res, _ui| res,
    );
}

/// Adds a selectable ComboBox in a Tui context
pub fn raw_selection(
    tui: &mut egui_taffy::Tui,
    label: &str,
    help_text: Option<&str>,
    current_value: &mut String,
    options: Vec<String>,
) {
    tui.ui_add_manual(
        |ui| {
            let res = egui::ComboBox::from_id_salt(label)
                .selected_text(&*current_value)
                .show_ui(ui, |ui| {
                    for selected_value in options {
                        ui.selectable_value(current_value, selected_value.clone(), &selected_value);
                    }
                })
                .response;
            if let Some(help_text) = help_text
                && !help_text.is_empty()
            {
                res.on_hover_text(help_text)
            } else {
                res
            }
        },
        |res, _ui| res,
    );
}

/// Adds a selection with a label in a Tui context
pub fn selection_with_label<T: PartialEq + ToLabel + ToHelpText>(
    tui: &mut egui_taffy::Tui,
    label: &str,
    help_text: Option<&str>,
    current_value: &mut T,
    options: Vec<T>,
) {
    ui_with_label(tui, label, help_text, |tui| {
        selection(
            tui,
            label,
            Some(current_value.help_text()),
            current_value,
            options,
        );
    });
}

/// Adds a vertical ScrollArea in a Tui context
pub fn scroll(tui: &mut egui_taffy::Tui, add_contents: impl FnOnce(&mut Tui), name: &str) {
    tui.ui_add_manual(
        |ui| {
            ui.scope(|ui| {
                egui::ScrollArea::vertical()
                    .max_height(1000.0)
                    .min_scrolled_height(300.0)
                    .show(ui, |ui| {
                        egui_taffy::tui(ui, ui.id().with(name))
                            .reserve_available_width()
                            .style(Style::col())
                            .show(add_contents)
                    })
            })
            .response
        },
        |res, _ui| res,
    );
}

/// Adds the given UI and a label in a Tui context
pub fn ui_with_label(
    tui: &mut egui_taffy::Tui,
    label: &str,
    help_text: Option<&str>,
    add_contents: impl FnOnce(&mut Tui),
) {
    tui.style(Style::row()).add(|tui| {
        tui.style(Style::default()).add(|tui| {
            let res = tui.label(label);
            if let Some(help_text) = help_text {
                tui.small(egui_material_icons::icons::ICON_QUESTION_MARK)
                    .union(res)
                    .on_hover_text(help_text);
            }
        });

        add_contents(tui);
    });
}

#[derive(Clone, Debug)]
struct FloatPointEditState {
    x_text: String,
    y_text: String,
}

/// Adds an edit UI for an arbitrary precision [`ComplexPoint`]
pub fn point_edit(
    tui: &mut Tui,
    point_name: &str,
    help_text: Option<&str>,
    precision: u32,
    point: &mut ComplexPoint,
) {
    tui.style(Style::col().gap(tui.egui_ui().spacing().item_spacing.y))
        .add(|tui| {
            let id = tui.egui_ui().next_auto_id();
            let mut state =
                tui.egui_ctx()
                    .data_mut(|d| d.get_persisted(id))
                    .unwrap_or(FloatPointEditState {
                        x_text: point.x.to_string_radix(10, None),
                        y_text: point.y.to_string_radix(10, None),
                    });

            tui.style(Style::default()).add(|tui| {
                let res = tui.label(point_name);
                if let Some(help_text) = help_text {
                    tui.small(egui_material_icons::icons::ICON_QUESTION_MARK)
                        .union(res)
                        .on_hover_text(help_text);
                }
            });
            indent_with_line(tui, |tui| {
                tui.style(taffy::Style {
                    size: Size {
                        width: percent(1.0),
                        height: auto(),
                    },
                    display: taffy::Display::Grid,
                    align_items: Some(taffy::AlignItems::Center),
                    justify_items: Some(taffy::AlignItems::Stretch),
                    justify_content: Some(AlignContent::Stretch),
                    grid_template_rows: vec![min_content(); 2],
                    grid_template_columns: vec![auto(), auto()],
                    gap: length(tui.egui_ui().spacing().item_spacing.y),
                    ..Default::default()
                })
                .add(|tui| {
                    for (label, text_reference, value_reference) in [
                        ("real", &mut state.x_text, &mut point.x),
                        ("imaginary", &mut state.y_text, &mut point.y),
                    ] {
                        tui.label(label);
                        let response = tui.ui_add_manual(
                            |ui| {
                                ui.add(
                                    egui::TextEdit::singleline(text_reference)
                                        .desired_width(f32::INFINITY),
                                )
                            },
                            |mut res, _ui| {
                                res.min_size = emath::Vec2::new(0.0, res.min_size.y);
                                res
                            },
                        );
                        if !response.has_focus() {
                            *text_reference = value_reference.to_string_radix(10, None);
                        } else if let Ok(res) = Float::parse(text_reference) {
                            *value_reference = Float::with_val(precision, res);
                        }
                    }
                });
                tui.egui_ctx().data_mut(|d| d.insert_persisted(id, state));
            });
        });
}

/// Adds an intented UI with a line decoration in the indent
pub fn indent_with_line(tui: &mut Tui, add_contents: impl FnOnce(&mut Tui)) {
    tui.style(Style::row()).add(|tui| {
        {
            let size = taffy::Size {
                height: auto(),
                width: length(tui.egui_ui().spacing().indent),
            };
            let tui = tui.mut_style(|style| {
                style.align_self = Some(taffy::AlignItems::Stretch);
                style.min_size = size;
                style.max_size = size;
                style.size = size;
            });

            tui.add_with_background_ui(
                |ui, container| {
                    let inner = container.full_container_without_border_and_padding();
                    ui.scope_builder(
                        egui::UiBuilder::new()
                            .layout(egui::Layout::left_to_right(egui::Align::Center))
                            .max_rect(inner),
                        |ui| {
                            ui.add(
                                egui::Separator::default()
                                    .vertical()
                                    .spacing(ui.spacing().indent),
                            )
                        },
                    )
                    .inner
                },
                |_, _| {},
            );
        }
        let gap = tui.egui_ui().spacing().item_spacing.y;
        tui.style(Style::col().gap(gap)).add(|tui| {
            add_contents(tui);
        });
    });
}

/// Adds a color editing UI in a Tui context for `[f32; 3]`
pub fn color_edit(tui: &mut egui_taffy::Tui, color: &mut [f32; 3]) {
    tui.ui_add_manual(
        |ui| egui::widgets::color_picker::color_edit_button_rgb(ui, color),
        |res, _ui| res,
    );
}

/// Adds a color editing UI in a Tui context for a [`Color32`]
pub fn color32_edit(tui: &mut egui_taffy::Tui, color: &mut Color32) {
    tui.ui_add_manual(
        |ui| {
            egui::widgets::color_picker::color_edit_button_srgba(
                ui,
                color,
                egui::color_picker::Alpha::Opaque,
            )
        },
        |res, _ui| res,
    );
}

/// Adds a color editing UI in a Tui context for a potentially non-color value
pub fn pseudo_color_edit(tui: &mut egui_taffy::Tui, color: &mut [f32; 3]) {
    tui.style(Style::row()).add(|tui| {
        tui.ui_add(
            egui::DragValue::new(&mut color[0])
                .speed(0.003)
                .fixed_decimals(3),
        );
        tui.ui_add(
            egui::DragValue::new(&mut color[1])
                .speed(0.003)
                .fixed_decimals(3),
        );
        tui.ui_add(
            egui::DragValue::new(&mut color[2])
                .speed(0.003)
                .fixed_decimals(3),
        );
        if color.map(|x| (0.0..=1.0).contains(&x)).iter().all(|x| *x) {
            color_edit(tui, color);
        }
    });
}

pub fn custom_colored_collapse<T>(
    tui: &mut egui_taffy::Tui,
    salt: impl std::hash::Hash,
    data: &mut T,
    add_header: impl FnOnce(&mut egui_taffy::Tui, &mut T),
    add_contents: impl FnOnce(&mut egui_taffy::Tui, &mut T),
) {
    let item_spacing = tui.egui_ui().spacing().item_spacing;
    let id = tui.egui_ui().make_persistent_id(salt);
    let mut state =
        egui::collapsing_header::CollapsingState::load_with_default_open(tui.egui_ctx(), id, true);
    let is_open = state.openness(tui.egui_ctx()) > 0.0;
    let radius = tui.egui_ui().visuals().widgets.inactive.corner_radius.nw * 2;
    tui.style(Style::col()).add_with_background_ui(
        |ui, container| {
            ui.painter()
                .rect_filled(container.full_container(), radius, ui.visuals().window_fill);
        },
        |tui, _| {
            tui.style(Style::row().pad(item_spacing.x))
                .add_with_background_ui(
                    |ui, container| {
                        ui.painter().rect_filled(
                            container.full_container(),
                            if is_open {
                                egui::CornerRadius {
                                    nw: radius,
                                    ne: radius,
                                    sw: 0,
                                    se: 0,
                                }
                            } else {
                                egui::CornerRadius::same(radius)
                            },
                            ui.visuals().selection.bg_fill,
                        );
                    },
                    |tui, _| {
                        let text_color = tui.egui_ui().visuals().selection.stroke.color;
                        let text_color_alt = tui.egui_ui().visuals().panel_fill;
                        let widgets = &mut tui.egui_ui_mut().style_mut().visuals.widgets;
                        widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
                        widgets.inactive.fg_stroke.color = text_color;
                        widgets.noninteractive.weak_bg_fill = egui::Color32::TRANSPARENT;
                        widgets.noninteractive.fg_stroke.color = text_color;
                        widgets.active.weak_bg_fill = egui::Color32::TRANSPARENT;
                        widgets.active.fg_stroke.color = text_color;
                        widgets.hovered.weak_bg_fill = egui::Color32::TRANSPARENT;
                        widgets.hovered.fg_stroke.color = text_color_alt;
                        widgets.open.weak_bg_fill = egui::Color32::TRANSPARENT;
                        widgets.open.fg_stroke.color = text_color;
                        tui.egui_style_mut().spacing.button_padding = item_spacing / 2.0;
                        tui.style(Style::default()).ui_add_manual(
                            |ui| {
                                state.show_toggle_button(
                                    ui,
                                    egui::collapsing_header::paint_default_icon,
                                )
                            },
                            |cont, _ui| cont,
                        );
                        add_header(tui, data)
                    },
                );
            tui.ui_add_manual(
                |ui| {
                    if let Some(res) = state.show_body_unindented(ui, |ui| {
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                        egui_taffy::tui(ui, ui.id().with("ext"))
                            .reserve_available_width()
                            .style(Style::col().pad(item_spacing.x).gap(item_spacing.x))
                            .show(|tui| add_contents(tui, data))
                    }) {
                        res.response
                    } else {
                        ui.interact(egui::Rect::ZERO, ui.id(), egui::Sense::empty())
                    }
                },
                |cont, _ui| cont,
            );
        },
    );
}
