use eframe::egui::Key::*;
use eframe::egui::{KeyboardShortcut, Modifiers};

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
