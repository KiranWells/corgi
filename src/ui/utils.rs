use std::mem::{Discriminant, discriminant};

use eframe::egui::{self, Color32, RichText, Sense, Separator, TextStyle, WidgetText};
use egui_taffy::{Tui, TuiBuilderLogic, TuiWidget};
use rug::Float;
use taffy::{Overflow, prelude::*};

use corgi::types::{ComplexPoint, FractalKind, Gradient, LayerKind, LightingKind};

use super::coloring::{OrbitType, StripeType};

pub trait TuiExt {
    fn horizontal(&mut self) -> egui_taffy::TuiBuilder<'_>;
    fn vertical(&mut self) -> egui_taffy::TuiBuilder<'_>;
    fn grow(&mut self) -> egui_taffy::TuiBuilder<'_>;
}

impl TuiExt for Tui {
    fn horizontal(&mut self) -> egui_taffy::TuiBuilder<'_> {
        self.style(Style {
            flex_direction: FlexDirection::Row,
            padding: Rect::zero(),
            justify_content: Some(AlignContent::SpaceEvenly),
            align_content: Some(AlignContent::Stretch),
            justify_items: Some(AlignItems::Stretch),
            align_items: Some(AlignItems::Center),
            size: Size {
                width: percent(1.0),
                height: auto(),
            },
            flex_grow: 1.0,
            ..self.current_style().clone()
        })
    }
    fn vertical(&mut self) -> egui_taffy::TuiBuilder<'_> {
        self.style(Style {
            flex_direction: FlexDirection::Column,
            padding: Rect::zero(),
            justify_content: Some(AlignContent::SpaceEvenly),
            align_content: Some(AlignContent::Stretch),
            justify_items: Some(AlignItems::Stretch),
            align_items: Some(AlignItems::Center),
            size: Size {
                width: percent(1.0),
                height: auto(),
            },
            flex_grow: 1.0,
            ..self.current_style().clone()
        })
    }
    fn grow(&mut self) -> egui_taffy::TuiBuilder<'_> {
        self.style(Style {
            flex_grow: 1.0,
            ..self.current_style().clone()
        })
    }
}

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

impl ToLabel for Discriminant<Gradient> {
    fn label(&self) -> &'static str {
        let flat = discriminant(&Gradient::Flat(Default::default()));
        let procedural = discriminant(&Gradient::Procedural(Default::default()));
        let manual = discriminant(&Gradient::Manual(Default::default()));
        let hue = discriminant(&Gradient::Hsv(0.0, 0.0));
        match *self {
            x if x == flat => "Flat",
            x if x == procedural => "Procedural",
            x if x == manual => "Manual",
            x if x == hue => "Hue",
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
            LayerKind::None => unreachable!(),
            LayerKind::Step => "Step",
            LayerKind::SmoothStep => "Smooth Step",
            LayerKind::Distance => "Distance",
            LayerKind::OrbitTrap => "Orbit Trap",
            LayerKind::Stripe => "Stripe Average",
        }
    }
}

pub trait ToHelpText {
    fn help_text(&self) -> &'static str;
}

impl ToHelpText for Discriminant<Gradient> {
    fn help_text(&self) -> &'static str {
        let flat = discriminant(&Gradient::Flat(Default::default()));
        let procedural = discriminant(&Gradient::Procedural(Default::default()));
        let manual = discriminant(&Gradient::Manual(Default::default()));
        let hue = discriminant(&Gradient::Hsv(0.0, 0.0));
        match *self {
            x if x == flat => "A single color",
            x if x == procedural => {
                "Generates a gradient using a procedural equation based on Inigo Quilez's simple color palettes. The result is seamless if the third parameter's values are whole numbers."
            }
            x if x == manual => "A repeating linear gradient with manual colors and gradient stops",
            x if x == hue => "A gradient with rotating hue",
            _ => unreachable!(),
        }
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

impl ToHelpText for LightingKind {
    fn help_text(&self) -> &'static str {
        match self {
            LightingKind::Flat => "Full brightnes over the entire image",
            LightingKind::Gradient => {
                "Layers are added, then used directly as the brightness value."
            }
            LightingKind::RepeatingGradient => {
                "Layers are added, then adjusted to repeat from 0.0 to 1.0 brightness (using cosine)."
            }
            LightingKind::Shaded => "Mimics a 3D shape with lighting by 3 lights.",
        }
    }
}

impl ToHelpText for FractalKind {
    fn help_text(&self) -> &'static str {
        match self {
            FractalKind::Mandelbrot => "",
            FractalKind::Julia(_) => "",
        }
    }
}

impl ToHelpText for LayerKind {
    fn help_text(&self) -> &'static str {
        match self {
            LayerKind::None => unreachable!(),
            LayerKind::Step => {
                "Uses the number of iterations required for the point to escape. Has hard lines between colors."
            }
            LayerKind::SmoothStep => {
                "A version of step without hard lines. Logarithmic instead of linear."
            }
            LayerKind::Distance => {
                "Distance estimation to the edge of the set. Scales depending on the zoom level."
            }
            LayerKind::OrbitTrap => {
                "Draws copies of a given shape in repeated patterns around critical points in the set."
            }
            LayerKind::Stripe => "Draws effects radiating from the edges of the fractal.",
        }
    }
}

pub fn collapsible(tui: &mut egui_taffy::Tui, summary: &str, add_contents: impl FnOnce(&mut Tui)) {
    tui.ui_add_manual(
        |ui| {
            let cr = egui::CollapsingHeader::new(summary)
                .default_open(false)
                .show(ui, |ui| {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                    let gap = length(ui.style().spacing.item_spacing.y * 2.0);
                    egui_taffy::tui(ui, ui.id().with("ext"))
                        .reserve_available_width()
                        .style(taffy::Style {
                            flex_direction: taffy::FlexDirection::Column,
                            size: percent(1.0),
                            flex_grow: 1.0,
                            gap,
                            overflow: taffy::Point {
                                x: Overflow::Hidden,
                                y: Overflow::Scroll,
                            },
                            ..Default::default()
                        })
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

pub fn fancy_header_tui(tui: &mut Tui, text: impl Into<WidgetText>) {
    tui.ui_add_manual(|ui| fancy_header(ui, text), |res, _ui| res);
}

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
            // let header_res = ui.button(title);
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

pub fn ui_with_label(
    tui: &mut egui_taffy::Tui,
    label: &str,
    help_text: Option<&str>,
    add_contents: impl FnOnce(&mut Tui),
) {
    tui.horizontal().add(|tui| {
        let current_style = tui.current_style().clone();
        tui.grow()
            .style(Style {
                gap: length(0.0),
                align_items: Some(AlignItems::Start),
                justify_content: Some(AlignContent::Start),
                ..current_style
            })
            .add(|tui| {
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

pub fn point_edit(
    tui: &mut Tui,
    point_name: &str,
    help_text: Option<&str>,
    precision: u32,
    point: &mut ComplexPoint,
) {
    let current = tui.current_style().clone();
    tui.style(Style {
        gap: length(tui.egui_ui().spacing().item_spacing.y),
        padding: Rect::zero(),
        ..current
    })
    .add(|tui| {
        let id = tui.egui_ui().next_auto_id();
        let mut state =
            tui.egui_ctx()
                .data_mut(|d| d.get_persisted(id))
                .unwrap_or(FloatPointEditState {
                    x_text: point.x.to_string_radix(10, None),
                    y_text: point.y.to_string_radix(10, None),
                });

        tui.style(Style {
            flex_direction: FlexDirection::Row,
            padding: Rect::zero(),
            gap: length(0.0),
            size: auto(),
            ..tui.current_style().clone()
        })
        .add(|tui| {
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
                let text_width = WidgetText::from("imaginary")
                    .into_galley(
                        tui.egui_ui(),
                        None,
                        tui.egui_ui().available_width(),
                        TextStyle::Body,
                    )
                    .size()
                    .x;
                for (label, text_reference, value_reference) in [
                    ("real", &mut state.x_text, &mut point.x),
                    ("imaginary", &mut state.y_text, &mut point.y),
                ] {
                    tui.label(label);
                    let available = tui.egui_ui().available_width();
                    let response =
                        tui.ui_add(egui::TextEdit::singleline(text_reference).desired_width(
                            available
                                - text_width
                                - tui.egui_ui().spacing().item_spacing.y
                                - tui.egui_ui().spacing().indent,
                        ));
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

pub fn indent_with_line(tui: &mut Tui, add_contents: impl FnOnce(&mut Tui)) {
    tui.horizontal().add(|tui| {
        tui.style(taffy::Style {
            size: Size {
                height: percent(1.0),
                width: auto(),
            },
            gap: length(0.0),
            ..tui.current_style().clone()
        })
        .ui_add_manual(
            |ui| ui.add(Separator::default().vertical().spacing(ui.spacing().indent)),
            |res, _ui| res,
        );
        tui.vertical().add(|tui| {
            add_contents(tui);
        });
    });
}
