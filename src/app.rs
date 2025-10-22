use clap::Parser;
use eframe::egui::Color32;
use eframe::egui::CornerRadius;
use eframe::egui::FontId;
use eframe::egui::Stroke;
use eframe::egui::Style;
use eframe::egui::TextStyle;
use eframe::egui::style::WidgetVisuals;
use eframe::egui::vec2;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use std::time::Instant;
use wgpu::Extent3d;

use crate::ui::{CorgiUI, PreviewRenderResources};
use crate::worker::WorkerState;
use corgi::types::{Debouncer, Image, ImageGenCommand, StatusMessage};

/// Command line options for the application
#[derive(Parser, Debug)]
#[command(
    version,
    about,
    long_about = r"
Corgi - high-precision accelerated fractal renderer.

Corgi generates fractal images using high-precision calculation methods that
allow for super deep zooms. By default, Corgi will open a UI for exploring
fractals and rendering the selected locations. It also supports directly
rendering images given image settings defined in a JSON file."
)]
pub struct CorgiCliOptions {
    /// Optional image settings file to start with. Supported formats include
    /// JSON (.json or .corg) and image files containing the necessary metadata.
    #[arg(short, long, value_name = "FILE")]
    pub settings_file: Option<PathBuf>,
    /// Optional output image location. If specified, Corgi will not launch a UI.
    /// If the image format supports metadata, the generations settings will be
    /// written into the finished file.
    #[arg(short, long, value_name = "FILE")]
    pub output_file: Option<PathBuf>,
}

struct Theme {
    bg_color: Color32,
    fg_color: Color32,
    accent_color: Color32,
    spacing: f32,
    base_rem: f32,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            bg_color: Color32::from_rgb(30, 30, 46),
            fg_color: Color32::from_rgb(186, 194, 222),
            accent_color: Color32::from_rgb(116, 199, 236),
            spacing: 4.0,
            base_rem: 12.0,
        }
    }
}

impl Theme {
    pub fn style(&self) -> Style {
        Style {
            text_styles: [
                (
                    TextStyle::Body,
                    FontId::new(self.rem(1.0), eframe::egui::FontFamily::Proportional),
                ),
                (
                    TextStyle::Button,
                    FontId::new(self.rem(1.0), eframe::egui::FontFamily::Proportional),
                ),
                (
                    TextStyle::Monospace,
                    FontId::new(self.rem(1.0), eframe::egui::FontFamily::Monospace),
                ),
                (
                    TextStyle::Small,
                    FontId::new(self.rem(0.75), eframe::egui::FontFamily::Proportional),
                ),
                (
                    TextStyle::Heading,
                    FontId::new(self.rem(1.5), eframe::egui::FontFamily::Proportional),
                ),
                (
                    TextStyle::Name("Subheading".into()),
                    FontId::new(self.rem(1.25), eframe::egui::FontFamily::Proportional),
                ),
            ]
            .into(),
            drag_value_text_style: TextStyle::Monospace,
            spacing: eframe::egui::Spacing {
                item_spacing: vec2(self.spacing, self.spacing),
                window_margin: eframe::egui::Margin::same(self.spacing as i8),
                menu_margin: eframe::egui::Margin::same(self.spacing as i8),
                button_padding: vec2(self.spacing, self.spacing),
                indent: self.spacing * 4.0,
                interact_size: vec2(self.rem(5.0), self.rem(1.5)),
                slider_width: 100.0,
                slider_rail_height: 8.0,
                combo_width: 80.0,
                text_edit_width: 200.0,
                icon_width: self.rem(1.25),
                icon_width_inner: self.rem(0.75),
                icon_spacing: self.spacing,
                default_area_size: vec2(600.0, 400.0),
                tooltip_width: 300.0,
                menu_width: 300.0,
                menu_spacing: 0.0,
                combo_height: 200.0,
                scroll: Default::default(),
                indent_ends_with_horizontal_line: false,
            },
            interaction: eframe::egui::style::Interaction {
                interact_radius: 8.0,
                resize_grab_radius_side: 5.0,
                resize_grab_radius_corner: 10.0,
                show_tooltips_only_when_still: true,
                tooltip_delay: 0.5,
                tooltip_grace_time: 0.2,
                selectable_labels: true,
                multi_widget_text_select: true,
            },
            visuals: eframe::egui::Visuals {
                dark_mode: true,
                text_alpha_from_coverage: eframe::epaint::AlphaFromCoverage::DARK_MODE_DEFAULT,
                widgets: eframe::egui::style::Widgets {
                    noninteractive: eframe::egui::style::WidgetVisuals {
                        weak_bg_fill: self.base(),
                        bg_fill: self.base(),
                        bg_stroke: Stroke::new(2.0, self.overlay2()),
                        fg_stroke: Stroke::new(2.0, self.subtext()),
                        corner_radius: CornerRadius::same(self.radius()),
                        expansion: 0.0,
                    },
                    inactive: eframe::egui::style::WidgetVisuals {
                        weak_bg_fill: self.surface1(),
                        bg_fill: self.surface2(),
                        bg_stroke: Default::default(),
                        fg_stroke: Stroke::new(2.0, self.text()),
                        corner_radius: CornerRadius::same(self.radius()),
                        expansion: 0.0,
                    },
                    hovered: WidgetVisuals {
                        weak_bg_fill: self.overlay1(),
                        bg_fill: self.overlay1(),
                        bg_stroke: Default::default(),
                        fg_stroke: Stroke::new(2.5, self.text()),
                        corner_radius: CornerRadius::same(self.radius()),
                        expansion: 0.0,
                    },
                    active: WidgetVisuals {
                        weak_bg_fill: self.overlay1(),
                        bg_fill: self.accent(),
                        bg_stroke: Default::default(),
                        fg_stroke: Stroke::new(2.0, self.crust()),
                        corner_radius: CornerRadius::same(self.radius()),
                        expansion: 0.0,
                    },
                    open: WidgetVisuals {
                        weak_bg_fill: self.surface2(),
                        bg_fill: self.surface2(),
                        bg_stroke: Default::default(),
                        fg_stroke: Stroke::new(1.0, self.text()),
                        corner_radius: CornerRadius::same(self.radius()),
                        expansion: 0.0,
                    },
                },
                selection: eframe::egui::style::Selection {
                    bg_fill: self.accent(),
                    stroke: Stroke::new(1.0, self.crust()),
                },
                hyperlink_color: self.accent(),
                faint_bg_color: self.mantle(),
                extreme_bg_color: self.crust(),
                code_bg_color: self.crust(),
                window_corner_radius: eframe::egui::CornerRadius::same(self.spacing as u8),
                window_fill: self.base(),
                window_stroke: Default::default(),
                menu_corner_radius: eframe::egui::CornerRadius::same(self.spacing as u8),
                panel_fill: self.crust(),
                ..Default::default()
            },
            // debug: eframe::egui::style::DebugOptions {
            //     debug_on_hover: true,
            //     ..Default::default()
            // },
            explanation_tooltips: true,
            ..Default::default()
        }
    }

    fn text(&self) -> Color32 {
        self.fg_color
    }
    fn subtext(&self) -> Color32 {
        self.bg_color.lerp_to_gamma(self.fg_color, 0.9)
    }
    fn overlay2(&self) -> Color32 {
        self.bg_color.lerp_to_gamma(self.fg_color, 0.7)
    }
    fn overlay1(&self) -> Color32 {
        self.bg_color.lerp_to_gamma(self.fg_color, 0.5)
    }
    fn surface2(&self) -> Color32 {
        self.bg_color.lerp_to_gamma(self.fg_color, 0.2)
    }
    fn surface1(&self) -> Color32 {
        self.bg_color.lerp_to_gamma(self.fg_color, 0.1)
    }
    fn base(&self) -> Color32 {
        self.bg_color
    }
    fn mantle(&self) -> Color32 {
        self.bg_color.lerp_to_gamma(Color32::BLACK, 0.2)
    }
    fn crust(&self) -> Color32 {
        self.bg_color.lerp_to_gamma(Color32::BLACK, 0.3)
    }
    fn accent(&self) -> Color32 {
        self.accent_color
    }
    fn radius(&self) -> u8 {
        self.spacing as u8
    }
    fn rem(&self, value: f32) -> f32 {
        value * self.base_rem
    }
}

/// The App State management struct
#[derive(Debug)]
pub struct CorgiApp {
    ui_state: CorgiUI,
    command_channel: mpsc::Sender<ImageGenCommand>,
    status_channel: mpsc::Receiver<StatusMessage>,
    last_rendered: Image,
    previous_frame: Image,
    last_send_time: Instant,
    last_calc_time: Duration,
    debouncer: Debouncer,
}

impl CorgiApp {
    pub fn create(
        cc: &eframe::CreationContext<'_>,
        cli_options: CorgiCliOptions,
    ) -> std::result::Result<Box<dyn eframe::App>, Box<dyn std::error::Error + Send + Sync>> {
        let wgpu = cc
            .wgpu_render_state
            .as_ref()
            .expect("Eframe must be launched with the wgpu backend");
        let (ui_send, worker_recv) = mpsc::channel::<ImageGenCommand>();
        let (worker_send, ui_recv) = mpsc::channel::<StatusMessage>();
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut initial_image = Image::default();
        let output_image = Image::default();
        let ctx = cc.egui_ctx.clone();
        eframe::egui::Visuals::default();
        let theme = Theme::default();
        ctx.set_style(theme.style());

        if let Some(image_file) = &cli_options.settings_file {
            initial_image = Image::load_from_file(image_file)?
        }

        egui_material_icons::initialize(&cc.egui_ctx);
        ctx.options_mut(|options| {
            options.max_passes = std::num::NonZeroUsize::new(1).unwrap();
        });

        let mut worker_state = WorkerState::new(
            wgpu,
            initial_image.clone(),
            output_image.clone(),
            worker_recv,
            worker_send,
            cancelled,
            ctx,
        );
        let extents = Extent3d::from(&initial_image.viewport);
        let resources = PreviewRenderResources::init(
            &wgpu.device,
            wgpu.target_format,
            worker_state.preview_texture(),
            worker_state.output_texture(),
            (extents.width, extents.height),
            (
                output_image.viewport.width as u32,
                output_image.viewport.height as u32,
            ),
        )?;
        let ui_state = CorgiUI::new(initial_image, ui_send.clone());

        wgpu.renderer.write().callback_resources.insert(resources);
        thread::spawn(move || {
            worker_state.run();
        });

        Ok(Box::new(CorgiApp {
            command_channel: ui_send,
            status_channel: ui_recv,
            debouncer: Debouncer::new(std::time::Duration::from_millis(300)),
            last_rendered: ui_state.image().clone(),
            previous_frame: ui_state.image().clone(),
            last_send_time: Instant::now(),
            last_calc_time: Duration::from_millis(16),
            ui_state,
        }))
    }
}

impl eframe::App for CorgiApp {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        for msg in self.status_channel.try_iter() {
            match msg {
                StatusMessage::Progress(message, progress) => {
                    self.ui_state.status.message = message;
                    self.ui_state.status.progress = Some(progress);
                }
                StatusMessage::NewPreviewViewport(new_calc_time, viewport) => {
                    self.ui_state.status.message = "Finished rendering".into();
                    self.ui_state.status.progress = None;
                    self.ui_state.rendered_explore_viewport = viewport;
                    self.ui_state.swap = true;
                    // use a running average
                    self.last_calc_time = (self.last_calc_time + new_calc_time) / 2;
                    tracing::debug!(
                        "Ready for display in {:?}",
                        Instant::now() - self.last_send_time
                    );
                }
                StatusMessage::NewOutputViewport(calc_time, viewport) => {
                    self.ui_state.status.message = "Finished rendering output".into();
                    self.ui_state.status.progress = None;
                    self.ui_state.rendered_output_viewport = viewport.clone();
                    self.ui_state.output_preview_viewport = viewport;
                    self.ui_state.output_preview_viewport.zoom -= 1.0;
                    self.ui_state.swap = true;
                    tracing::debug!("Finished in {calc_time:?}");
                }
            }
        }
        self.ui_state.generate_ui(ctx);
        let image = self.ui_state.image();
        //  sanity check on image size
        if !(image.viewport.width < 10
            || image.viewport.height < 10
            || image.viewport.width * image.viewport.height > 20_000_000)
        {
            // send the new image to the render thread, but only if
            // - the image is different
            // - the image has not changed for a full frame
            let mouse_down = ctx.input(|is| is.pointer.primary_down());
            if self.ui_state.has_active_viewport() && self.last_rendered != image {
                let diff = image.comp(&self.last_rendered);
                let calc_time = if diff.reprobe || diff.recompute {
                    self.last_calc_time
                } else {
                    // if the image just needs recoloring, we assume it will be fast
                    Duration::from_millis(1)
                };
                let do_send = match calc_time {
                    x if x < Duration::from_millis(30) => true,
                    x if x < Duration::from_millis(500) => {
                        image == self.previous_frame && !mouse_down
                    }
                    _ => {
                        self.debouncer.wait_time = (calc_time / 2).max(Duration::from_millis(500));
                        image == self.previous_frame && !mouse_down && self.debouncer.poll()
                    }
                };
                if do_send {
                    if self
                        .command_channel
                        .send(ImageGenCommand::NewPreviewSettings(image.clone()))
                        .is_ok()
                    {
                        self.last_send_time = Instant::now();
                        self.last_rendered = image.clone();
                        self.debouncer.reset();
                        if calc_time < Duration::from_millis(16) {
                            ctx.request_repaint();
                        }
                    } else {
                        tracing::warn!("Failed to send image update")
                    }
                } else {
                    if self.previous_frame != image {
                        self.debouncer.trigger();
                    }
                    // we need to force a re-check next frame
                    ctx.request_repaint();
                }
            }
            self.previous_frame = image;
        }
    }
}
