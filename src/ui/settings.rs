use eframe::egui::DragValue;

use crate::ui::EditUI;
use crate::ui::utils::{color32_edit, input_with_label, ui_with_label};

impl EditUI for crate::Config {
    fn render_edit_ui(&mut self, _ctx: &eframe::egui::Context, tui: &mut egui_taffy::Tui) {
        input_with_label(
            tui,
            "Max Steps per Shader Batch in UI",
            Some(
                "The number of iterations calculated in one GPU compute batch when rendering updates to the UI. Lower this if the UI freezes for too long during viewport updates, but higher values reduce total render time.",
            ),
            DragValue::new(&mut self.ui_max_shader_batch_iters).speed(10),
        );
        input_with_label(
            tui,
            "Max Steps per Shader Batch in Render",
            Some(
                "The number of iterations calculated in one GPU compute batch during Rendering. Lower this if the UI freezes for too long during image rendering, but higher values reduce total render time.",
            ),
            DragValue::new(&mut self.max_shader_batch_iters).speed(10),
        );
    }
}

impl EditUI for crate::Theme {
    fn render_edit_ui(&mut self, ctx: &eframe::egui::Context, tui: &mut egui_taffy::Tui) {
        ui_with_label(tui, "Background Color", None, |tui| {
            color32_edit(tui, &mut self.bg_color);
        });
        ui_with_label(tui, "Text Color", None, |tui| {
            color32_edit(tui, &mut self.fg_color);
        });
        ui_with_label(tui, "Accent Color", None, |tui| {
            color32_edit(tui, &mut self.accent_color);
        });
        input_with_label(
            tui,
            "Spacing",
            Some("The measurement in logical pixels of the smallest unit of padding"),
            DragValue::new(&mut self.spacing).speed(0.03),
        );
        input_with_label(
            tui,
            "Base font size",
            Some("The measurement in logical pixels of the normal font"),
            DragValue::new(&mut self.base_rem).speed(0.03),
        );
        ctx.set_style(self.style());
    }
}
