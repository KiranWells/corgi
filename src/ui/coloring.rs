use std::mem::discriminant;

use super::{
    EditUI, input_with_label,
    utils::{collapsible, ui_with_label},
};
use crate::types::{Coloring2, Gradient, Layer, LayerKind, Light, LightingKind, Overlays};
use eframe::egui::{self, RichText};
use egui_material_icons::icons;
use egui_taffy::TuiBuilderLogic;
use taffy::prelude::*;

fn color_edit(tui: &mut egui_taffy::Tui, color: &mut [f32; 3]) {
    tui.ui_add_manual(
        |ui| egui::widgets::color_picker::color_edit_button_rgb(ui, color),
        |res, _ui| res,
    );
}

fn pseudo_color_edit(tui: &mut egui_taffy::Tui, color: &mut [f32; 3]) {
    tui.style(Style {
        flex_direction: FlexDirection::Row,
        gap: length(8.0),
        ..Default::default()
    })
    .add(|tui| {
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
        tui.separator();
        collapsible(tui, "Colors", |tui| {
            input_with_label(
                tui,
                "Gradient repeat frequency",
                egui::DragValue::new(&mut self.color_frequency).speed(0.003),
            );
            input_with_label(
                tui,
                "Gradient offset",
                egui::DragValue::new(&mut self.color_offset)
                    .speed(0.003)
                    .range(0.0..=1.0),
            );
            self.gradient.render_edit_ui(ctx, tui);
            self.color_layers.render_edit_ui(ctx, tui);
            tui.separator();
        });
        collapsible(tui, "Lighting", |tui| {
            self.lighting_kind.render_edit_ui(ctx, tui);
            if self.lighting_kind != LightingKind::Flat {
                self.light_layers.render_edit_ui(ctx, tui);
            }
            if self.lighting_kind == LightingKind::Shaded {
                for light in self.lights.iter_mut() {
                    light.render_edit_ui(ctx, tui);
                }
            }
            tui.separator();
        });
        collapsible(tui, "Overlays", |tui| {
            self.overlays.render_edit_ui(ctx, tui);
        });
    }
}

impl EditUI for Gradient {
    fn render_edit_ui(&mut self, _ctx: &egui::Context, tui: &mut egui_taffy::Tui) {
        let flat = discriminant(&Gradient::Flat([0.0; 3]));
        let procedural = discriminant(&Gradient::Procedural([[0.0; 3]; 4]));
        let manual = discriminant(&Gradient::Manual([[0.0; 4]; 3]));
        let hue = discriminant(&Gradient::Hsv(0.0, 0.0));
        let label = match discriminant(self) {
            x if x == flat => "Flat",
            x if x == procedural => "Procedural",
            x if x == manual => "Manual",
            x if x == hue => "Hue",
            _ => unreachable!(),
        };
        let mut tmp = discriminant(self);
        tui.ui_add_manual(
            |ui| {
                egui::ComboBox::from_label("Gradient Type")
                    .selected_text(label)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut tmp, flat, "Flat");
                        ui.selectable_value(&mut tmp, manual, "Manual");
                        ui.selectable_value(&mut tmp, procedural, "Procedural");
                        ui.selectable_value(&mut tmp, hue, "Hue");
                    })
                    .response
            },
            |res, _ui| res,
        );
        if tmp != discriminant(self) {
            *self = match tmp {
                x if x == flat => Gradient::Flat([1.0; 3]),
                x if x == procedural => {
                    Gradient::Procedural([[0.5; 3], [0.5; 3], [1.0; 3], [0.0, 0.1, 0.2]])
                }
                x if x == manual => Gradient::Manual([
                    [0.6, 0.9, 0.8, 0.1],
                    [0.2, 0.2, 0.3, 0.5],
                    [1.0, 1.0, 1.0, 1.0],
                ]),
                x if x == hue => Gradient::Hsv(0.7, 1.0),
                _ => unreachable!(),
            };
        }

        match self {
            Gradient::Flat(color) => {
                ui_with_label(tui, "Color", |tui| {
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
                let [a, b, c] = colors;
                let (a_stop, a) = a.split_first_chunk_mut::<3>().unwrap();
                let (b_stop, b) = b.split_first_chunk_mut::<3>().unwrap();
                let (c_stop, c) = c.split_first_chunk_mut::<3>().unwrap();
                tui.style(taffy::Style {
                    display: taffy::Display::Grid,
                    // align_items: Some(taffy::AlignItems::Stretch),
                    // justify_items: Some(taffy::AlignItems::Stretch),
                    grid_template_rows: vec![min_content(); 2],
                    grid_template_columns: vec![min_content(); 3],
                    align_items: Some(AlignItems::Center),
                    gap: length(8.),
                    ..Default::default()
                })
                .add(move |tui| {
                    color_edit(tui, a_stop);
                    color_edit(tui, b_stop);
                    color_edit(tui, c_stop);
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
            Gradient::Hsv(saturation, value) => {
                input_with_label(
                    tui,
                    "Saturation",
                    egui::DragValue::new(saturation)
                        .speed(0.003)
                        .range(0.0..=1.0),
                );
                input_with_label(
                    tui,
                    "Value",
                    egui::DragValue::new(value).speed(0.003).range(0.0..=1.0),
                );
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
            LayerKind::SmoothStep => input_with_label(
                tui,
                "Offset",
                egui::DragValue::new(&mut self.param).speed(0.01),
            ),
            LayerKind::Distance => input_with_label(
                tui,
                "Boost",
                egui::DragValue::new(&mut self.param).speed(0.01),
            ),
            LayerKind::OrbitTrap | LayerKind::Stripe => {
                let mut index = self.param as i32 + 1;
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
                        .speed(0.003)
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
            self.iteration_outline_color[3].fract(),
        );
        let mut steps = self.iteration_outline_color[3] as i32;
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
        tui.ui_add_manual(
            |ui| {
                egui::ComboBox::from_label("Lighting")
                    .selected_text(format!("{self:?}"))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(self, LightingKind::Flat, "Flat");
                        ui.selectable_value(self, LightingKind::Gradient, "Gradient");
                        ui.selectable_value(
                            self,
                            LightingKind::RepeatingGradient,
                            "Repeating Gradient",
                        );
                        ui.selectable_value(self, LightingKind::Shaded, "Shaded");
                    })
                    .response
            },
            |res, _ui| res,
        );
    }
}

impl EditUI for [Layer; 8] {
    fn render_edit_ui(&mut self, ctx: &egui::Context, tui: &mut egui_taffy::Tui) {
        let valid_ct = self.iter().filter(|l| l.kind != LayerKind::None).count();
        let mut add = false;
        tui.style(Style {
            justify_content: Some(AlignContent::SpaceBetween),
            ..Default::default()
        })
        .add(|tui| {
            tui.label(RichText::new("Layers").strong());
            if valid_ct < 8 {
                add = tui
                    .button(|tui| {
                        tui.label(format!("{} Add Layer", icons::ICON_ADD));
                    })
                    .clicked();
            }
        });
        let mut layer_ct = 0;
        let mut new_layers = [Layer::default(); 8];
        for layer in self.iter_mut() {
            if layer.kind == LayerKind::None {
                break;
            }
            let mut remove = false;
            let mut dup = false;
            collapsible(tui, &layer.kind.icon_text(), |tui| {
                layer.render_edit_ui(ctx, tui);
                tui.style(Style {
                    flex_direction: FlexDirection::Row,
                    gap: length(4.0),
                    size: Size {
                        width: percent(1.0),
                        height: auto(),
                    },
                    ..Default::default()
                })
                .add(|tui| {
                    remove = tui
                        .style(Style {
                            flex_grow: 1.0,
                            ..Default::default()
                        })
                        .button(|tui| {
                            tui.label(format!("{} Remove", icons::ICON_REMOVE_CIRCLE_OUTLINE))
                        })
                        .clicked();
                    if valid_ct < 8 {
                        dup = tui
                            .style(Style {
                                flex_grow: 1.0,
                                ..Default::default()
                            })
                            .button(|tui| {
                                tui.label(format!("{} Duplicate", icons::ICON_TAB_DUPLICATE))
                            })
                            .clicked();
                    }
                });
            });
            if !remove {
                new_layers[layer_ct] = *layer;
                layer_ct += 1;
            }
            if dup {
                new_layers[layer_ct] = *layer;
                layer_ct += 1;
            }
        }
        *self = new_layers;
        if layer_ct < 8 && add {
            self[layer_ct] = Layer {
                kind: LayerKind::Step,
                strength: 1.0,
                param: 0.0,
            };
        }
    }
}

impl EditUI for Light {
    fn render_edit_ui(&mut self, _ctx: &egui::Context, tui: &mut egui_taffy::Tui) {
        tui.style(Style {
            flex_direction: FlexDirection::Row,
            ..Default::default()
        })
        .add(|tui| {
            color_edit(tui, &mut self.color);
            input_with_label(
                tui,
                "Strength",
                egui::DragValue::new(&mut self.strength).speed(0.003),
            );
        });
        tui.style(Style {
            flex_direction: FlexDirection::Row,
            ..Default::default()
        })
        .add(|tui| {
            tui.label("Direction");
            tui.separator();
            tui.ui_add(egui::DragValue::new(&mut self.direction[0]).speed(0.003));
            tui.ui_add(egui::DragValue::new(&mut self.direction[1]).speed(0.003));
            tui.ui_add(egui::DragValue::new(&mut self.direction[2]).speed(0.003));
        });
        self.normalize()
    }
}
