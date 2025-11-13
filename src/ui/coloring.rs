use std::mem::discriminant;

use corgi::types::{
    Coloring, Gradient, Layer, LayerKind, Light, LightingKind, MAX_GRADIENT_STOPS, Overlays,
    next_layer_id,
};
use eframe::egui::collapsing_header::{CollapsingState, paint_default_icon};
use eframe::egui::{self, CornerRadius, Event, RichText, Sense, Stroke};
use egui_material_icons::icons;
use egui_taffy::TuiBuilderLogic;
use taffy::prelude::*;

use super::utils::{
    TuiExt, fancy_header_tui, indent_with_line, selection_with_label, ui_with_label,
};
use super::{EditUI, input_with_label};
use crate::ui::utils::{color_edit, pseudo_color_edit, selection};

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
                Some(
                    "How often the colors in the gradient repeat. This acts like a global 'strength' value.",
                ),
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
            for (i, light) in self.lights.iter_mut().enumerate() {
                tui.vertical().add(|tui| {
                    tui.small(format!("Light {}", i + 1));
                    indent_with_line(tui, |tui| {
                        light.render_edit_ui(ctx, tui);
                    });
                });
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
        let flat = discriminant(&Gradient::Flat(Default::default()));
        let procedural = discriminant(&Gradient::Procedural(Default::default()));
        let manual = discriminant(&Gradient::Manual(Default::default()));
        let hue = discriminant(&Gradient::Hsv(0.0, 0.0));
        let mut tmp = discriminant(self);
        selection_with_label(
            tui,
            "Coloring mode",
            None,
            &mut tmp,
            vec![flat, manual, procedural, hue],
        );
        if tmp != discriminant(self) {
            *self = match tmp {
                x if x == flat => Gradient::Flat([1.0; 3]),
                x if x == procedural => {
                    Gradient::Procedural([[0.5; 3], [0.5; 3], [1.0; 3], [0.0, 0.1, 0.2]])
                }
                x if x == manual => Gradient::Manual(vec![
                    [0.6, 0.9, 0.8, 0.1],
                    [0.2, 0.2, 0.3, 0.5],
                    [1.0, 1.0, 1.0, 1.0],
                ]),
                x if x == hue => Gradient::Hsv(0.7, 1.0),
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
                        grid_template_columns: vec![min_content(); 6],
                        align_items: Some(AlignItems::Center),
                        justify_content: Some(AlignContent::Center),
                        gap: length(ctx.style().spacing.item_spacing.x),
                        size: percent(1.0),
                        ..Default::default()
                    })
                    .add(|tui| {
                        for i in 0..colors.len() {
                            color_edit(tui, colors[i].first_chunk_mut().unwrap());
                            let res = tui.ui_add(
                                egui::DragValue::new(&mut colors[i][3])
                                    .speed(0.001)
                                    .range(0.0..=1.0),
                            );
                            dragged = dragged || res.is_pointer_button_down_on() || res.has_focus();
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
                        .horizontal()
                        .enabled_ui(colors.len() < MAX_GRADIENT_STOPS)
                        .ui_add(egui::Button::new(format!(
                            "{} Add Color Stop",
                            icons::ICON_ADD
                        )))
                        .clicked()
                    {
                        let ratio = (colors.len() as f32) / (colors.len() as f32 + 1.0);
                        colors.iter_mut().for_each(|x| x[3] *= ratio);
                        colors.push([1.0, 1.0, 1.0, 1.0]);
                    }
                    if !dragged {
                        colors.sort_by(|a, b| {
                            a[3].partial_cmp(&b[3]).unwrap_or(std::cmp::Ordering::Equal)
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
            };
        });
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct OrbitType(pub u8);
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct StripeType(pub u8);

impl EditUI for Layer {
    fn render_edit_ui(&mut self, _ctx: &egui::Context, tui: &mut egui_taffy::Tui) {
        let strength_help_text = "How strongly this layer influences the end value";
        match self.kind {
            LayerKind::None => unreachable!(),
            LayerKind::Step => {
                input_with_label(
                    tui,
                    "Strength",
                    Some(strength_help_text),
                    egui::DragValue::new(&mut self.strength).speed(0.01),
                );
            }
            LayerKind::SmoothStep => {
                input_with_label(
                    tui,
                    "Strength",
                    Some(strength_help_text),
                    egui::DragValue::new(&mut self.strength).speed(0.01),
                );
            }
            LayerKind::Distance => {
                input_with_label(
                    tui,
                    "Strength",
                    Some(strength_help_text),
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
                    Some(strength_help_text),
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
                    Some(strength_help_text),
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
        let mut rgba = egui::Rgba::from_rgba_unmultiplied(
            self.iteration_outline_color[0],
            self.iteration_outline_color[1],
            self.iteration_outline_color[2],
            self.iteration_outline_color[3].fract(),
        );
        let mut steps = self.iteration_outline_color[3] as i32;
        tui.horizontal().add(|tui| {
            tui.grow().label("Step Outline");
            tui.ui_add_manual(
                |ui| {
                    egui::widgets::color_picker::color_edit_button_rgba(
                        ui,
                        &mut rgba,
                        egui::color_picker::Alpha::BlendOrAdditive,
                    )
                },
                |res, _ui| res,
            );
            tui.ui_add(
                egui::DragValue::new(&mut steps)
                    .speed(0.1)
                    .range(1..=i32::MAX),
            );
        });
        self.iteration_outline_color = rgba.to_rgba_unmultiplied();
        if self.iteration_outline_color[3] > 0.999 {
            self.iteration_outline_color[3] = 0.999;
        }
        self.iteration_outline_color[3] += steps as f32;

        let set_col = self.set_outline_color;
        let mut rgba = egui::Rgba::from_rgba_unmultiplied(
            set_col[0],
            set_col[1],
            set_col[2],
            set_col[3].fract(),
        );
        let mut scale = set_col[3].floor() / 10.0;
        tui.horizontal().add(|tui| {
            tui.grow().label("Set Outline");
            tui.ui_add_manual(
                |ui| {
                    egui::widgets::color_picker::color_edit_button_rgba(
                        ui,
                        &mut rgba,
                        egui::color_picker::Alpha::BlendOrAdditive,
                    )
                },
                |res, _ui| res,
            );
            tui.ui_add(
                egui::DragValue::new(&mut scale)
                    .speed(0.03)
                    .range(0.1..=f32::MAX)
                    .max_decimals(1),
            );
        });
        self.set_outline_color = rgba.to_rgba_unmultiplied();
        if self.set_outline_color[3] > 0.999 {
            self.set_outline_color[3] = 0.999;
        }
        self.set_outline_color[3] += scale * 10.0;
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

impl EditUI for [Layer; 8] {
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
        let current_style = tui.current_style().clone();
        let item_spacing = tui.egui_ui().spacing().item_spacing;
        tui.style(Style {
            flex_direction: FlexDirection::Column,
            padding: Rect {
                left: length(0.0),
                right: length(0.0),
                top: length(item_spacing.y * 2.0),
                bottom: length(item_spacing.y * 2.0),
            },
            size: Size {
                width: percent(1.0),
                height: auto(),
            },
            flex_grow: 1.0,
            ..current_style
        })
        .add_with_background_ui(background, |tui, _| {
            let valid_ct = self.iter().filter(|l| l.kind != LayerKind::None).count();
            let mut add_layer = false;
            tui.horizontal().add(|tui| {
                tui.style(taffy::Style {
                    flex_grow: 1.0,
                    ..Default::default()
                })
                .label(
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
            let mut new_layers = [Layer::default(); 8];
            let mut swap_first = -1;
            for (i, layer) in self.iter_mut().enumerate() {
                if layer.kind == LayerKind::None {
                    break;
                }
                let mut remove = false;
                let mut duplicate = false;
                let id = tui
                    .egui_ui()
                    .make_persistent_id(format!("Layer {}", layer.id));
                let mut state = CollapsingState::load_with_default_open(tui.egui_ctx(), id, true);
                let is_open = state.openness(tui.egui_ctx()) > 0.0;
                let radius = tui.egui_ui().visuals().widgets.inactive.corner_radius.nw * 2;
                tui.style(Style {
                    flex_direction: FlexDirection::Column,
                    padding: Rect::zero(),
                    gap: if is_open {
                        length(item_spacing.y * 2.0)
                    } else {
                        length(0.0)
                    },
                    ..tui.current_style().clone()
                })
                .add_with_background_ui(
                    |ui, container| {
                        ui.painter().rect_filled(
                            container.full_container(),
                            radius,
                            ui.visuals().window_fill,
                        );
                    },
                    |tui, _| {
                        tui.style(Style {
                            flex_direction: FlexDirection::Row,
                            padding: Rect {
                                left: length(item_spacing.x),
                                right: length(item_spacing.x),
                                top: length(0.0),
                                bottom: length(0.0),
                            },
                            align_items: Some(AlignItems::Center),
                            size: Size {
                                width: percent(1.0),
                                height: auto(),
                            },
                            gap: length(0.0),
                            flex_grow: 0.0,
                            ..tui.current_style().clone()
                        })
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
                                tui.ui_add_manual(
                                    |ui| state.show_toggle_button(ui, paint_default_icon),
                                    |cont, _ui| cont,
                                );
                                selection(
                                    tui,
                                    &format!("Layer Type {}", layer.id),
                                    Some("The type of the layer"),
                                    &mut layer.kind,
                                    vec![
                                        LayerKind::Step,
                                        LayerKind::SmoothStep,
                                        LayerKind::Distance,
                                        LayerKind::OrbitTrap,
                                        LayerKind::Stripe,
                                    ],
                                );
                                tui.grow().add_empty();
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
                        tui.style(Style {
                            size: percent(1.0),
                            ..current.clone()
                        })
                        .ui_add_manual(
                            |ui| {
                                if let Some(res) = state.show_body_unindented(ui, |ui| {
                                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                                    let gap = ui.spacing().item_spacing.y * 2.0;
                                    egui_taffy::tui(ui, ui.id().with("ext"))
                                        .reserve_available_width()
                                        .style(taffy::Style {
                                            flex_direction: taffy::FlexDirection::Column,
                                            size: percent(1.0),
                                            gap: length(gap),
                                            padding: Rect {
                                                left: length(gap),
                                                right: length(gap),
                                                bottom: length(gap),
                                                top: length(0.0),
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
                    new_layers[layer_ct] = *layer;
                    layer_ct += 1;
                }
                if duplicate {
                    new_layers[layer_ct] = *layer;
                    new_layers[layer_ct].id = next_layer_id();
                    layer_ct += 1;
                }
            }
            if swap_first != -1 {
                new_layers.swap(swap_first as usize, swap_first as usize + 1);
            }
            *self = new_layers;
            if layer_ct < 8 && add_layer {
                self[layer_ct] = Layer {
                    id: next_layer_id(),
                    kind: LayerKind::Step,
                    strength: 1.0,
                    param: 0.0,
                };
            }
        });
    }
}

impl EditUI for Light {
    fn render_edit_ui(&mut self, _ctx: &egui::Context, tui: &mut egui_taffy::Tui) {
        tui.horizontal().add(|tui| {
            ui_with_label(tui, "Color", None, |tui| {
                color_edit(tui, &mut self.color);
            });
            input_with_label(
                tui,
                "Strength",
                None,
                egui::DragValue::new(&mut self.strength).speed(0.003),
            );
        });
        tui.horizontal().add(|tui| {
            tui.grow().label("Direction");
            tui.ui_add(egui::DragValue::new(&mut self.direction[0]).speed(0.003));
            tui.ui_add(egui::DragValue::new(&mut self.direction[1]).speed(0.003));
            tui.ui_add(egui::DragValue::new(&mut self.direction[2]).speed(0.003));
        });
        self.normalize()
    }
}
