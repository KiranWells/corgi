use std::io::Write;
use std::mem;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use color_eyre::Result;
use color_eyre::eyre::eyre;
use corgi_lib::types::ImgSpec;
use corgi_lib::types::serde::SafeSaveLoad;
use eframe::egui::{self, RichText, TextEdit};
use egui_material_icons::icons;
use egui_taffy::TuiBuilderLogic;
use little_exif::exif_tag::ExifTag;
use little_exif::metadata::Metadata;
use rand_seeder::rand_core::Rng;
use rand_seeder::{Seeder, SipRng};
use taffy::prelude::*;

use super::utils::scroll;
use crate::ui::utils::{StyleExt, custom_colored_collapse, fancy_header_tui};

const GROUP_NAME_FILE: &str = ".name";
#[derive(Debug)]
pub struct PresetLibrary {
    roots: Vec<PathBuf>,
    groups: Vec<PresetGroup>,
}

#[derive(Debug)]
pub struct PresetGroup {
    root: PathBuf,
    name: String,
    edit_name: String,
    thumbnails: Vec<LazyPresetThumb>,
    editable: bool,
    ui_edit_mode: bool,
}

#[derive(Debug, PartialEq)]
pub enum LazyPresetThumb {
    Unloaded(PathBuf),
    Loaded(Box<PresetThumb>),
    Failed(FailedLoad),
}

#[derive(Debug, PartialEq)]
pub struct FailedLoad {
    path: PathBuf,
    err_msg: String,
    retry_count: u32,
    last_try_time: Instant,
}

#[derive(Debug, PartialEq)]
pub struct PresetThumb {
    pub path: PathBuf,
    pub name: String,
    pub spec: ImgSpec,
}

impl PresetLibrary {
    pub fn new(library_roots: Vec<PathBuf>) -> Self {
        let mut groups = vec![];
        for root in library_roots.iter() {
            let editable =
                std::env::var("CORGI_OVERRIDE_EDITABLE").is_ok() || root == &library_roots[0];
            if !root.is_dir() {
                tracing::warn!("Invalid library root - not a folder: {root:?}");
                continue;
            }
            let dir = match root.read_dir() {
                Ok(dir) => dir,
                Err(err) => {
                    tracing::error!("Failed to read library root: {root:?} | {err}");
                    continue;
                }
            };
            for group_root in dir {
                let group_dirent = match group_root {
                    Ok(dirent) => dirent,
                    Err(err) => {
                        tracing::error!("Failed to read group file: {err}");
                        continue;
                    }
                };
                let next_group = match PresetGroup::try_new(group_dirent.path(), editable) {
                    Ok(group) => group,
                    Err(err) => {
                        tracing::error!("Failed to load group: {:?} | {err}", group_dirent.path());
                        continue;
                    }
                };
                groups.push(next_group);
            }
        }
        groups.sort_by(|a, b| (!a.editable, &a.name).cmp(&(!b.editable, &b.name)));
        Self {
            roots: library_roots,
            groups,
        }
    }

    pub fn save_root(&self) -> &PathBuf {
        &self.roots[0]
    }

    pub fn group_names(&self) -> Vec<String> {
        self.groups
            .iter()
            .filter(|g| g.editable)
            .map(|g| g.name.clone())
            .collect()
    }

    pub fn create_group(&mut self, group_name: &str) -> Result<()> {
        if self.groups.iter().any(|g| g.name == group_name) {
            return Err(eyre!("Group {group_name} already exists"));
        }

        let group_path = Path::new(self.save_root()).join(name_to_safe_path(group_name, true));
        if !group_path.exists() {
            std::fs::create_dir_all(&group_path)?;
        }
        let mut name_file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(group_path.join(GROUP_NAME_FILE))?;
        name_file.write_all(group_name.as_bytes())?;

        self.groups.push(PresetGroup::try_new(group_path, true)?);

        Ok(())
    }

    pub fn create_preset(&mut self, preset_name: &str, preset_group: &str) -> Result<PathBuf> {
        let Some(group) = self.groups.iter_mut().find(|g| g.name == preset_group) else {
            self.create_group(preset_group)?;
            return self.create_preset(preset_name, preset_group);
        };
        if !group.editable {
            return Err(eyre!("Group {preset_group} is read-only"));
        }

        Ok(group.add_preset(preset_name))
    }

    pub fn render_ui(
        &mut self,
        mut callback: impl FnMut(&ImgSpec),
        show_save_preset: &mut bool,
        tui: &mut egui_taffy::Tui,
        uses_location: bool,
    ) {
        let item_spacing = tui.egui_ui().spacing().item_spacing;
        tui.style(
            Style::col()
                .pad2(item_spacing.y * 2.0, 0.0)
                .gap(item_spacing.y * 2.0),
        )
        .add_with_background_color(|tui| {
            fancy_header_tui(tui, RichText::new("Presets").heading());

            scroll(
                tui,
                |tui| {
                    tui.style(Style::col().side(item_spacing.x * 3.0))
                        .mut_style(|s| s.gap = length(item_spacing.y))
                        .add(|tui| {
                            let mut new_groups = vec![];
                            for mut group in mem::take(&mut self.groups).into_iter() {
                                let remove =
                                    group.render_section(tui, &mut callback, uses_location);
                                if !remove {
                                    new_groups.push(group);
                                } else {
                                    if let Err(err) = std::fs::remove_dir_all(group.root) {
                                        tracing::error!("Failed to remove group directory: {err}");
                                    }
                                }
                            }
                            self.groups = new_groups;
                        });
                },
                "lib",
            );
            if tui
                .style(Style::default().side(item_spacing.x * 3.0))
                .ui_add(egui::Button::new("Save New Preset"))
                .clicked()
            {
                // open a save screen with a rendered preview and name option
                *show_save_preset = true;
            }
        });
    }
}

impl PresetGroup {
    pub fn try_new(root: PathBuf, editable: bool) -> Result<Self> {
        let mut thumbnails = vec![];
        let mut group_name = root
            .iter()
            .next_back()
            .map(|x| x.to_string_lossy().into_owned())
            .unwrap_or("Unnamed Group".into());
        if !root.is_dir() {
            return Err(eyre!("Invalid library root - not a folder: {root:?}"));
        }
        let dir = match root.read_dir() {
            Ok(dir) => dir,
            Err(err) => {
                return Err(eyre!("Failed to read library root: {root:?} | {err}"));
            }
        };
        for thumb_maybe in dir {
            let thumb_dirent = match thumb_maybe {
                Ok(dirent) => dirent,
                Err(err) => {
                    tracing::error!("Failed to read thumb file: {err}");
                    continue;
                }
            };
            if thumb_dirent.file_name() == GROUP_NAME_FILE {
                match std::fs::read_to_string(thumb_dirent.path()) {
                    Ok(s) => group_name = s.trim_ascii().to_string(),
                    Err(err) => {
                        tracing::error!("Failed to read group name: {err}");
                    }
                }
                continue;
            }
            thumbnails.push(PresetThumb::lazy(thumb_dirent.path()));
        }
        thumbnails.sort();
        Ok(Self {
            root,
            thumbnails,
            name: group_name,
            edit_name: String::new(),
            editable,
            ui_edit_mode: false,
        })
    }

    fn add_preset(&mut self, preset_name: &str) -> PathBuf {
        let path = self
            .root
            .join(format!("{}.avif", name_to_safe_path(preset_name, false)));
        self.thumbnails.push(PresetThumb::lazy(path.clone()));
        path
    }

    fn render_section(
        &mut self,
        tui: &mut egui_taffy::Tui,
        callback: &mut impl FnMut(&ImgSpec),
        uses_location: bool,
    ) -> bool {
        let item_spacing = tui.egui_ui().spacing().item_spacing;
        let mut remove = false;
        let mut edit_clicked = false;
        let old_text_color = tui.egui_ui().visuals().text_color();
        custom_colored_collapse(
            tui,
            &self.root,
            &mut self.thumbnails,
            |tui, thumbnails| {
                if self.ui_edit_mode {
                    let margin = tui.egui_ui().spacing().button_padding;
                    if tui
                        .style(Style::grow())
                        .mut_style(|s| s.padding = Rect::zero())
                        .ui_add(
                            TextEdit::singleline(&mut self.edit_name)
                                .margin(margin)
                                .text_color(old_text_color),
                        )
                        .lost_focus()
                        && self.edit_name != self.name
                    {
                        self.name = self.edit_name.clone();
                        if let Ok(mut name_file) = std::fs::OpenOptions::new()
                            .create(true)
                            .write(true)
                            .truncate(true)
                            .open(self.root.join(GROUP_NAME_FILE))
                            && let Err(err) = name_file.write_all(self.name.as_bytes())
                        {
                            tracing::error!("Failed to write group name file: {err}");
                        }
                    }
                } else {
                    tui.style(Style::grow())
                        .mut_style(|s| s.padding = Rect::zero())
                        .label(&self.name);
                }
                if self.ui_edit_mode && thumbnails.is_empty() {
                    remove = tui
                        .mut_style(|s| s.padding = Rect::zero())
                        .add(|tui| {
                            tui.ui_add(
                                egui::Button::new(icons::ICON_DELETE)
                                    .fill(tui.egui_ui().style().visuals.error_fg_color),
                            )
                            .on_hover_text("Delete Group")
                        })
                        .clicked();
                    tui.mut_style(|s| s.padding = Rect::length(item_spacing.x))
                        .add_empty();
                }
                if self.editable {
                    edit_clicked = tui
                        .mut_style(|s| s.padding = Rect::zero())
                        .add(|tui| {
                            tui.ui_add(egui::Button::new(if self.ui_edit_mode {
                                icons::ICON_DONE
                            } else {
                                icons::ICON_EDIT
                            }))
                            .on_hover_text("Edit Group")
                        })
                        .clicked()
                } else {
                    tui.enabled_ui(false).ui_add(egui::Button::new(" "));
                }
            },
            |tui, thumbnails| {
                tui.style(taffy::Style {
                    size: taffy::Size {
                        width: percent(1.0),
                        height: auto(),
                    },
                    display: taffy::Display::Grid,
                    grid_template_rows: vec![min_content(); thumbnails.len().div_ceil(2)],
                    grid_template_columns: vec![fr(0.5); 2],
                    padding: Rect::length(item_spacing.y),
                    gap: length(item_spacing.y),
                    ..Default::default()
                })
                .add(|tui| {
                    if thumbnails.is_empty() {
                        tui.mut_style(|s| s.padding = Rect::length(item_spacing.x))
                            .label("No presets.");
                        return;
                    }
                    let mut new_thumbnails = vec![];
                    for mut thumb in mem::take(thumbnails).into_iter() {
                        let (delete, res) =
                            thumb.render_button(tui, self.ui_edit_mode, uses_location);
                        if res.clicked()
                            && let LazyPresetThumb::Loaded(preset_thumb) = &thumb
                        {
                            callback(&preset_thumb.spec);
                        }
                        if !delete {
                            new_thumbnails.push(thumb);
                        } else {
                            if let Err(err) = std::fs::remove_file(thumb.path()) {
                                tracing::error!("Failed to delete preset file: {err}");
                            }
                        }
                    }
                    *thumbnails = new_thumbnails;
                })
            },
        );

        if edit_clicked {
            self.ui_edit_mode = !self.ui_edit_mode;
            if self.ui_edit_mode {
                self.edit_name = self.name.clone();
            }
        }
        if !self.ui_edit_mode {
            self.thumbnails.sort();
        }
        remove
    }
}

impl Ord for LazyPresetThumb {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        match self {
            LazyPresetThumb::Unloaded(path_buf) => match other {
                LazyPresetThumb::Unloaded(o_path_buf) => path_buf.cmp(o_path_buf),
                LazyPresetThumb::Loaded(_) => Ordering::Greater,
                LazyPresetThumb::Failed(_) => Ordering::Less,
            },
            LazyPresetThumb::Loaded(preset_thumb) => match other {
                LazyPresetThumb::Unloaded(_) => Ordering::Less,
                LazyPresetThumb::Loaded(o_preset_thumb) => {
                    preset_thumb.name.cmp(&o_preset_thumb.name)
                }
                LazyPresetThumb::Failed(_) => Ordering::Less,
            },
            LazyPresetThumb::Failed(failed_load) => match other {
                LazyPresetThumb::Unloaded(_) => Ordering::Greater,
                LazyPresetThumb::Loaded(_) => Ordering::Greater,
                LazyPresetThumb::Failed(o_failed_load) => failed_load.path.cmp(&o_failed_load.path),
            },
        }
    }
}

impl PartialOrd for LazyPresetThumb {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Eq for LazyPresetThumb {}

impl LazyPresetThumb {
    pub fn load(&mut self) -> Result<&mut PresetThumb> {
        let (count, path) = match self {
            Self::Loaded(thumb) => {
                return Ok(thumb);
            }
            Self::Failed(fail) => {
                if fail.retry_count == 1 {
                    tracing::warn!("{}", fail.err_msg);
                }
                if fail.should_retry() {
                    (fail.retry_count, mem::take(&mut fail.path))
                } else {
                    return Err(eyre!(fail.err_msg.clone()));
                }
            }
            Self::Unloaded(path) => (0, mem::take(path)),
        };
        *self = match PresetThumb::new(path.clone()) {
            Ok(thumb) => Self::Loaded(Box::new(thumb)),
            Err(err) => Self::Failed(FailedLoad {
                err_msg: format!(
                    "Failed to load preset image '{}': {err}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                ),
                path,
                retry_count: count + 1,
                last_try_time: Instant::now(),
            }),
        };
        self.load()
    }

    pub fn loaded(&self) -> bool {
        match self {
            LazyPresetThumb::Unloaded(_) => false,
            LazyPresetThumb::Failed(_) => false,
            LazyPresetThumb::Loaded(_) => true,
        }
    }

    fn render_button(
        &mut self,
        tui: &mut egui_taffy::Tui,
        editing: bool,
        uses_location: bool,
    ) -> (bool, egui_taffy::TuiInnerResponse<egui::Response>) {
        let item_spacing = tui.egui_ui().spacing().item_spacing;
        let mut delete = false;
        let res = tui.clickable(|tui| {
            let width = tui.egui_ui().available_width();

            tui.ui_add_manual(
                |ui| {
                    let res = ui
                        .scope_builder(
                            egui::UiBuilder::new()
                                .max_rect(egui::Rect::from_min_size(
                                    ui.min_rect().min,
                                    egui::vec2(width, width),
                                ))
                                .layout(egui::Layout::centered_and_justified(
                                    egui::Direction::TopDown,
                                )),
                            |ui| match self {
                                LazyPresetThumb::Unloaded(_path_buf) => {
                                    egui::Frame::new()
                                        .fill(ecolor::Color32::from_black_alpha(128))
                                        .corner_radius(item_spacing.x)
                                        .show(ui, |ui| ui.label("Loading..."))
                                        .response
                                }
                                LazyPresetThumb::Loaded(preset_thumb) => {
                                    preset_thumb.render_ui(ui, editing, uses_location)
                                }
                                LazyPresetThumb::Failed(fail) => {
                                    egui::Frame::new()
                                        .fill(ecolor::Color32::from_black_alpha(128))
                                        .corner_radius(item_spacing.x)
                                        .show(ui, |ui| {
                                            ui.add(egui::Label::new(&fail.err_msg).wrap())
                                        })
                                        .response
                                }
                            },
                        )
                        .response;
                    if editing {
                        ui.scope_builder(
                            egui::UiBuilder::new()
                                .max_rect(res.rect.shrink(item_spacing.x))
                                .layout(egui::Layout::right_to_left(egui::Align::Min)),
                            |ui| {
                                if ui.button(icons::ICON_DELETE).clicked() {
                                    delete = true;
                                }
                            },
                        );
                    }
                    res
                },
                |mut res, _ui| {
                    res.min_size = emath::Vec2::new(0.0, res.min_size.y);
                    res
                },
            )
        });
        if tui.egui_ui().is_rect_visible(res.rect) && !self.loaded() {
            let _ = self.load();
        }

        (delete, res)
    }

    fn path(&self) -> &PathBuf {
        match self {
            LazyPresetThumb::Unloaded(path_buf) => path_buf,
            LazyPresetThumb::Loaded(preset_thumb) => &preset_thumb.path,
            LazyPresetThumb::Failed(failed_load) => &failed_load.path,
        }
    }
}

impl PresetThumb {
    pub fn lazy(path: PathBuf) -> LazyPresetThumb {
        LazyPresetThumb::Unloaded(path)
    }

    pub fn new(path: PathBuf) -> Result<Self> {
        let exif = little_exif::metadata::Metadata::new_from_path(&path)?;
        let name = match exif
            .get_tag(&little_exif::exif_tag::ExifTag::Make(String::new()))
            .next()
        {
            Some(little_exif::exif_tag::ExifTag::Make(name)) => name.clone(),
            _ => path
                .with_extension("")
                .file_name()
                .map_or("Unnamed".into(), |fname| {
                    fname.to_string_lossy().to_string()
                }),
        };
        Ok(Self {
            spec: ImgSpec::load(&path)?,
            path,
            name,
        })
    }

    fn render_ui(
        &mut self,
        ui: &mut egui::Ui,
        editing: bool,
        uses_location: bool,
    ) -> egui::Response {
        let item_spacing = ui.spacing().item_spacing;
        let res = ui.add(
            egui::Image::new(format!(
                "file://{}",
                self.path
                    .canonicalize()
                    .unwrap_or_default()
                    .to_string_lossy()
            ))
            .corner_radius(item_spacing.x),
        );
        if !ui.rect_contains_pointer(res.rect) {
            return res;
        }
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(res.rect)
                .layout(egui::Layout::centered_and_justified(
                    egui::Direction::TopDown,
                )),
            |ui| {
                egui::Frame::new()
                    .fill(ecolor::Color32::from_black_alpha(196))
                    .corner_radius(item_spacing.x)
                    .show(ui, |ui| {
                        if editing {
                            ui.with_layout(
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    ui.add_space(item_spacing.x);
                                    if ui
                                        .add(
                                            TextEdit::singleline(&mut self.name)
                                                .margin(ui.spacing().button_padding)
                                                .desired_width(
                                                    ui.available_width()
                                                        - item_spacing.x * 2.0
                                                        - 2.0,
                                                )
                                                .horizontal_align(egui::Align::Center),
                                        )
                                        .lost_focus()
                                    {
                                        let mut meta = Metadata::new();
                                        match self.spec.stringify() {
                                            Ok(description) => {
                                                meta.set_tag(ExifTag::ImageDescription(description));
                                                // There is no "name" field, so thumbnails use this instead
                                                meta.set_tag(ExifTag::Make(self.name.clone()));
                                                meta.set_tag(ExifTag::Software("Corgi".into()));
                                                if let Err(err) = meta.write_to_file(&self.path) {
                                                    tracing::error!("Failed to save preset name: {err}");
                                                }
                                            },
                                            Err(err) => {
                                                tracing::warn!("Failed to update metadata; cannot serialize image spec: {err}");
                                            }
                                        }
                                    }
                                    ui.add_space(ui.available_width());
                                },
                            );
                        } else {
                            ui.label(&self.name);
                        }
                    })
            },
        );
        if uses_location && self.spec.location.max_iter > 100_000 {
            ui.scope_builder(
                egui::UiBuilder::new()
                    .max_rect(res.rect.shrink(item_spacing.x))
                    .layout(egui::Layout::right_to_left(egui::Align::Min)),
                |ui| {
                    ui.label(icons::ICON_WARNING).on_hover_text(format!(
                        "Location requires {} max steps, this may be slow to render",
                        self.spec.location.max_iter
                    ));
                },
            );
        }
        res
    }
}

impl FailedLoad {
    fn should_retry(&self) -> bool {
        let mut cooldown = Duration::from_millis(30);
        if self.retry_count < 10 {
            cooldown = cooldown.mul_f64(2.0_f64.powi(self.retry_count as i32));
        } else {
            cooldown = Duration::from_secs(30);
        }

        Instant::now() - self.last_try_time > cooldown
    }
}

fn name_to_safe_path(name: &str, seeded: bool) -> String {
    let filtered: String = name
        .replace(' ', "_")
        .chars()
        .filter(|c| ['_', '-'].contains(c) || c.is_ascii_alphanumeric())
        .collect();
    let random_suffix = if seeded {
        let mut rng: SipRng = Seeder::from(name).into_rng();
        nanoid::nanoid!(10, &nanoid::alphabet::SAFE, |size| {
            let mut bytes = vec![0u8; size];
            rng.fill_bytes(&mut bytes[..]);
            bytes
        })
    } else {
        nanoid::nanoid!()
    };
    format!("{filtered}-{random_suffix}")
}
