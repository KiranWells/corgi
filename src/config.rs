use std::fs;
use std::io::Write;
use std::path::PathBuf;

use directories::{ProjectDirs, UserDirs};
use eframe::egui::style::WidgetVisuals;
use eframe::egui::{Color32, CornerRadius, FontId, Stroke, Style, TextStyle, vec2};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    pub max_shader_batch_iters: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PreviousPaths {
    pub settings: PathBuf,
    pub image: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Cache {
    pub previous_paths: PreviousPaths,
    pub default_image_type: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Theme {
    pub bg_color: Color32,
    pub fg_color: Color32,
    pub accent_color: Color32,
    pub spacing: f32,
    pub base_rem: f32,
}

#[derive(Debug)]
pub struct Context {
    config: Config,
    cache: Cache,
    theme: Theme,
    dirty: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_shader_batch_iters: 5000,
        }
    }
}

impl Default for Cache {
    fn default() -> Self {
        let home_dir = UserDirs::new()
            .as_ref()
            .and_then(UserDirs::picture_dir)
            .map(ToOwned::to_owned)
            .unwrap_or_default();
        Self {
            previous_paths: PreviousPaths {
                settings: home_dir.clone(),
                image: home_dir,
            },
            default_image_type: "avif".into(),
        }
    }
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

impl Context {
    pub fn new(config: Config, cache: Cache, theme: Theme) -> Self {
        Self {
            config,
            cache,
            theme,
            dirty: false,
        }
    }
    pub fn config(&self) -> &Config {
        &self.config
    }
    pub fn cache(&self) -> &Cache {
        &self.cache
    }
    pub fn theme(&self) -> &Theme {
        &self.theme
    }
    pub fn config_mut(&mut self) -> &mut Config {
        self.dirty = true;
        &mut self.config
    }
    pub fn cache_mut(&mut self) -> &mut Cache {
        self.dirty = true;
        &mut self.cache
    }
    pub fn theme_mut(&mut self) -> &mut Theme {
        self.dirty = true;
        &mut self.theme
    }

    pub fn save(&mut self) {
        if self.dirty {
            tracing::debug!("Saving settings");
            let Some(proj_dirs) = ProjectDirs::from("com", "kiranwells", "corgi") else {
                tracing::error!("Failed to get project dirs");
                return;
            };
            save_to_toml(&self.config, &proj_dirs.config_dir().join("config.toml"));
            save_to_toml(&self.cache, &proj_dirs.cache_dir().join("cache.toml"));
            save_to_toml(&self.theme, &proj_dirs.config_dir().join("theme.toml"));
            self.dirty = false;
        }
    }
}

fn save_to_toml<T: Serialize + Default>(value: &T, path: &PathBuf) {
    let directory = path.parent().unwrap();
    let err = fs::create_dir_all(directory);
    if !directory.exists() {
        tracing::error!("Failed to create save directory: {directory:?}: {err:?}");
    }
    let serialized = toml::to_string_pretty(&value);
    match serialized {
        Ok(serialized) => {
            match fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(path)
            {
                Ok(mut file) => {
                    if let Err(err) = file.write(serialized.as_bytes()) {
                        tracing::error!("Failed to write data to file: {path:?}: {err}]")
                    }
                }
                Err(err) => tracing::error!("Failed to open {path:?}: {err}"),
            }
        }
        Err(err) => tracing::error!("Failed to serialize value: {err}"),
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
                menu_width: 400.0,
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
                text_alpha_from_coverage: if self.bg_color.lightness() < 0.5 {
                    eframe::epaint::AlphaFromCoverage::DARK_MODE_DEFAULT
                } else {
                    eframe::epaint::AlphaFromCoverage::LIGHT_MODE_DEFAULT
                },
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
        self.bg_color.extreme(0.05)
    }
    fn crust(&self) -> Color32 {
        self.bg_color.extreme(0.075)
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

impl ColorExt for Color32 {
    fn lightness(&self) -> f32 {
        let color = self.to_opaque();
        (color.r() as f32 + color.g() as f32 + color.b() as f32) / 255.0 / 3.0
    }

    fn lighten(&self, inc: f32) -> Self {
        let mut vals = self.to_srgba_unmultiplied();
        let increase = 255.0 * inc;
        for val in vals[0..=2].iter_mut() {
            let offset = if increase > 0.0 {
                increase.max((255.0 - *val as f32) * inc * 2.0)
            } else {
                increase.min(*val as f32 * inc * 2.0)
            };
            *val = (*val as f32 + offset) as u8;
        }
        Color32::from_rgba_unmultiplied(vals[0], vals[1], vals[2], vals[3])
    }
}

trait ColorExt {
    fn lightness(&self) -> f32;
    fn lighten(&self, val: f32) -> Self;
    fn darken(&self, val: f32) -> Self
    where
        Self: Sized,
    {
        self.lighten(-val)
    }
    fn extreme(&self, val: f32) -> Self
    where
        Self: Sized,
    {
        match self.lightness() {
            x if x > 0.5 && (1.0 - x) < val => self.darken(val),
            x if x > 0.5 && (1.0 - x) > val => self.lighten(val),
            x if x < 0.5 && x > val => self.darken(val),
            x if x < 0.5 && x < val => self.lighten(val),
            _ => self.darken(val),
        }
    }
}
