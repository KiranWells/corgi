use std::f32::consts::PI;

use corgi_lib::types::{ComplexPoint, ImgSpec, Rotate, get_precision};
use eframe::egui::{Color32, Pos2, Sense, Stroke, UiBuilder, Vec2};
use eframe::{egui, egui_wgpu};
use rug::Float;
use rug::ops::PowAssign;

use crate::ui::preview_resources::PaintCallback;
use crate::ui::tabs::UITab;

impl super::CorgiUI {
    /// Render the image preview viewport
    pub(super) fn viewport(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
    ) -> Option<ComplexPoint> {
        let mut new_max_rect = ui.max_rect();
        new_max_rect.set_height(new_max_rect.height() - 20.0);
        let mut hover_pt = None;
        ui.scope_builder(
            UiBuilder::new().sense(Sense::drag()).max_rect(new_max_rect),
            |ui| {
                let size = ui.available_size();
                let (_id, rect) = ui.allocate_space(size);

                // handle mouse events

                // get input beforehand
                let pointer_in_rect = ui.rect_contains_pointer(rect);
                let (primary_down, pointer_pos) = ctx.input(|i| {
                    (
                        i.pointer.button_down(egui::PointerButton::Primary),
                        i.pointer.interact_pos(),
                    )
                });

                let view_image = self.image();
                // update image settings
                if pointer_in_rect && let Some(pos) = pointer_pos {
                    hover_pt = Some(
                        view_image
                            .view()
                            .px_to_complex(pos, self.explore_state.scaling),
                    );
                }
                if self.setting_probe {
                    // probe setting mode, set the probe location to the mouse position
                    // on click
                    if primary_down && let Some(hover_pt) = hover_pt.clone() {
                        self.root_spec.location.probe_location = hover_pt;
                        self.setting_probe = false;
                    }
                } else {
                    self.handle_viewport_input(ui, pointer_in_rect, &view_image);
                }

                self.current_view.width = size.x as u32;
                self.current_view.height = size.y as u32;

                let cb = PaintCallback {
                    rendered_viewport: match self.tab {
                        UITab::Render => self.render_state.rendered_view.clone(),
                        UITab::Explore => self.explore_state.rendered_view.clone(),
                        UITab::Style => self.style_state.rendered_view.clone(),
                    },
                    view: view_image.view(),
                    swap: self.swap,
                    tab: self.tab,
                };

                let callback = egui_wgpu::Callback::new_paint_callback(rect, cb);

                // this paint call must be before others for some reason
                ui.painter().add(callback);
            },
        );
        hover_pt
    }

    pub(super) fn render_widgets(
        &self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        context: &mut crate::Context,
    ) {
        fn paint_crosshair(
            painter: &egui::Painter,
            center: egui::Pos2,
            radius: f32,
            stroke: Stroke,
        ) {
            painter.line_segment(
                [
                    center + Vec2::new(0.0, -radius),
                    center + Vec2::new(0.0, radius),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    center + Vec2::new(-radius, 0.0),
                    center + Vec2::new(radius, 0.0),
                ],
                stroke,
            );
        }
        fn rotated_text(
            painter: &egui::Painter,
            ctx: &egui::Context,
            theme: &crate::config::Theme,
            pos: Pos2,
            anchor: egui::Align2,
            text: String,
            angle: f32,
        ) {
            let (t, r) = ctx.fonts_mut(|f| {
                let mut t = egui::Shape::text(
                    f,
                    pos,
                    anchor,
                    text,
                    egui::FontId::proportional(theme.base_rem),
                    Color32::WHITE,
                );

                let mut rect = egui::Rect::ZERO;
                let mut pos = Pos2::ZERO;
                if let egui::epaint::Shape::Text(ts) = &mut t {
                    *ts = ts.clone().with_angle_and_anchor(angle, anchor);
                    rect = ts.galley.rect.expand(theme.spacing / 2.0);
                    pos = ts.pos;
                };
                let rotator = egui::emath::Rot2::from_angle(angle);

                let r = egui::Shape::Path(egui::epaint::PathShape {
                    points: vec![
                        rect.left_top(),
                        rect.right_top(),
                        rect.right_bottom(),
                        rect.left_bottom(),
                    ]
                    .into_iter()
                    .map(|p| pos + rotator * p.to_vec2())
                    .collect(),
                    closed: true,
                    fill: Color32::from_black_alpha(150),
                    stroke: egui::epaint::PathStroke::NONE,
                });

                (t, r)
            });
            painter.add(r);
            painter.add(t);
        }
        let view_image = self.image();
        let camera_view = if self.tab == UITab::Render {
            self.render_state.rendered_view.clone()
        } else {
            self.root_spec.view()
        };
        let rect =
            egui::Rect::from_min_size(Pos2::ZERO, view_image.size() / self.viewport_scaling());
        let mut render_rect =
            rect.scale_from_center2(egui::Vec2::splat(1.0) / view_image.view().aspect_scale());
        let mut offset = view_image.view().complex_to_px_delta(&camera_view.center);
        offset.y *= -1.0;
        render_rect = render_rect.translate(offset * self.viewport_scaling());
        render_rect = render_rect.scale_from_center(f32::powf(
            2.0,
            -(camera_view.zoom - view_image.location.zoom),
        ));
        render_rect = render_rect.scale_from_center2(camera_view.aspect_scale());
        let painter = ui.painter();
        let simple_stroke = Stroke::new(2.0, Color32::WHITE);
        let center = egui::Pos2::ZERO + self.current_view.size() / 2.0;
        let current_angle = if self.tab == UITab::Render {
            self.current_view.angle - self.render_state.rendered_view.angle
        } else {
            self.root_spec.location.angle
        };
        let (pointer, modifiers, scroll) =
            ui.input(|i| (i.pointer.clone(), i.modifiers, i.smooth_scroll_delta));
        let theme = context.theme();
        if pointer.secondary_down()
            && let Some(pos) = pointer.latest_pos()
        {
            // render rotation guide line
            let current_angle = if modifiers.ctrl {
                (current_angle / (PI / 12.0)).round() * (PI / 12.0)
            } else {
                current_angle
            };
            let radius = (pos - center).length();
            painter.circle_stroke(center, radius, simple_stroke);
            let guideline = Vec2::new(radius, 0.0).rotated(current_angle);
            painter.line_segment([center, center + guideline], simple_stroke);
            rotated_text(
                painter,
                ctx,
                theme,
                center + guideline / 2.0 + Vec2::new(0.0, -theme.spacing).rotated(current_angle),
                egui::Align2::CENTER_BOTTOM,
                format!("{:.2}°", current_angle.to_degrees()),
                current_angle,
            );
        }
        if pointer.middle_down() {
            paint_crosshair(painter, center, 10.0, simple_stroke);
        }
        if (scroll.length() > 0.0 || self.setting_probe)
            && let Some(pos) = pointer.latest_pos()
        {
            paint_crosshair(painter, pos, 10.0, simple_stroke);
        }
        if self.show_camera || self.tab == UITab::Render {
            if self.tab != UITab::Render {
                painter.rect_stroke(
                    render_rect.intersect(rect),
                    0.0,
                    simple_stroke,
                    egui::StrokeKind::Outside,
                );
            }
            let current_angle = if self.tab == UITab::Render {
                current_angle
            } else {
                0.0
            };
            let text_anchor = render_rect.center()
                + (render_rect.center_bottom() - render_rect.center()
                    + Vec2::new(0.0, theme.spacing))
                .rotated(current_angle);

            rotated_text(
                painter,
                ctx,
                theme,
                text_anchor,
                egui::Align2::CENTER_TOP,
                format!("{}x{}", camera_view.width, camera_view.height),
                current_angle,
            );
        }
    }

    /// Draw tool widgets onto the preview viewport
    pub(super) fn handle_viewport_input(
        &mut self,
        ui: &mut egui::Ui,
        pointer_in_rect: bool,
        view_image: &ImgSpec,
    ) {
        let response = ui.response();
        if response.rect.width() < 1.0 || response.rect.height() < 1.0 {
            // The viewport is too small for the following math to make sense
            return;
        }
        // get inputs to change the viewport
        let (mut scroll, pixel_scale, mouse, modifiers) = ui.input(|i| {
            (
                i.smooth_scroll_delta,
                i.pixels_per_point,
                i.pointer.clone(),
                i.modifiers,
            )
        });
        if !pointer_in_rect {
            scroll = Vec2::ZERO;
        }
        let drag = response.drag_delta();

        // calculate deltas
        let precision = get_precision(view_image.location.zoom);
        let mut scale = Float::with_val(precision, 2.0);
        scale.pow_assign(-view_image.location.zoom);
        let viewport_scaling = self.viewport_scaling();
        let ComplexPoint {
            x: x_offset,
            y: y_offset,
        } = view_image
            .view()
            .px_delta_to_complex_delta(drag, viewport_scaling);
        let mut scroll_zoom =
            if modifiers.shift { scroll.x } else { scroll.y } * pixel_scale * 0.005;
        let mut drag_zoom = (drag.x + drag.y) * pixel_scale * 0.01;
        let mut drag_rotation = if let Some(pos) = mouse.latest_pos() {
            let center = response.rect.size() / 2.0;
            let start_point = (pos - drag).to_vec2();
            let end_point = pos.to_vec2();
            (end_point - center).angle() - (start_point - center).angle()
        } else {
            0.0
        };

        // adjust behavior if modifiers are pressed
        if modifiers.shift {
            scroll_zoom *= 0.1;
            drag_zoom *= 0.1;
            drag_rotation *= 0.1;
        }
        if modifiers.alt {
            scroll_zoom *= 3.0;
            drag_zoom *= 3.0;
            drag_rotation *= 3.0;
        }

        // apply deltas to relevant view or location
        if self.tab == UITab::Render {
            self.current_view.zoom += scroll_zoom;
            if scroll.y != 0.0
                && let Some(pos) = mouse.latest_pos()
            {
                let unzoomed_real = view_image.view().px_to_complex(pos, viewport_scaling);
                let zoomed_real = self.current_view.px_to_complex(pos, viewport_scaling);
                self.current_view.center.x -= zoomed_real.x - unzoomed_real.x;
                self.current_view.center.y -= zoomed_real.y - unzoomed_real.y;
            }
            if mouse.primary_down() {
                self.current_view.center.x -= x_offset;
                self.current_view.center.y -= y_offset;
            }
            if mouse.secondary_down() {
                self.current_view.angle += drag_rotation;
                self.current_view.angle %= PI * 2.0;
            }
            if mouse.middle_down() {
                self.current_view.zoom += drag_zoom;
            }
            if modifiers.ctrl && mouse.secondary_released() {
                let reference_angle = self.render_state.rendered_view.angle;
                // round values
                self.current_view.angle =
                    ((self.current_view.angle - reference_angle) / (PI / 12.0)).round()
                        * (PI / 12.0)
                        + reference_angle;
            }
        } else {
            self.root_spec.location.zoom += scroll_zoom;
            if scroll.y != 0.0
                && let Some(pos) = mouse.latest_pos()
            {
                let unzoomed_real = view_image.view().px_to_complex(pos, viewport_scaling);
                let zoomed_real = self.image().view().px_to_complex(pos, viewport_scaling);
                self.root_spec.location.center.x -= zoomed_real.x - unzoomed_real.x;
                self.root_spec.location.center.y -= zoomed_real.y - unzoomed_real.y;
            }
            // including "none" down to support touch
            if mouse.primary_down() || !mouse.any_down() {
                self.root_spec.location.center.x -= x_offset;
                self.root_spec.location.center.y -= y_offset;
            }
            if mouse.secondary_down() {
                self.root_spec.location.angle += drag_rotation;
                self.root_spec.location.angle %= PI * 2.0;
            }
            if mouse.middle_down() {
                self.root_spec.location.zoom += drag_zoom;
            }
            if modifiers.ctrl && mouse.secondary_released() {
                // round values
                self.root_spec.location.angle =
                    (self.root_spec.location.angle / (PI / 12.0)).round() * (PI / 12.0);
            }
            self.root_spec.location.update_prec();
            self.root_spec.update_probe();
        }
    }
}
