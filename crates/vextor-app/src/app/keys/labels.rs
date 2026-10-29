//! How keys are written in the UI (spec 14c): `⇧⌘G` on macOS,
//! `Ctrl+Shift+G` on Windows and Linux; and which OS to write them for.

use eframe::egui::{self, Key, KeyboardShortcut, ModifierNames, Modifiers, os::OperatingSystem};

/// `shortcut` as the OS writes it: `⇧⌘G` on macOS, `Ctrl+Shift+G`
/// elsewhere. Every key in a UI string comes from here.
pub(crate) fn label(ctx: &egui::Context, shortcut: KeyboardShortcut) -> String {
    label_for(ctx.os(), shortcut)
}

/// [`label`] for `os`.
pub(crate) fn label_for(os: OperatingSystem, shortcut: KeyboardShortcut) -> String {
    let mac = os.is_mac();
    let names = if mac {
        ModifierNames::SYMBOLS
    } else {
        ModifierNames::NAMES
    };
    let mut s = names.format(&shortcut.modifiers, mac);
    if !s.is_empty() {
        s += names.concat;
    }
    s + key_name(shortcut.logical_key, shortcut.modifiers, mac)
}

/// A key's name in a label: arrows and `'` as such on every OS; macOS
/// symbols on macOS, Windows names (`Del`, `PageDown`) elsewhere.
fn key_name(key: Key, modifiers: Modifiers, mac: bool) -> &'static str {
    match key {
        Key::ArrowUp => "↑",
        Key::ArrowDown => "↓",
        Key::ArrowLeft => "←",
        Key::ArrowRight => "→",
        Key::Backspace if mac => "⌫",
        Key::Delete if mac => "⌦",
        Key::Delete => "Del",
        Key::Quote => "'",
        Key::Plus if !mac && modifiers.is_none() => "+",
        _ if mac => key.symbol_or_name(),
        _ => key.name(),
    }
}

/// A modifier held during a gesture, for hints: `⌥` / `Alt`, `⌘` /
/// `Ctrl`, `⇧` / `Shift`.
pub(crate) fn modifier_name(ctx: &egui::Context, modifier: Modifiers) -> String {
    let os = ctx.os();
    let names = if os.is_mac() {
        ModifierNames::SYMBOLS
    } else {
        ModifierNames::NAMES
    };
    names.format(&modifier, os.is_mac())
}

/// The OS named by `VEXTOR_PRETEND_OS` (`windows`, `linux`, `macos`), to
/// try another platform's keys and labels on this one (spec 14c).
pub(crate) fn pretend_os(value: Option<&str>) -> Option<OperatingSystem> {
    match value?.trim().to_ascii_lowercase().as_str() {
        "windows" => Some(OperatingSystem::Windows),
        "linux" => Some(OperatingSystem::Nix),
        "mac" | "macos" => Some(OperatingSystem::Mac),
        _ => None,
    }
}
