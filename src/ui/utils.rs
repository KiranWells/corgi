use eframe::egui;
use egui_taffy::{Tui, TuiBuilderLogic, TuiWidget};
use rug::Float;
use taffy::{Overflow, prelude::*};

use corgi::types::ComplexPoint;

pub fn collapsible(tui: &mut egui_taffy::Tui, summary: &str, add_contents: impl FnOnce(&mut Tui)) {
    tui.ui_add_manual(
        |ui| {
            let cr = egui::CollapsingHeader::new(summary)
                .default_open(true)
                .show(ui, |ui| {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                    egui_taffy::tui(ui, ui.id().with("ext"))
                        .reserve_available_width()
                        .style(taffy::Style {
                            flex_direction: taffy::FlexDirection::Column,
                            size: percent(1.0),
                            flex_grow: 1.0,
                            gap: length(8.),
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

pub fn input_with_label(tui: &mut egui_taffy::Tui, label: &str, widget: impl TuiWidget) {
    ui_with_label(tui, label, |tui| {
        tui.ui_add(widget);
    });
}

pub fn ui_with_label(tui: &mut egui_taffy::Tui, label: &str, add_contents: impl FnOnce(&mut Tui)) {
    tui.style(taffy::Style {
        flex_direction: taffy::FlexDirection::Row,
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
        .label(label);
        add_contents(tui);
    });
}

#[derive(Clone, Debug)]
struct FloatPointEditState {
    x_text: String,
    y_text: String,
}

pub fn point_edit(tui: &mut Tui, point_name: &str, precision: u32, point: &mut ComplexPoint) {
    let id = tui.egui_ui().next_auto_id();
    let mut state =
        tui.egui_ctx()
            .data_mut(|d| d.get_persisted(id))
            .unwrap_or(FloatPointEditState {
                x_text: point.x.to_string_radix(10, None),
                y_text: point.y.to_string_radix(10, None),
            });

    tui.label(point_name);
    tui.style(taffy::Style {
        size: Size {
            width: percent(1.0),
            height: auto(),
        },
        display: taffy::Display::Grid,
        align_items: Some(taffy::AlignItems::Stretch),
        justify_items: Some(taffy::AlignItems::Stretch),
        grid_template_rows: vec![min_content(); 2],
        grid_template_columns: vec![auto(), auto()],
        gap: length(8.),
        padding: Rect {
            left: length(10.0),
            right: length(0.0),
            top: length(0.0),
            bottom: length(0.0),
        },
        ..Default::default()
    })
    .add(|tui| {
        for (label, text_reference, value_reference) in [
            ("real", &mut state.x_text, &mut point.x),
            ("imaginary", &mut state.y_text, &mut point.y),
        ] {
            tui.label(label);
            let response = tui.ui_add(egui::TextEdit::singleline(text_reference));
            if !response.has_focus() {
                *text_reference = value_reference.to_string_radix(10, None);
            } else if let Ok(res) = Float::parse(text_reference) {
                *value_reference = Float::with_val(precision, res);
            }
        }
    });
    tui.egui_ctx().data_mut(|d| d.insert_persisted(id, state));
}
