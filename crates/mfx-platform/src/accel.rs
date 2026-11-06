//! Parser delle combinazioni (SPEC §8.1): stringhe come `"Ctrl+Alt+F1"`, `"Numpad5"`, `"F13"`.
//!
//! I nomi dei tasti seguono le posizioni fisiche (come `KeyboardEvent.code`), così un tasto
//! "è" su tastiera italiana resta lo stesso tasto fisico anche cambiando layout. Per lettere, cifre
//! e punteggiatura il virtual key si ricava dallo scancode con il layout attivo al momento della
//! registrazione; F-key, tastierino, navigazione e tasti media hanno VK fissi.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
}

impl Mods {
    pub fn is_empty(&self) -> bool {
        !(self.ctrl || self.alt || self.shift || self.win)
    }

    /// Flag `MOD_*` di `RegisterHotKey` (senza `MOD_NOREPEAT`).
    pub fn to_win32(&self) -> u32 {
        (self.alt as u32) | (self.ctrl as u32) << 1 | (self.shift as u32) << 2 | (self.win as u32) << 3
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key {
    pub name: &'static str,
    /// Scancode set 1; `0xE0xx` = tasto esteso.
    pub scancode: u16,
    /// VK fisso, oppure `None` se dipende dal layout (si calcola dallo scancode).
    pub fixed_vk: Option<u16>,
}

impl Key {
    pub fn is_extended(&self) -> bool {
        self.scancode & 0xFF00 == 0xE000
    }

    /// Tasti che è sensato usare da soli, senza modificatori (SPEC §8.1).
    pub fn ok_without_mods(&self) -> bool {
        matches!(self.fixed_vk, Some(vk) if (0x7C..=0x87).contains(&vk) || (0x60..=0x6F).contains(&vk) || (0xAD..=0xB3).contains(&vk))
            || self.name == "Pause"
            || self.name == "ScrollLock"
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Accelerator {
    pub mods: Mods,
    pub key: Key,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AccelError {
    #[error("combinazione vuota")]
    Empty,
    #[error("tasto sconosciuto: {0}")]
    UnknownKey(String),
    #[error("manca il tasto principale")]
    OnlyModifiers,
    #[error("più di un tasto principale")]
    MultipleKeys,
}

// (nome, scancode, vk fisso)
const KEYS: &[(&str, u16, Option<u16>)] = &[
    // Lettere (VK dal layout)
    ("Q", 0x10, None),
    ("W", 0x11, None),
    ("E", 0x12, None),
    ("R", 0x13, None),
    ("T", 0x14, None),
    ("Y", 0x15, None),
    ("U", 0x16, None),
    ("I", 0x17, None),
    ("O", 0x18, None),
    ("P", 0x19, None),
    ("A", 0x1E, None),
    ("S", 0x1F, None),
    ("D", 0x20, None),
    ("F", 0x21, None),
    ("G", 0x22, None),
    ("H", 0x23, None),
    ("J", 0x24, None),
    ("K", 0x25, None),
    ("L", 0x26, None),
    ("Z", 0x2C, None),
    ("X", 0x2D, None),
    ("C", 0x2E, None),
    ("V", 0x2F, None),
    ("B", 0x30, None),
    ("N", 0x31, None),
    ("M", 0x32, None),
    // Cifre della riga superiore
    ("1", 0x02, None),
    ("2", 0x03, None),
    ("3", 0x04, None),
    ("4", 0x05, None),
    ("5", 0x06, None),
    ("6", 0x07, None),
    ("7", 0x08, None),
    ("8", 0x09, None),
    ("9", 0x0A, None),
    ("0", 0x0B, None),
    // Punteggiatura (posizione fisica US)
    ("Minus", 0x0C, None),
    ("Equal", 0x0D, None),
    ("BracketLeft", 0x1A, None),
    ("BracketRight", 0x1B, None),
    ("Semicolon", 0x27, None),
    ("Quote", 0x28, None),
    ("Backquote", 0x29, None),
    ("Backslash", 0x2B, None),
    ("Comma", 0x33, None),
    ("Period", 0x34, None),
    ("Slash", 0x35, None),
    ("IntlBackslash", 0x56, None),
    // Controllo
    ("Escape", 0x01, Some(0x1B)),
    ("Backspace", 0x0E, Some(0x08)),
    ("Tab", 0x0F, Some(0x09)),
    ("Enter", 0x1C, Some(0x0D)),
    ("Space", 0x39, Some(0x20)),
    ("CapsLock", 0x3A, Some(0x14)),
    ("Pause", 0xE11D, Some(0x13)),
    ("ScrollLock", 0x46, Some(0x91)),
    ("PrintScreen", 0xE037, Some(0x2C)),
    // Navigazione
    ("Insert", 0xE052, Some(0x2D)),
    ("Delete", 0xE053, Some(0x2E)),
    ("Home", 0xE047, Some(0x24)),
    ("End", 0xE04F, Some(0x23)),
    ("PageUp", 0xE049, Some(0x21)),
    ("PageDown", 0xE051, Some(0x22)),
    ("Up", 0xE048, Some(0x26)),
    ("Down", 0xE050, Some(0x28)),
    ("Left", 0xE04B, Some(0x25)),
    ("Right", 0xE04D, Some(0x27)),
    // Funzione
    ("F1", 0x3B, Some(0x70)),
    ("F2", 0x3C, Some(0x71)),
    ("F3", 0x3D, Some(0x72)),
    ("F4", 0x3E, Some(0x73)),
    ("F5", 0x3F, Some(0x74)),
    ("F6", 0x40, Some(0x75)),
    ("F7", 0x41, Some(0x76)),
    ("F8", 0x42, Some(0x77)),
    ("F9", 0x43, Some(0x78)),
    ("F10", 0x44, Some(0x79)),
    ("F11", 0x57, Some(0x7A)),
    ("F12", 0x58, Some(0x7B)),
    ("F13", 0x64, Some(0x7C)),
    ("F14", 0x65, Some(0x7D)),
    ("F15", 0x66, Some(0x7E)),
    ("F16", 0x67, Some(0x7F)),
    ("F17", 0x68, Some(0x80)),
    ("F18", 0x69, Some(0x81)),
    ("F19", 0x6A, Some(0x82)),
    ("F20", 0x6B, Some(0x83)),
    ("F21", 0x6C, Some(0x84)),
    ("F22", 0x6D, Some(0x85)),
    ("F23", 0x6E, Some(0x86)),
    ("F24", 0x76, Some(0x87)),
    // Tastierino numerico
    ("Numpad0", 0x52, Some(0x60)),
    ("Numpad1", 0x4F, Some(0x61)),
    ("Numpad2", 0x50, Some(0x62)),
    ("Numpad3", 0x51, Some(0x63)),
    ("Numpad4", 0x4B, Some(0x64)),
    ("Numpad5", 0x4C, Some(0x65)),
    ("Numpad6", 0x4D, Some(0x66)),
    ("Numpad7", 0x47, Some(0x67)),
    ("Numpad8", 0x48, Some(0x68)),
    ("Numpad9", 0x49, Some(0x69)),
    ("NumpadMultiply", 0x37, Some(0x6A)),
    ("NumpadAdd", 0x4E, Some(0x6B)),
    ("NumpadSubtract", 0x4A, Some(0x6D)),
    ("NumpadDecimal", 0x53, Some(0x6E)),
    ("NumpadDivide", 0xE035, Some(0x6F)),
    // Media
    ("AudioVolumeMute", 0xE020, Some(0xAD)),
    ("AudioVolumeDown", 0xE02E, Some(0xAE)),
    ("AudioVolumeUp", 0xE030, Some(0xAF)),
    ("MediaTrackNext", 0xE019, Some(0xB0)),
    ("MediaTrackPrevious", 0xE010, Some(0xB1)),
    ("MediaStop", 0xE024, Some(0xB2)),
    ("MediaPlayPause", 0xE022, Some(0xB3)),
];

// Alias accettati in ingresso (stampati sempre col nome canonico).
const ALIASES: &[(&str, &str)] = &[
    ("Esc", "Escape"),
    ("Return", "Enter"),
    ("Del", "Delete"),
    ("Ins", "Insert"),
    ("PgUp", "PageUp"),
    ("PgDn", "PageDown"),
    ("ArrowUp", "Up"),
    ("ArrowDown", "Down"),
    ("ArrowLeft", "Left"),
    ("ArrowRight", "Right"),
    ("NumpadEnter", "Enter"),
    ("Plus", "Equal"),
];

pub fn key_by_name(name: &str) -> Option<Key> {
    let canonical = ALIASES.iter().find(|(a, _)| a.eq_ignore_ascii_case(name)).map(|(_, c)| *c).unwrap_or(name);
    let canonical = canonical.strip_prefix("Key").filter(|s| s.len() == 1).unwrap_or(canonical);
    let canonical = canonical.strip_prefix("Digit").filter(|s| s.len() == 1).unwrap_or(canonical);
    KEYS.iter().find(|(n, _, _)| n.eq_ignore_ascii_case(canonical)).map(|&(name, scancode, fixed_vk)| Key {
        name,
        scancode,
        fixed_vk,
    })
}

impl Accelerator {
    pub fn parse(s: &str) -> Result<Self, AccelError> {
        let s = s.trim();
        if s.is_empty() {
            return Err(AccelError::Empty);
        }
        let mut mods = Mods::default();
        let mut key = None;
        // "Ctrl++" non serve: il tasto + si chiama "Equal" o "NumpadAdd".
        for part in s.split('+').map(str::trim).filter(|p| !p.is_empty()) {
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => mods.ctrl = true,
                "alt" => mods.alt = true,
                "shift" => mods.shift = true,
                "win" | "meta" | "super" | "cmd" => mods.win = true,
                _ => {
                    if key.is_some() {
                        return Err(AccelError::MultipleKeys);
                    }
                    key = Some(key_by_name(part).ok_or_else(|| AccelError::UnknownKey(part.to_string()))?);
                }
            }
        }
        let key = key.ok_or(AccelError::OnlyModifiers)?;
        Ok(Self { mods, key })
    }

    /// Stringa canonica: `Ctrl+Alt+Shift+Win+Tasto`.
    pub fn canonical(&self) -> String {
        self.to_string()
    }

    /// VK per `RegisterHotKey` secondo il layout di tastiera attivo.
    #[cfg(windows)]
    pub fn virtual_key(&self) -> u32 {
        if let Some(vk) = self.key.fixed_vk {
            return vk as u32;
        }
        use windows::Win32::UI::Input::KeyboardAndMouse::{MAPVK_VSC_TO_VK_EX, MapVirtualKeyW};
        // SAFETY: funzione pura su valori interi.
        let vk = unsafe { MapVirtualKeyW(self.key.scancode as u32, MAPVK_VSC_TO_VK_EX) };
        if vk != 0 { vk } else { self.key.scancode as u32 }
    }
}

impl fmt::Display for Accelerator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.mods.ctrl {
            f.write_str("Ctrl+")?;
        }
        if self.mods.alt {
            f.write_str("Alt+")?;
        }
        if self.mods.shift {
            f.write_str("Shift+")?;
        }
        if self.mods.win {
            f.write_str("Win+")?;
        }
        f.write_str(self.key.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_spec_examples() {
        let a = Accelerator::parse("Ctrl+Alt+F1").unwrap();
        assert!(a.mods.ctrl && a.mods.alt && !a.mods.shift);
        assert_eq!(a.key.name, "F1");
        assert_eq!(a.to_string(), "Ctrl+Alt+F1");
        assert_eq!(Accelerator::parse("Numpad5").unwrap().key.fixed_vk, Some(0x65));
        assert!(Accelerator::parse("F13").unwrap().key.ok_without_mods());
    }

    #[test]
    fn normalizes_order_case_and_aliases() {
        assert_eq!(Accelerator::parse("shift + ctrl + a").unwrap().to_string(), "Ctrl+Shift+A");
        assert_eq!(Accelerator::parse("Ctrl+KeyB").unwrap().to_string(), "Ctrl+B");
        assert_eq!(Accelerator::parse("Alt+Digit3").unwrap().to_string(), "Alt+3");
        assert_eq!(Accelerator::parse("Ctrl+ArrowUp").unwrap().to_string(), "Ctrl+Up");
        assert_eq!(Accelerator::parse("Meta+Esc").unwrap().to_string(), "Win+Escape");
    }

    #[test]
    fn rejects_invalid() {
        assert_eq!(Accelerator::parse(""), Err(AccelError::Empty));
        assert_eq!(Accelerator::parse("Ctrl+Alt"), Err(AccelError::OnlyModifiers));
        assert_eq!(Accelerator::parse("Ctrl+A+B"), Err(AccelError::MultipleKeys));
        assert!(matches!(Accelerator::parse("Ctrl+Boh"), Err(AccelError::UnknownKey(_))));
    }

    #[test]
    fn modifier_flags_match_win32() {
        let m = Mods { ctrl: true, alt: true, shift: true, win: true };
        assert_eq!(m.to_win32(), 0x1 | 0x2 | 0x4 | 0x8);
        assert!(!Accelerator::parse("A").unwrap().key.ok_without_mods());
        assert!(Accelerator::parse("MediaPlayPause").unwrap().key.ok_without_mods());
    }

    #[test]
    fn extended_keys() {
        assert!(Accelerator::parse("Delete").unwrap().key.is_extended());
        assert!(!Accelerator::parse("A").unwrap().key.is_extended());
    }
}
