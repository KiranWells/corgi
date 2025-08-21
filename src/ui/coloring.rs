use std::mem::discriminant;

use super::input_with_label;
use crate::types::{Coloring2, Gradient, Layer, LayerKind, Overlays};
use eframe::egui;
use egui_taffy::TuiBuilderLogic;
use taffy::prelude::*;

fn color_edit(tui: &mut egui_taffy::Tui, color: &mut [f32; 3]) {
    tui.ui_add_manual(
        |ui| egui::widgets::color_picker::color_edit_button_rgb(ui, color),
        |res, _ui| res,
    );
}

pub trait EditUI {
    fn render_edit_ui(&mut self, ctx: &egui::Context, tui: &mut egui_taffy::Tui);
}

impl EditUI for Coloring2 {
    fn render_edit_ui(&mut self, ctx: &egui::Context, tui: &mut egui_taffy::Tui) {
        input_with_label(
            tui,
            "Saturation",
            egui::DragValue::new(&mut self.saturation)
                .speed(0.003)
                .range(0.0..=f32::MAX),
        );
        input_with_label(
            tui,
            "Brightness",
            egui::DragValue::new(&mut self.brightness)
                .speed(0.003)
                .range(0.0..=f32::MAX),
        );
        input_with_label(
            tui,
            "Color frequency",
            egui::DragValue::new(&mut self.color_frequency).speed(0.003),
        );
        input_with_label(
            tui,
            "Color offset",
            egui::DragValue::new(&mut self.color_offset)
                .speed(0.003)
                .range(0.0..=1.0),
        );

        self.gradient.render_edit_ui(ctx, tui);
        let mut layer_ct = 0;
        let mut new_layers = [Layer::default(); 8];
        for layer in self.color_layers.iter_mut() {
            if layer.kind == LayerKind::None {
                break;
            }
            tui.style(Style {
                flex_direction: FlexDirection::Row,
                ..Default::default()
            })
            .add(|tui| {
                tui.label(layer_ct.to_string());
                tui.style(Style {
                    flex_direction: FlexDirection::Column,
                    align_items: Some(AlignItems::Start),
                    padding: Rect {
                        left: length(5.0),
                        right: length(5.0),
                        top: length(0.0),
                        bottom: length(0.0),
                    },
                    gap: length(5.0),
                    ..Default::default()
                })
                .add(|tui| {
                    layer.render_edit_ui(ctx, tui);
                    if !tui.button(|tui| tui.label("Remove")).clicked() {
                        new_layers[layer_ct] = *layer;
                        layer_ct += 1;
                    }
                })
            });
        }
        self.color_layers = new_layers;
        if layer_ct < 8
            && tui
                .button(|tui| {
                    tui.label("Add Layer");
                })
                .clicked()
        {
            self.color_layers[layer_ct] = Layer {
                kind: LayerKind::Step,
                strength: 1.0,
                param: 0.0,
            };
        }
        tui.separator();
        tui.label("Overlays");
        self.overlays.render_edit_ui(ctx, tui);
    }
}

impl EditUI for Gradient {
    fn render_edit_ui(&mut self, _ctx: &egui::Context, tui: &mut egui_taffy::Tui) {
        let flat = discriminant(&Gradient::Flat([0.0; 3]));
        let procedural = discriminant(&Gradient::Procedural([[0.0; 3]; 4]));
        let manual = discriminant(&Gradient::Manual([[0.0; 4]; 3]));
        let label = match discriminant(self) {
            x if x == flat => "Flat",
            x if x == procedural => "Procedural",
            x if x == manual => "Manual",
            _ => unreachable!(),
        };
        let mut tmp = discriminant(self);
        tui.ui_add_manual(
            |ui| {
                egui::ComboBox::from_label("Gradient")
                    .selected_text(label)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut tmp, flat, "Flat");
                        ui.selectable_value(&mut tmp, manual, "Manual");
                        ui.selectable_value(&mut tmp, procedural, "Procedural");
                    })
                    .response
            },
            |res, _ui| res,
        );
        tui.separator();
        if tmp != discriminant(self) {
            *self = match tmp {
                x if x == flat => Gradient::Flat([1.0; 3]),
                x if x == procedural => {
                    Gradient::Procedural([[0.5; 3], [0.5; 3], [1.0; 3], [0.0, 0.1, 0.2]])
                }
                x if x == manual => Gradient::Manual([[0.7; 4], [0.5; 4], [0.0; 4]]),
                _ => unreachable!(),
            };
        }

        match self {
            Gradient::Flat(color) => {
                tui.add(|tui| {
                    color_edit(tui, color);
                });
            }
            Gradient::Procedural(colors) => {
                tui.style(taffy::Style {
                    flex_direction: FlexDirection::Row,
                    ..Default::default()
                })
                .add(|tui| {
                    color_edit(tui, &mut colors[0]);
                    color_edit(tui, &mut colors[1]);
                    color_edit(tui, &mut colors[2]);
                    color_edit(tui, &mut colors[3]);
                });
            }
            Gradient::Manual(colors) => {
                let [a, b, c] = colors;
                let (a_stop, a) = a.split_first_chunk_mut::<3>().unwrap();
                let (b_stop, b) = b.split_first_chunk_mut::<3>().unwrap();
                let (c_stop, c) = c.split_first_chunk_mut::<3>().unwrap();
                tui.style(taffy::Style {
                    flex_direction: FlexDirection::Row,
                    ..Default::default()
                })
                .add(move |tui| {
                    color_edit(tui, a_stop);
                    color_edit(tui, b_stop);
                    color_edit(tui, c_stop);
                });
                tui.style(taffy::Style {
                    flex_direction: FlexDirection::Row,
                    ..Default::default()
                })
                .add(move |tui| {
                    tui.ui_add(
                        egui::DragValue::new(&mut a[0])
                            .speed(0.003)
                            .range(0.0..=1.0),
                    );
                    tui.ui_add(
                        egui::DragValue::new(&mut b[0])
                            .speed(0.003)
                            .range(0.0..=1.0),
                    );
                    tui.ui_add(
                        egui::DragValue::new(&mut c[0])
                            .speed(0.003)
                            .range(0.0..=1.0),
                    );
                });
            }
        };
    }
}

impl EditUI for Layer {
    fn render_edit_ui(&mut self, _ctx: &egui::Context, tui: &mut egui_taffy::Tui) {
        tui.ui_add_manual(
            |ui| {
                egui::ComboBox::from_label("Layer Type")
                    .selected_text(format!("{:?}", self.kind))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.kind, LayerKind::Step, "Step");
                        ui.selectable_value(&mut self.kind, LayerKind::SmoothStep, "Smooth Step");
                        ui.selectable_value(&mut self.kind, LayerKind::Distance, "Distance");
                        ui.selectable_value(&mut self.kind, LayerKind::OrbitTrap, "Orbit Trap");
                        ui.selectable_value(&mut self.kind, LayerKind::Stripe, "Stripe Average");
                        ui.selectable_value(&mut self.kind, LayerKind::Normal, "Normal");
                    })
                    .response
            },
            |res, _ui| res,
        );
        input_with_label(
            tui,
            "Strength",
            egui::DragValue::new(&mut self.strength).speed(0.01),
        );
        match self.kind {
            LayerKind::None => unreachable!(),
            LayerKind::Step => {}
            LayerKind::SmoothStep => {}
            LayerKind::Distance => input_with_label(
                tui,
                "Boost",
                egui::DragValue::new(&mut self.param).speed(0.01),
            ),
            LayerKind::OrbitTrap => {
                let mut index = self.param as i32 + 1;

                input_with_label(
                    tui,
                    "Version",
                    egui::DragValue::new(&mut index).speed(0.03).range(1..=4),
                );
                self.param = index as f32 - 1.0;
            }
            LayerKind::Normal => input_with_label(
                tui,
                "Minimum Brightness",
                egui::DragValue::new(&mut self.param).speed(0.01),
            ),
            LayerKind::Stripe => {
                let mut index = self.param.floor() as i32 + 1;
                let mut offset = self.param.fract();

                input_with_label(
                    tui,
                    "Version",
                    egui::DragValue::new(&mut index).speed(0.03).range(1..=4),
                );
                input_with_label(
                    tui,
                    "Offset",
                    egui::DragValue::new(&mut offset)
                        .speed(0.01)
                        .range(0.0..=0.99),
                );

                self.param = index as f32 - 1.0 + offset;
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
            self.iteration_outline_color[3],
        );
        tui.style(taffy::Style {
            flex_direction: taffy::FlexDirection::Row,
            // flex_grow: 1.0,
            justify_content: Some(taffy::AlignContent::Stretch),
            size: Size {
                width: percent(1.0),
                height: auto(),
            },
            ..Default::default()
        })
        .add(|tui| {
            tui.style(taffy::Style {
                flex_grow: 1.0,
                ..Default::default()
            })
            .label("Step Outline");
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
        });
        self.iteration_outline_color = rgba.to_rgba_unmultiplied();

        let (rgb, a) = self.set_outline_color.split_first_chunk_mut().unwrap();
        tui.style(taffy::Style {
            flex_direction: taffy::FlexDirection::Row,
            // flex_grow: 1.0,
            justify_content: Some(taffy::AlignContent::Stretch),
            size: Size {
                width: percent(1.0),
                height: auto(),
            },
            ..Default::default()
        })
        .add(|tui| {
            tui.style(taffy::Style {
                flex_grow: 1.0,
                ..Default::default()
            })
            .label("Set Outline");
            tui.ui_add_manual(
                |ui| egui::widgets::color_picker::color_edit_button_rgb(ui, rgb),
                |res, _ui| res,
            );
            tui.ui_add(egui::DragValue::new(&mut a[0]).speed(0.003));
        });
    }
}
