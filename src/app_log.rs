use ecolor::Color32;
use eframe::egui;
use egui_material_icons::icons;
use parking_lot::RwLock;
use tracing::Subscriber;
use tracing_subscriber::Layer;

static LOGS: RwLock<Vec<AppLog>> = RwLock::new(vec![]);

pub struct AppLog {
    pub level: tracing::Level,
    pub message: String,
    pub time: std::time::Instant,
}
pub struct AppSubscriber {}

impl<S: Subscriber> Layer<S> for AppSubscriber {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        if *event.metadata().level() <= tracing::Level::WARN
            && event
                .metadata()
                .module_path()
                .is_some_and(|p| p.split_once(':').is_some_and(|x| x.0 == "corgi"))
            && let Some(mut logs) = LOGS.try_write()
        {
            logs.push(AppLog::from(event))
        }
    }
}

impl<'a> From<&tracing::Event<'a>> for AppLog {
    fn from(value: &tracing::Event<'a>) -> Self {
        struct MsgVisitor {
            msg: String,
        }
        impl tracing::field::Visit for MsgVisitor {
            fn record_debug(
                &mut self,
                field: &tracing::field::Field,
                value: &dyn core::fmt::Debug,
            ) {
                if field.name() == "message" {
                    self.msg = format!("{value:?}");
                }
            }
        }
        let mut mv = MsgVisitor { msg: String::new() };
        value.record(&mut mv);
        AppLog {
            level: *value.metadata().level(),
            message: mv.msg,
            time: std::time::Instant::now(),
        }
    }
}

/// Allows handling the global stored logs. DO NOT log new messages within this scope
pub fn logs_mut(callback: impl FnOnce(&mut Vec<AppLog>)) {
    callback(&mut LOGS.write());
}

pub fn logs_ui(ui: &mut egui::Ui, origin_rect: egui::Rect) {
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(origin_rect.shrink(ui.spacing().indent))
            .layout(egui::Layout {
                main_dir: egui::Direction::TopDown,
                main_wrap: false,
                main_align: egui::Align::Min,
                main_justify: false,
                cross_align: egui::Align::Min,
                cross_justify: false,
            }),
        |ui| {
            logs_mut(|logs| {
                let mut closed = None;
                for (i, log) in logs.iter().enumerate() {
                    egui::Frame::new()
                        .shadow(egui::Shadow {
                            offset: [5, 5],
                            blur: 5,
                            spread: 0,
                            color: Color32::from_black_alpha(128),
                        })
                        .fill(ui.visuals().window_fill)
                        .show(ui, |ui| {
                            ui.set_height(
                                ui.text_style_height(&egui::TextStyle::Button)
                                    + ui.spacing().button_padding.y * 2.0,
                            );
                            ui.set_width(ui.spacing().indent * 10.0);
                            let log_color = match log.level {
                                tracing::Level::ERROR => ui.visuals().error_fg_color,
                                tracing::Level::WARN => ui.visuals().warn_fg_color,
                                _ => ui.visuals().text_color(),
                            };
                            egui::Frame::new().fill(log_color).show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.add_space(ui.spacing().item_spacing.x);
                                    ui.label(
                                        egui::RichText::new(match log.level {
                                            tracing::Level::ERROR => {
                                                format!("{} Error", icons::ICON_ERROR.codepoint)
                                            }
                                            tracing::Level::WARN => {
                                                format!("{} Warning", icons::ICON_WARNING.codepoint)
                                            }
                                            _ => format!("{} Notice", icons::ICON_INFO.codepoint),
                                        })
                                        .color(ui.visuals().window_fill),
                                    );
                                    ui.add_space(ui.available_width() - ui.available_height());
                                    if ui
                                        .add(
                                            egui::Button::new(
                                                egui::RichText::new(icons::ICON_CLOSE)
                                                    .color(ui.visuals().window_fill),
                                            )
                                            .fill(
                                                log_color
                                                    .lerp_to_gamma(ui.visuals().window_fill, 0.25),
                                            )
                                            .corner_radius(0.0)
                                            .frame_when_inactive(false),
                                        )
                                        .clicked()
                                    {
                                        closed = Some(i);
                                    }
                                });
                            });
                            egui::Frame::new()
                                .inner_margin(ui.spacing().button_padding * 2.0)
                                .show(ui, |ui| {
                                    ui.label(&log.message);
                                });
                        });
                    // not sure why, but `add_space` does nothing here
                    ui.label("");
                }
                let mut offset = 0;
                for i in 0..logs.len() {
                    let i = i - offset;
                    if logs[i].level != tracing::Level::ERROR
                        && std::time::Instant::now() - logs[i].time
                            > std::time::Duration::from_secs(5)
                        || closed == Some(i)
                    {
                        logs.remove(i);
                        offset += 1;
                    }
                }
            });
        },
    );
}
