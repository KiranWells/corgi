use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use corgi_lib::types::ImgSpec;
use corgi_lib::types::serde::SafeSaveLoad;
use eframe::egui::Key::*;
use eframe::egui::{self, KeyboardShortcut, Modifiers};

use crate::ui::ActiveFile;

const CTRL_SHIFT: Modifiers = Modifiers {
    ctrl: true,
    shift: true,
    alt: false,
    mac_cmd: false,
    command: false,
};

pub const SAVE: KeyboardShortcut = KeyboardShortcut {
    modifiers: Modifiers::CTRL,
    logical_key: S,
};
pub const SAVE_AS: KeyboardShortcut = KeyboardShortcut {
    modifiers: CTRL_SHIFT,
    logical_key: S,
};
pub const OPEN: KeyboardShortcut = KeyboardShortcut {
    modifiers: Modifiers::CTRL,
    logical_key: O,
};
pub const CLOSE: KeyboardShortcut = KeyboardShortcut {
    modifiers: Modifiers::CTRL,
    logical_key: Q,
};
pub const FORCE_CLOSE: KeyboardShortcut = KeyboardShortcut {
    modifiers: CTRL_SHIFT,
    logical_key: Q,
};

// Note: These must be in order of most specific to least specific;
// if Ctrl-S is before Ctrl-Shift-S, it will consume the shortcut first.
pub const ALL: &[KeyboardShortcut] = &[SAVE_AS, SAVE, OPEN, FORCE_CLOSE, CLOSE];

impl super::CorgiUI {
    pub(super) fn save(&mut self, context: &mut crate::Context) -> bool {
        let path = if let Some(af) = self.active_file.as_ref() {
            af.path.clone()
        } else if let Some(path) = rfd::FileDialog::new()
            .set_directory(context.cache().previous_paths.settings.clone())
            .set_file_name("saved_fractal.corg")
            .add_filter("corg", &["corg"])
            .save_file()
        {
            path
        } else {
            self.status.message = "Save cancelled".into();
            self.status.progress = None;
            return false;
        };
        if let Err(err) = self.root_spec.save(&path) {
            tracing::error!("Failed to save image settings: {err:?}");
            self.status.message = format!("Failed to save image settings: {err}");
            self.status.progress = None;
            return false;
        } else {
            self.status.message = format!("Saved {}", path.as_os_str().to_string_lossy());
            self.status.progress = None;
            if let Some(dir) = path.parent() {
                context.cache_mut().previous_paths.settings = dir.to_owned();
            }
            self.active_file = Some(ActiveFile {
                path,
                last_saved_spec: self.root_spec.clone(),
            });
        }
        true
    }

    pub(super) fn save_as(&mut self, context: &mut crate::Context) {
        let (starting_path, starting_filename) = if let Some(af) = &self.active_file
            && let Some(parent) = af
                .path
                .canonicalize()
                .ok()
                .as_deref()
                .and_then(Path::parent)
                .map(Path::to_path_buf)
            && let Some(file_name) = af.path.file_name().and_then(OsStr::to_str)
        {
            (parent, file_name)
        } else {
            (
                context.cache().previous_paths.settings.clone(),
                "saved_fractal.corg",
            )
        };
        if let Some(path) = rfd::FileDialog::new()
            .set_directory(starting_path)
            .set_file_name(starting_filename)
            .add_filter("corg", &["corg"])
            .save_file()
        {
            if let Err(err) = self.root_spec.save(&path) {
                self.status.message = format!("Failed to save: {err}");
                self.status.progress = None;
            } else {
                self.status.message = format!("Saved {}", path.as_os_str().to_string_lossy());
                self.status.progress = None;
                self.active_file = Some(ActiveFile {
                    path,
                    last_saved_spec: self.root_spec.clone(),
                });
            }
            return;
        }
        self.status.message = "Save as cancelled".into();
        self.status.progress = None;
    }

    pub(super) fn open(&mut self, context: &mut crate::Context) {
        let (starting_path, _starting_filename) = if let Some(af) = &self.active_file
            && let Some(parent) = af
                .path
                .canonicalize()
                .ok()
                .as_deref()
                .and_then(Path::parent)
                .map(Path::to_path_buf)
            && let Some(file_name) = af.path.file_name().and_then(OsStr::to_str)
        {
            (parent, file_name)
        } else {
            (
                context.cache().previous_paths.settings.clone(),
                "saved_fractal.corg",
            )
        };
        if let Some(path) = rfd::FileDialog::new()
            .set_directory(starting_path)
            .add_filter(
                "settings file or image with metadata",
                &["corg", "json", "avif", "jpg", "jpeg", "webp", "png"],
            )
            .pick_file()
        {
            if let Some(dir) = path.parent() {
                context.cache_mut().previous_paths.settings = dir.to_owned();
            }
            match ImgSpec::load(&path) {
                Ok(image) => {
                    self.status.message = format!(
                        "Opened {}",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    );
                    self.status.progress = None;
                    self.root_spec = image.clone();
                    self.active_file = ActiveFile::try_new(image, path);
                }
                Err(err) => {
                    tracing::error!("Failed to load image settings `{path:?}`: {err}");
                    self.status.message = format!("Failed to load image settings: {err:?}");
                    self.status.progress = None;
                }
            }
        } else {
            self.status.message = "Open cancelled".into();
            self.status.progress = None;
        }
    }

    pub(super) fn force_close(&mut self, ctx: &egui::Context) {
        self.active_file = Some(ActiveFile {
            path: PathBuf::default(),
            last_saved_spec: self.root_spec.clone(),
        });
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    pub(super) fn handle_shortcuts(
        &mut self,
        context: &mut crate::Context,
        ctx: &eframe::egui::Context,
    ) {
        // We collect the pressed shortcut here to avoid running the action within input_mut -
        // calling ctx within will lead to a deadlock as it is already locked by input_mut.
        let mut pressed_shortcut = None;
        ctx.input_mut(|input| {
            for shortcut in ALL {
                if input.consume_shortcut(shortcut) {
                    pressed_shortcut = Some(*shortcut);
                    return;
                }
            }
        });
        match pressed_shortcut {
            Some(x) if x == SAVE => {
                self.save(context);
            }
            Some(x) if x == SAVE_AS => {
                self.save_as(context);
            }
            Some(x) if x == OPEN => {
                self.open(context);
            }
            Some(x) if x == CLOSE => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Some(x) if x == FORCE_CLOSE => {
                self.force_close(ctx);
            }
            Some(_) => unreachable!(),
            None => {}
        }
    }
}
