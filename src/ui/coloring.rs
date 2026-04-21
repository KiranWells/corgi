use std::mem::discriminant;

use corgi_lib::image_gen::shader_types::{MAX_GRADIENT_STOPS, MAX_LIGHTS};
use corgi_lib::types::{
    Coloring, Gradient, Layer, LayerKind, Light, LightingKind, Outline, Overlays, next_layer_id,
};
use documented::DocumentedFieldsOpt;
use eframe::egui::collapsing_header::{CollapsingState, paint_default_icon};
use eframe::egui::color_picker::Alpha;
use eframe::egui::widgets::color_picker::color_edit_button_rgba;
use eframe::egui::{self, CornerRadius, Event, RichText, Sense, Stroke};
use egui_material_icons::icons;
use egui_taffy::TuiBuilderLogic;
use taffy::prelude::*;

use super::utils::{fancy_header_tui, indent_with_line, selection_with_label, ui_with_label};
use super::{EditUI, input_with_label};
use crate::ui::utils::{StyleExt, color_edit, pseudo_color_edit, selection};

/// Wrapper type for gradient stops
struct StopKind(u8);
/// Wrapper type for Orbit types
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct OrbitType(pub u8);
/// Wrapper type for Stripe types
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct StripeType(pub u8);

impl EditUI for Coloring {
    fn render_edit_ui(&mut self, ctx: &egui::Context, tui: &mut egui_taffy::Tui) {
        input_with_label(
            tui,
            "Brightness",
            None,
            egui::DragValue::new(&mut self.brightness)
                .speed(0.003)
                .range(0.0..=f32::MAX),
        );
        input_with_label(
            tui,
            "Saturation",
            None,
            egui::DragValue::new(&mut self.saturation)
                .speed(0.003)
                .range(0.0..=f32::MAX),
        );
        fancy_header_tui(
            tui,
            RichText::new("Color").text_style(egui::TextStyle::Name("Subheading".into())),
        );
        self.gradient.render_edit_ui(ctx, tui);
        if discriminant(&self.gradient) != discriminant(&Gradient::Flat(Default::default())) {
            input_with_label(
                tui,
                "Gradient repeat frequency",
                Self::get_field_docs("color_frequency").ok(),
                egui::DragValue::new(&mut self.color_frequency).speed(0.003),
            );
            input_with_label(
                tui,
                "Gradient offset",
                None,
                egui::DragValue::new(&mut self.color_offset)
                    .speed(0.003)
                    .range(0.0..=1.0),
            );
        }
        if discriminant(&self.gradient) != discriminant(&Gradient::Flat(Default::default())) {
            self.color_layers.render_edit_ui(ctx, tui);
        }
        fancy_header_tui(
            tui,
            RichText::new("Lighting").text_style(egui::TextStyle::Name("Subheading".into())),
        );
        self.lighting_kind.render_edit_ui(ctx, tui);
        if self.lighting_kind == LightingKind::Shaded {
            if self.lights.is_empty() {
                self.lights
                    .push(Light::new([1.0, 1.0, 1.0], 1.0, [1.0, 1.0, 1.0]));
            }
            for i in 0..self.lights.len() {
                let mut brk = false;
                tui.style(Style::col()).add(|tui| {
                    tui.style(Style::row()).add(|tui| {
                        tui.small(format!("Light {}", i + 1));
                        if tui
                            .enabled_ui(self.lights.len() > 1)
                            .ui_add(
                                egui::Button::new(icons::ICON_DELETE)
                                    .fill(ecolor::Color32::TRANSPARENT),
                            )
                            .on_hover_text("Delete")
                            .clicked()
                        {
                            self.lights.remove(i);
                            // Cancel rendering the rest of the list, as we just messed up the indexes.
                            // This appears not to cause rendering issues, and simplifies the logic.
                            brk = true;
                        }
                    });
                    if !brk {
                        indent_with_line(tui, |tui| {
                            self.lights[i].render_edit_ui(ctx, tui);
                        });
                    }
                });
                if brk {
                    break;
                }
            }
            if tui
                .style(Style::row())
                .enabled_ui(self.lights.len() < MAX_LIGHTS)
                .ui_add(egui::Button::new(format!("{} Add Light", icons::ICON_ADD)))
                .clicked()
            {
                self.lights
                    .push(Light::new([1.0, 1.0, 1.0], 1.0, [-1.0, -1.0, 1.0]));
            }
        }
        if self.lighting_kind != LightingKind::Flat {
            self.light_layers.render_edit_ui(ctx, tui);
        }
        fancy_header_tui(
            tui,
            RichText::new("Outlines").text_style(egui::TextStyle::Name("Subheading".into())),
        );
        self.overlays.render_edit_ui(ctx, tui);
    }
}

impl EditUI for Gradient {
    fn render_edit_ui(&mut self, ctx: &egui::Context, tui: &mut egui_taffy::Tui) {
        let flat = Gradient::Flat(Default::default());
        let procedural = Gradient::Procedural(Default::default());
        let manual = Gradient::Manual(Default::default());
        let hue = Gradient::Hsv(0.0, 0.0);
        let oklch = Gradient::Oklch(0.0, 0.0);
        let mut tmp = self.clone();
        selection_with_label(
            tui,
            "Coloring mode",
            None,
            &mut tmp,
            vec![
                flat.clone(),
                manual.clone(),
                procedural.clone(),
                hue.clone(),
                oklch.clone(),
            ],
        );
        if discriminant(&tmp) != discriminant(self) {
            *self = match tmp {
                x if x == flat => Gradient::Flat([1.0; 3]),
                x if x == procedural => {
                    Gradient::Procedural([[0.5; 3], [0.5; 3], [1.0; 3], [0.0, 0.1, 0.2]])
                }
                x if x == manual => Gradient::Manual(vec![
                    [0.6, 0.9, 0.8, 0.1 * 0.999 + 2.0],
                    [0.2, 0.2, 0.3, 0.5 * 0.999 + 2.0],
                    [1.0, 1.0, 1.0, 1.0 * 0.999 + 2.0],
                ]),
                x if x == hue => Gradient::Hsv(0.7, 1.0),
                x if x == oklch => Gradient::Oklch(1.0, 0.15),
                _ => unreachable!(),
            };
        }

        indent_with_line(tui, |tui| {
            match self {
                Gradient::Flat(color) => {
                    ui_with_label(tui, "Color", None, |tui| {
                        color_edit(tui, color);
                    });
                }
                Gradient::Procedural(colors) => {
                    pseudo_color_edit(tui, &mut colors[0]);
                    pseudo_color_edit(tui, &mut colors[1]);
                    pseudo_color_edit(tui, &mut colors[2]);
                    pseudo_color_edit(tui, &mut colors[3]);
                }
                Gradient::Manual(colors) => {
                    let mut dragged = false;
                    tui.style(taffy::Style {
                        display: taffy::Display::Grid,
                        grid_template_rows: vec![min_content(); colors.len()],
                        grid_template_columns: vec![min_content(); 7],
                        align_items: Some(AlignItems::Center),
                        justify_content: Some(AlignContent::Center),
                        gap: length(ctx.style().spacing.item_spacing.x),
                        size: percent(1.0),
                        ..Default::default()
                    })
                    .add(|tui| {
                        for i in 0..colors.len() {
                            let mut stop = colors[i][3].fract() / 0.999;
                            let mut stop_kind = StopKind(colors[i][3].floor() as u8);
                            color_edit(tui, colors[i].first_chunk_mut().unwrap());
                            let res = tui.ui_add(
                                egui::DragValue::new(&mut stop)
                                    .speed(0.001)
                                    .range(0.0..=1.0),
                            );
                            dragged = dragged || res.is_pointer_button_down_on() || res.has_focus();
                            stop_kind.render_edit_ui(ctx, tui);
                            colors[i][3] = stop_kind.0 as f32 + stop * 0.999;
                            if tui
                                .enabled_ui(colors.len() < MAX_GRADIENT_STOPS)
                                .ui_add(egui::Button::new(icons::ICON_CONTROL_POINT_DUPLICATE))
                                .on_hover_text("Duplicate")
                                .clicked()
                            {
                                colors.insert(i, colors[i]);
                            }
                            if tui
                                .ui_add(egui::Button::new(icons::ICON_CONTENT_COPY))
                                .on_hover_text("Copy")
                                .clicked()
                            {
                                tui.egui_ctx().copy_text(format!(
                                    "{}, {}, {}",
                                    colors[i][0], colors[i][1], colors[i][2]
                                ));
                            }
                            let paste_response = tui
                                .ui_add(egui::Button::new(icons::ICON_CONTENT_PASTE))
                                .on_hover_text("Paste");
                            if paste_response.clicked() {
                                tui.egui_ctx()
                                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
                                paste_response.request_focus();
                            }
                            if paste_response.has_focus() {
                                let mut pasted = false;
                                tui.egui_ui().input(|r| {
                                    for event in r.events.iter() {
                                        if let Event::Paste(text) = event {
                                            pasted = true;
                                            let splits: Vec<Result<f32, _>> =
                                                text.split(", ").map(str::parse).collect();
                                            if splits.len() == 3 && splits.iter().all(Result::is_ok)
                                            {
                                                for j in 0..3 {
                                                    colors[i][j] = splits[j].clone().unwrap();
                                                }
                                            }
                                        }
                                    }
                                });
                                if pasted {
                                    paste_response.surrender_focus();
                                }
                            }
                            if tui
                                .ui_add(egui::Button::new(icons::ICON_DELETE))
                                .on_hover_text("Delete")
                                .clicked()
                            {
                                colors.remove(i);
                                // Cancel rendering the rest of the list, as we just messed up the indexes.
                                // This appears not to cause rendering issues, and simplifies the logic.
                                break;
                            }
                        }
                    });
                    if tui
                        .style(Style::row())
                        .enabled_ui(colors.len() < MAX_GRADIENT_STOPS)
                        .ui_add(egui::Button::new(format!(
                            "{} Add Color Stop",
                            icons::ICON_ADD
                        )))
                        .clicked()
                    {
                        let ratio = (colors.len() as f32) / (colors.len() as f32 + 1.0);
                        colors.iter_mut().for_each(|x| {
                            x[3] = (x[3].fract() * ratio) + x[3].floor();
                        });
                        colors.push([
                            1.0,
                            1.0,
                            1.0,
                            1.0 * 0.999 + colors[colors.len() - 1][3].floor(),
                        ]);
                    }
                    if !dragged {
                        colors.sort_by(|a, b| {
                            a[3].fract()
                                .partial_cmp(&b[3].fract())
                                .unwrap_or(std::cmp::Ordering::Equal)
                        });
                    }
                }
                Gradient::Hsv(saturation, value) => {
                    input_with_label(
                        tui,
                        "Saturation",
                        None,
                        egui::DragValue::new(saturation)
                            .speed(0.003)
                            .range(0.0..=1.0),
                    );
                    input_with_label(
                        tui,
                        "Value",
                        None,
                        egui::DragValue::new(value).speed(0.003).range(0.0..=1.0),
                    );
                }
                Gradient::Oklch(lightness, chroma) => {
                    input_with_label(
                        tui,
                        "Lightness",
                        None,
                        egui::DragValue::new(lightness)
                            .speed(0.003)
                            .range(0.0..=1.0),
                    );
                    input_with_label(
                        tui,
                        "Chroma",
                        None,
                        egui::DragValue::new(chroma).speed(0.003).range(0.0..=1.0),
                    );
                }
            };
        });
    }
}

impl EditUI for StopKind {
    fn render_edit_ui(&mut self, _ctx: &egui::Context, tui: &mut egui_taffy::Tui) {
        const MAX_STOP_TYPES: u8 = 3;
        let label = match self.0 {
            0 => icons::ICON_STAIRS_2,
            1 => icons::ICON_DIAGONAL_LINE,
            2 => icons::ICON_LINE_CURVE,
            _ => icons::ICON_QUESTION_MARK,
        };
        let help = match self.0 {
            0 => "Constant interpolation",
            1 => "Linear interpolation",
            2 => "Smooth interpolation",
            _ => icons::ICON_QUESTION_MARK,
        };
        if tui
            .ui_add(egui::Button::new(label))
            .on_hover_text(help)
            .clicked()
        {
            self.0 += 1;
            self.0 %= MAX_STOP_TYPES;
        }
    }
}

impl EditUI for Layer {
    fn render_edit_ui(&mut self, _ctx: &egui::Context, tui: &mut egui_taffy::Tui) {
        match self.kind {
            LayerKind::Step => {
                input_with_label(
                    tui,
                    "Strength",
                    Self::get_field_docs("strength").ok(),
                    egui::DragValue::new(&mut self.strength).speed(0.01),
                );
            }
            LayerKind::SmoothStep => {
                input_with_label(
                    tui,
                    "Strength",
                    Self::get_field_docs("strength").ok(),
                    egui::DragValue::new(&mut self.strength).speed(0.01),
                );
            }
            LayerKind::Distance => {
                input_with_label(
                    tui,
                    "Strength",
                    Self::get_field_docs("strength").ok(),
                    egui::DragValue::new(&mut self.strength).speed(0.01),
                );
                input_with_label(
                    tui,
                    "Boost",
                    Some("Expands how far the distance effect spreads"),
                    egui::DragValue::new(&mut self.param).speed(0.01),
                );
            }
            LayerKind::OrbitTrap => {
                let mut index = OrbitType(self.param as u8 + 1);
                if index.0 > 4 {
                    index.0 = 4;
                }
                let mut offset = self.param.fract();
                selection_with_label(
                    tui,
                    "Orbit Shape",
                    Some("The base shape that is used to draw the repeated patterns"),
                    &mut index,
                    vec![1, 2, 3, 4].into_iter().map(OrbitType).collect(),
                );
                input_with_label(
                    tui,
                    "Strength",
                    Self::get_field_docs("strength").ok(),
                    egui::DragValue::new(&mut self.strength).speed(0.01),
                );
                input_with_label(
                    tui,
                    "Offset",
                    Some(
                        "Subtracts this value from this layer. Mostly useful in Lighting to adjust the black level of the layer.",
                    ),
                    egui::DragValue::new(&mut offset)
                        .speed(0.003)
                        .range(0.0..=0.99),
                );

                self.param = index.0 as f32 - 1.0 + offset;
            }
            LayerKind::Stripe => {
                let mut index = StripeType(self.param as u8 + 1);
                if index.0 > 3 {
                    index.0 = 3;
                }
                let mut offset = self.param.fract();
                selection_with_label(
                    tui,
                    "Stripe Variant",
                    Some(
                        "The value used to draw the stripes. Different values produce different effects.",
                    ),
                    &mut index,
                    vec![1, 2, 3].into_iter().map(StripeType).collect(),
                );
                input_with_label(
                    tui,
                    "Strength",
                    Self::get_field_docs("strength").ok(),
                    egui::DragValue::new(&mut self.strength).speed(0.01),
                );
                input_with_label(
                    tui,
                    "Offset",
                    Some(
                        "Subtracts this value from this layer. Mostly useful in Lighting to adjust the black level of the layer.",
                    ),
                    egui::DragValue::new(&mut offset)
                        .speed(0.003)
                        .range(0.0..=0.99),
                );

                self.param = index.0 as f32 - 1.0 + offset;
            }
        }
    }
}

impl EditUI for Overlays {
    fn render_edit_ui(&mut self, _ctx: &egui::Context, tui: &mut egui_taffy::Tui) {
        let gap = tui.egui_ui().spacing().item_spacing.x;
        if let Some(iteration_outline) = self.iteration_outline.as_mut() {
            tui.style(Style::row().gap(gap)).add(|tui| {
                tui.style(Style::grow()).label("Step Outline");
                tui.ui_add_manual(
                    |ui| {
                        color_edit_button_rgba(
                            ui,
                            &mut iteration_outline.color,
                            Alpha::BlendOrAdditive,
                        )
                    },
                    |res, _ui| res,
                );
                tui.ui_add(
                    egui::DragValue::new(&mut iteration_outline.parameter)
                        .speed(0.1)
                        .range(1..=i32::MAX),
                )
                .on_hover_text("Step Distance");
            });
            if iteration_outline.color.a() == 0.0
                || tui
                    .ui_add(egui::Button::new(format!("{} Remove", icons::ICON_REMOVE)))
                    .clicked()
            {
                self.iteration_outline = None;
            }
        } else if tui
            .ui_add(egui::Button::new(format!(
                "{} Add Iteration Outline",
                icons::ICON_ADD
            )))
            .clicked()
        {
            self.iteration_outline = Some(Outline {
                color: egui::Rgba::WHITE,
                parameter: 1,
            })
        }

        if let Some(set_outline) = self.set_outline.as_mut() {
            let mut scale = set_outline.parameter as f32 / 10.0;
            tui.style(Style::row().gap(gap)).add(|tui| {
                tui.style(Style::grow()).label("Set Outline");
                tui.ui_add_manual(
                    |ui| color_edit_button_rgba(ui, &mut set_outline.color, Alpha::BlendOrAdditive),
                    |res, _ui| res,
                );
                tui.ui_add(
                    egui::DragValue::new(&mut scale)
                        .speed(0.03)
                        .range(0.1..=f32::MAX)
                        .max_decimals(1),
                )
                .on_hover_text("Outline thickness");
            });
            set_outline.parameter = (scale * 10.0) as u32;
            if set_outline.color.a() == 0.0
                || tui
                    .ui_add(egui::Button::new(format!("{} Remove", icons::ICON_REMOVE)))
                    .clicked()
            {
                self.set_outline = None;
            }
        } else if tui
            .ui_add(egui::Button::new(format!(
                "{} Add Set Outline",
                icons::ICON_ADD
            )))
            .clicked()
        {
            self.set_outline = Some(Outline {
                color: egui::Rgba::WHITE,
                parameter: 30,
            })
        }
    }
}

impl EditUI for LightingKind {
    fn render_edit_ui(&mut self, _ctx: &egui::Context, tui: &mut egui_taffy::Tui) {
        selection_with_label(
            tui,
            "Lighting Mode",
            None,
            self,
            vec![
                LightingKind::Flat,
                LightingKind::Gradient,
                LightingKind::RepeatingGradient,
                LightingKind::Shaded,
            ],
        );
    }
}

impl EditUI for Vec<Layer> {
    fn render_edit_ui(&mut self, ctx: &egui::Context, tui: &mut egui_taffy::Tui) {
        fn background(ui: &mut egui::Ui, container: &egui_taffy::TaffyContainerUi) {
            let rect = container.full_container();
            let full_rect = rect.expand2(egui::Vec2::new(ui.spacing().indent * 2.0, 0.0));

            ui.interact(rect, ui.id().with("bg"), egui::Sense::click_and_drag());
            ui.painter().rect(
                full_rect,
                0,
                ui.style().visuals.panel_fill,
                Stroke::default(),
                egui::StrokeKind::Inside,
            );
        }
        let item_spacing = tui.egui_ui().spacing().item_spacing;
        tui.style(
            Style::col()
                .pad2(item_spacing.y * 2.0, 0.0)
                .gap(item_spacing.y),
        )
        .add_with_background_ui(background, |tui, _| {
            let valid_ct = self.len();
            let mut add_layer = false;
            tui.style(Style::row()).add(|tui| {
                tui.label(
                    RichText::new("Layers").text_style(egui::TextStyle::Name("Subheading".into())),
                );
                if valid_ct < 8 {
                    add_layer = tui
                        .button(|tui| {
                            tui.label(format!("{} Add Layer", icons::ICON_ADD));
                        })
                        .clicked();
                }
            });
            let mut layer_ct = 0;
            let mut new_layers = vec![];
            let mut swap_first = -1;
            for (i, layer) in self.iter_mut().enumerate() {
                let mut remove = false;
                let mut duplicate = false;
                let id = tui
                    .egui_ui()
                    .make_persistent_id(format!("Layer {}", layer.id));
                let mut state = CollapsingState::load_with_default_open(tui.egui_ctx(), id, true);
                let is_open = state.openness(tui.egui_ctx()) > 0.0;
                let radius = tui.egui_ui().visuals().widgets.inactive.corner_radius.nw * 2;
                tui.style(Style::col()).add_with_background_ui(
                    |ui, container| {
                        ui.painter().rect_filled(
                            container.full_container(),
                            radius,
                            ui.visuals().window_fill,
                        );
                    },
                    |tui, _| {
                        tui.style(Style::row().side(item_spacing.x))
                            .add_with_background_ui(
                                |ui, container| {
                                    ui.painter().rect_filled(
                                        container.full_container(),
                                        if is_open {
                                            CornerRadius {
                                                nw: radius,
                                                ne: radius,
                                                sw: 0,
                                                se: 0,
                                            }
                                        } else {
                                            CornerRadius::same(radius)
                                        },
                                        ui.visuals().selection.bg_fill,
                                    );
                                },
                                |tui, _| {
                                    let text_color = tui.egui_ui().visuals().selection.stroke.color;
                                    let text_color_alt = tui.egui_ui().visuals().panel_fill;
                                    let widgets =
                                        &mut tui.egui_ui_mut().style_mut().visuals.widgets;
                                    widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
                                    widgets.inactive.fg_stroke.color = text_color;
                                    widgets.noninteractive.weak_bg_fill =
                                        egui::Color32::TRANSPARENT;
                                    widgets.noninteractive.fg_stroke.color = text_color;
                                    widgets.active.weak_bg_fill = egui::Color32::TRANSPARENT;
                                    widgets.active.fg_stroke.color = text_color;
                                    widgets.hovered.weak_bg_fill = egui::Color32::TRANSPARENT;
                                    widgets.hovered.fg_stroke.color = text_color_alt;
                                    widgets.open.weak_bg_fill = egui::Color32::TRANSPARENT;
                                    widgets.open.fg_stroke.color = text_color;
                                    tui.ui_add_manual(
                                        |ui| state.show_toggle_button(ui, paint_default_icon),
                                        |cont, _ui| cont,
                                    );
                                    selection(
                                        tui,
                                        &format!("Layer Type {}", layer.id),
                                        Layer::get_field_docs("kind").ok(),
                                        &mut layer.kind,
                                        vec![
                                            LayerKind::Step,
                                            LayerKind::SmoothStep,
                                            LayerKind::Distance,
                                            LayerKind::OrbitTrap,
                                            LayerKind::Stripe,
                                        ],
                                    );
                                    tui.style(Style::grow()).add_empty();
                                    if i > 0
                                        && tui
                                            .button(|tui| tui.label(icons::ICON_ARROW_UPWARD))
                                            .response
                                            .on_hover_text("Move layer up")
                                            .clicked()
                                    {
                                        swap_first = i as i32 - 1;
                                    }
                                    if tui
                                        .enabled_ui(i < valid_ct.saturating_sub(1))
                                        .button(|tui| tui.label(icons::ICON_ARROW_DOWNWARD))
                                        .response
                                        .on_hover_text("Move layer down")
                                        .clicked()
                                    {
                                        swap_first = i as i32;
                                    }
                                    if tui
                                        .button(|tui| tui.label(icons::ICON_RESET_SETTINGS))
                                        .response
                                        .on_hover_text("Reset layer parameters")
                                        .clicked()
                                    {
                                        layer.strength = 1.0;
                                        match layer.kind {
                                            LayerKind::OrbitTrap | LayerKind::Stripe => {
                                                layer.param = layer.param.floor();
                                            }
                                            _ => layer.param = 0.0,
                                        }
                                    }
                                    if valid_ct < 8 {
                                        duplicate = tui
                                            .button(|tui| tui.label(icons::ICON_CONTENT_COPY))
                                            .response
                                            .on_hover_text("Duplicate layer")
                                            .clicked();
                                    }
                                    remove = tui
                                        .button(|tui| tui.label(icons::ICON_DELETE))
                                        .response
                                        .on_hover_text("Delete layer")
                                        .clicked();
                                },
                            );
                        let current = tui.current_style().clone();
                        tui.ui_add_manual(
                            |ui| {
                                if let Some(res) = state.show_body_unindented(ui, |ui| {
                                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                                    let gap = ui.spacing().item_spacing.y * 1.0;
                                    egui_taffy::tui(ui, ui.id().with("ext"))
                                        .reserve_available_width()
                                        .style(taffy::Style {
                                            flex_direction: taffy::FlexDirection::Column,
                                            size: percent(1.0),
                                            gap: length(gap),
                                            padding: Rect {
                                                left: length(gap * 2.0),
                                                right: length(gap),
                                                bottom: length(gap),
                                                top: length(gap),
                                            },
                                            ..current
                                        })
                                        .show(|tui| layer.render_edit_ui(ctx, tui))
                                }) {
                                    res.response
                                } else {
                                    ui.interact(egui::Rect::ZERO, ui.id(), Sense::hover())
                                }
                            },
                            |cont, _ui| cont,
                        );
                    },
                );
                if !remove {
                    new_layers.push(*layer);
                    layer_ct += 1;
                }
                if duplicate {
                    new_layers.push(*layer);
                    new_layers[layer_ct].id = next_layer_id();
                    layer_ct += 1;
                }
            }
            if swap_first != -1 {
                new_layers.swap(swap_first as usize, swap_first as usize + 1);
            }
            *self = new_layers;
            if layer_ct < 8 && add_layer {
                self.push(Layer {
                    id: next_layer_id(),
                    kind: LayerKind::Step,
                    strength: 1.0,
                    param: 0.0,
                });
            }
        });
    }
}

impl EditUI for Light {
    fn render_edit_ui(&mut self, _ctx: &egui::Context, tui: &mut egui_taffy::Tui) {
        tui.style(Style::row()).add(|tui| {
            tui.label("Color");
            color_edit(tui, &mut self.color);
            tui.label("Strength");
            tui.ui_add(egui::DragValue::new(&mut self.strength).speed(0.003));
        });
        tui.style(Style::row()).add(|tui| {
            tui.label("Direction");
            tui.ui_add(egui::DragValue::new(&mut self.direction[0]).speed(0.003));
            tui.ui_add(egui::DragValue::new(&mut self.direction[1]).speed(0.003));
            tui.ui_add(egui::DragValue::new(&mut self.direction[2]).speed(0.003));
        });
        self.normalize()
    }
}
