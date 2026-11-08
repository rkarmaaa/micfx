//! Parser delle combinazioni (SPEC §8.1): stringhe come `"Ctrl+Alt+F1"`, `"Numpad5"`, `"F13"`.
//!
//! I nomi dei tasti seguono le posizioni fisiche (come `KeyboardEvent.code`), così un tasto
//! "è" su tastiera italiana resta lo stesso tasto fisico anche cambiando layout. Per lettere, cifre
//! e punteggiatura il virtual key si ricava dallo scancode con il layout attivo al momento della
//! registrazione; F-key, tastierino, navigazione e tasti media hanno VK fissi.
//!
//! Su macOS lo stesso nome si traduce nel key code di posizione (`kVK_*`), anche lui indipendente
//! dal layout. Il modificatore `Win` è il tasto ⌘ (Command): su Mac fa da "Ctrl" delle scorciatoie.

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

    /// Maschera dei modificatori Carbon di `RegisterEventHotKey` (`cmdKey`, `shiftKey`, `optionKey`,
    /// `controlKey`). `win` = ⌘.
    pub fn to_carbon(&self) -> u32 {
        (self.win as u32) << 8 | (self.shift as u32) << 9 | (self.alt as u32) << 11 | (self.ctrl as u32) << 12
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

    /// Key code di macOS (`kVK_*`, posizione fisica). `None` = il tasto non esiste sulle tastiere Mac
    /// o non si può usare come hotkey (Pausa, Stamp, tasti media: macOS li tiene per sé).
    pub fn mac_keycode(&self) -> Option<u16> {
        MAC_KEYCODES.iter().find(|(n, _)| *n == self.name).map(|&(_, k)| k)
    }

    /// Tasto con questo scancode (il PTT in config è salvato come scancode).
    pub fn by_scancode(scancode: u16) -> Option<Self> {
        KEYS.iter().find(|(_, sc, _)| *sc == scancode).map(|&(name, scancode, fixed_vk)| Self {
            name,
            scancode,
            fixed_vk,
        })
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

impl Accelerator {
    /// La combinazione serve anche dentro la finestra di MicFX (F2, Invio, Canc, frecce, testo nei campi,
    /// Ctrl+A/E/F/O/M/1/2/, e la modifica del testo). Una hotkey globale la ruberebbe alla finestra:
    /// finché la finestra è in primo piano va sospesa. `mac`: il modificatore principale è ⌘ (`win`).
    pub fn used_by_window(&self, mac: bool) -> bool {
        let m = self.mods;
        // Tasti da soli (o con Maiusc, come Maiusc+Invio e Maiusc+F10, o le maiuscole scritte nei campi).
        if !m.ctrl && !m.alt && !m.win {
            return !self.key.ok_without_mods();
        }
        let primary = if mac { m.win && !m.ctrl } else { m.ctrl && !m.win };
        primary
            && !m.alt
            && matches!(
                self.key.name,
                "A" | "E" | "F" | "O" | "M" | "1" | "2" | "Comma" | "Enter" | "C" | "V" | "X" | "Z" | "Y"
                    | "Backspace" | "Delete" | "Left" | "Right" | "Up" | "Down" | "Home" | "End"
            )
    }
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

// (nome, kVK_* di macOS). Fonte: HIToolbox/Events.h.
const MAC_KEYCODES: &[(&str, u16)] = &[
    ("A", 0x00),
    ("S", 0x01),
    ("D", 0x02),
    ("F", 0x03),
    ("H", 0x04),
    ("G", 0x05),
    ("Z", 0x06),
    ("X", 0x07),
    ("C", 0x08),
    ("V", 0x09),
    ("IntlBackslash", 0x0A),
    ("B", 0x0B),
    ("Q", 0x0C),
    ("W", 0x0D),
    ("E", 0x0E),
    ("R", 0x0F),
    ("Y", 0x10),
    ("T", 0x11),
    ("1", 0x12),
    ("2", 0x13),
    ("3", 0x14),
    ("4", 0x15),
    ("6", 0x16),
    ("5", 0x17),
    ("Equal", 0x18),
    ("9", 0x19),
    ("7", 0x1A),
    ("Minus", 0x1B),
    ("8", 0x1C),
    ("0", 0x1D),
    ("BracketRight", 0x1E),
    ("O", 0x1F),
    ("U", 0x20),
    ("BracketLeft", 0x21),
    ("I", 0x22),
    ("P", 0x23),
    ("Enter", 0x24),
    ("L", 0x25),
    ("J", 0x26),
    ("Quote", 0x27),
    ("K", 0x28),
    ("Semicolon", 0x29),
    ("Backslash", 0x2A),
    ("Comma", 0x2B),
    ("Slash", 0x2C),
    ("N", 0x2D),
    ("M", 0x2E),
    ("Period", 0x2F),
    ("Tab", 0x30),
    ("Space", 0x31),
    ("Backquote", 0x32),
    ("Backspace", 0x33),
    ("Escape", 0x35),
    ("CapsLock", 0x39),
    ("F17", 0x40),
    ("NumpadDecimal", 0x41),
    ("NumpadMultiply", 0x43),
    ("NumpadAdd", 0x45),
    ("NumpadDivide", 0x4B),
    ("NumpadSubtract", 0x4E),
    ("F18", 0x4F),
    ("F19", 0x50),
    ("Numpad0", 0x52),
    ("Numpad1", 0x53),
    ("Numpad2", 0x54),
    ("Numpad3", 0x55),
    ("Numpad4", 0x56),
    ("Numpad5", 0x57),
    ("Numpad6", 0x58),
    ("Numpad7", 0x59),
    ("F20", 0x5A),
    ("Numpad8", 0x5B),
    ("Numpad9", 0x5C),
    ("F5", 0x60),
    ("F6", 0x61),
    ("F7", 0x62),
    ("F3", 0x63),
    ("F8", 0x64),
    ("F9", 0x65),
    ("F11", 0x67),
    ("F13", 0x69),
    ("F16", 0x6A),
    ("F14", 0x6B),
    ("F10", 0x6D),
    ("F12", 0x6F),
    ("F15", 0x71),
    // Il tasto "Help" dei Mac sta dove sui PC c'è Ins.
    ("Insert", 0x72),
    ("Home", 0x73),
    ("PageUp", 0x74),
    ("Delete", 0x75),
    ("F4", 0x76),
    ("End", 0x77),
    ("F2", 0x78),
    ("PageDown", 0x79),
    ("F1", 0x7A),
    ("Left", 0x7B),
    ("Right", 0x7C),
    ("Down", 0x7D),
    ("Up", 0x7E),
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
    fn window_keys_are_recognised() {
        let w = |s: &str, mac: bool| Accelerator::parse(s).unwrap().used_by_window(mac);
        for s in ["F2", "Enter", "Shift+Enter", "Delete", "Escape", "Space", "A", "Shift+F10", "Ctrl+E", "Ctrl+Comma"] {
            assert!(w(s, false), "{s}");
        }
        for s in ["F13", "Numpad5", "Ctrl+Alt+F2", "Ctrl+Shift+F2", "Ctrl+K", "Alt+A", "Win+E"] {
            assert!(!w(s, false), "{s}");
        }
        assert!(w("Win+E", true) && !w("Ctrl+E", true));
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
    fn mac_keycodes_and_modifiers() {
        let a = Accelerator::parse("Cmd+Shift+K").unwrap();
        assert!(a.mods.win && a.mods.shift);
        assert_eq!(a.key.mac_keycode(), Some(0x28));
        assert_eq!(a.mods.to_carbon(), 0x100 | 0x200);
        assert_eq!(Mods { ctrl: true, alt: true, ..Default::default() }.to_carbon(), 0x1000 | 0x800);
        assert_eq!(Accelerator::parse("Numpad5").unwrap().key.mac_keycode(), Some(0x57));
        assert_eq!(Accelerator::parse("F13").unwrap().key.mac_keycode(), Some(0x69));
        assert_eq!(Accelerator::parse("MediaPlayPause").unwrap().key.mac_keycode(), None);
        // Ogni lettera, cifra e F1–F12 ha un key code, tutti distinti.
        let mut seen = std::collections::HashSet::new();
        for (name, _, _) in KEYS {
            let k = key_by_name(name).unwrap();
            if let Some(code) = k.mac_keycode() {
                assert!(seen.insert(code), "key code doppio per {name}");
            } else {
                let fkey = name.strip_prefix('F').and_then(|n| n.parse::<u32>().ok()).is_some_and(|n| n <= 20);
                assert!(name.len() > 1 && !fkey, "manca {name}");
            }
        }
        assert_eq!(Key::by_scancode(0x1E).unwrap().name, "A");
    }

    #[test]
    fn extended_keys() {
        assert!(Accelerator::parse("Delete").unwrap().key.is_extended());
        assert!(!Accelerator::parse("A").unwrap().key.is_extended());
    }
}
