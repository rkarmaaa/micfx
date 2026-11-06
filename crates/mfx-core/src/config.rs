//! `config.json` (SPEC §11): impostazioni con default, migrazioni in catena e validazione nel core.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::api::{BusKind, HotkeyAction};
use mfx_library::PlayMode;

pub const CONFIG_SCHEMA_VERSION: u32 = 1;

/// Sotto questa soglia il volume è silenzio.
pub const MIN_VOLUME_DB: f32 = -60.0;
pub const MAX_VOLUME_DB: f32 = 12.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct Settings {
    pub schema_version: u32,
    pub outputs: Outputs,
    pub mic: MicSettings,
    pub playback: PlaybackSettings,
    pub loudness: LoudnessSettings,
    pub library: LibrarySettings,
    pub ptt: PttSettings,
    pub hotkeys: Vec<HotkeyBinding>,
    pub ui: UiSettings,
    pub startup: StartupSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: CONFIG_SCHEMA_VERSION,
            outputs: Outputs::default(),
            mic: MicSettings::default(),
            playback: PlaybackSettings::default(),
            loudness: LoudnessSettings::default(),
            library: LibrarySettings::default(),
            ptt: PttSettings::default(),
            hotkeys: Vec::new(),
            ui: UiSettings::default(),
            startup: StartupSettings::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct Outputs {
    /// Uscita verso il microfono virtuale (CABLE Input): quello che sentono gli altri.
    pub cable: OutputSettings,
    /// Uscita verso le tue cuffie.
    pub monitor: OutputSettings,
}

impl Default for Outputs {
    fn default() -> Self {
        Self {
            cable: OutputSettings { device_id: None, volume_db: 0.0, enabled: true },
            monitor: OutputSettings { device_id: None, volume_db: -6.0, enabled: true },
        }
    }
}

impl Outputs {
    pub fn get(&self, bus: BusKind) -> &OutputSettings {
        match bus {
            BusKind::Cable => &self.cable,
            BusKind::Monitor => &self.monitor,
        }
    }

    pub fn get_mut(&mut self, bus: BusKind) -> &mut OutputSettings {
        match bus {
            BusKind::Cable => &mut self.cable,
            BusKind::Monitor => &mut self.monitor,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct OutputSettings {
    /// `null`: cable = rilevamento automatico di VB-Cable; monitor = segui il predefinito.
    pub device_id: Option<String>,
    pub volume_db: f32,
    pub enabled: bool,
}

impl Default for OutputSettings {
    fn default() -> Self {
        Self { device_id: None, volume_db: 0.0, enabled: true }
    }
}

impl OutputSettings {
    pub fn linear_gain(&self) -> f32 {
        if self.volume_db <= MIN_VOLUME_DB + 0.01 { 0.0 } else { mfx_audio::db_to_lin(self.volume_db) }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum MicMode {
    #[default]
    System,
    Bridge,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct MicSettings {
    pub mode: MicMode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct PlaybackSettings {
    pub max_voices: u32,
    pub default_mode: PlayMode,
    pub fade_in_ms: u32,
    pub fade_out_ms: u32,
    pub idle_stop_ms: u32,
    pub warm_hold_ms: u32,
}

impl Default for PlaybackSettings {
    fn default() -> Self {
        Self {
            max_voices: 32,
            default_mode: PlayMode::Overlap,
            fade_in_ms: 2,
            fade_out_ms: 8,
            idle_stop_ms: 2000,
            warm_hold_ms: 60_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct LoudnessSettings {
    pub target_lufs: f32,
    pub ceiling_dbtp: f32,
    pub trim_silence: bool,
}

impl Default for LoudnessSettings {
    fn default() -> Self {
        Self { target_lufs: -18.0, ceiling_dbtp: -1.0, trim_silence: true }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct LibrarySettings {
    pub max_duration_s: u32,
}

impl Default for LibrarySettings {
    fn default() -> Self {
        Self { max_duration_s: 600 }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct PttSettings {
    pub enabled: bool,
    /// Nome del tasto (come negli accelerator), es. `"V"` o `"CapsLock"`.
    pub key: Option<String>,
    /// Scancode inviato con `SendInput`; ricavato da `key`.
    pub scancode: Option<u16>,
    pub tail_ms: u32,
}

impl Default for PttSettings {
    fn default() -> Self {
        Self { enabled: false, key: None, scancode: None, tail_ms: 200 }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct HotkeyBinding {
    pub accelerator: String,
    pub action: HotkeyAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WindowRect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    #[serde(default)]
    pub maximized: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct UiSettings {
    /// Secondi in cui la finestra chiusa resta in memoria per riaprirsi all'istante (0–300).
    pub keepalive_s: u32,
    pub window: Option<WindowRect>,
    /// Preferenze libere della UI (vista, ordinamento, emoji recenti...). Il core non le interpreta.
    #[ts(type = "Record<string, unknown>")]
    pub prefs: serde_json::Value,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self { keepalive_s: 30, window: None, prefs: serde_json::Value::Object(Default::default()) }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct StartupSettings {
    pub autostart: bool,
}

impl Settings {
    /// Porta ogni valore nel suo intervallo valido. Ritorna `true` se ha corretto qualcosa.
    pub fn sanitize(&mut self) -> bool {
        let before = self.clone();
        for o in [&mut self.outputs.cable, &mut self.outputs.monitor] {
            if !o.volume_db.is_finite() {
                o.volume_db = 0.0;
            }
            o.volume_db = o.volume_db.clamp(MIN_VOLUME_DB, MAX_VOLUME_DB);
            if o.device_id.as_deref().is_some_and(|s| s.trim().is_empty()) {
                o.device_id = None;
            }
        }
        self.playback.max_voices = self.playback.max_voices.clamp(1, mfx_audio::MAX_VOICES as u32);
        self.playback.idle_stop_ms = self.playback.idle_stop_ms.clamp(200, 30_000);
        self.playback.warm_hold_ms = self.playback.warm_hold_ms.clamp(1_000, 600_000);
        let l = &mut self.loudness;
        if !l.target_lufs.is_finite() {
            l.target_lufs = -18.0;
        }
        l.target_lufs = l.target_lufs.clamp(-40.0, -5.0);
        if !l.ceiling_dbtp.is_finite() {
            l.ceiling_dbtp = -1.0;
        }
        l.ceiling_dbtp = l.ceiling_dbtp.clamp(-12.0, 0.0);
        self.library.max_duration_s = self.library.max_duration_s.clamp(5, 3600);
        self.ptt.tail_ms = self.ptt.tail_ms.min(2000);
        self.ui.keepalive_s = self.ui.keepalive_s.min(300);
        if !self.ui.prefs.is_object() {
            self.ui.prefs = serde_json::Value::Object(Default::default());
        }
        // Una combinazione per azione, un'azione per combinazione.
        let mut seen_actions = Vec::new();
        let mut seen_accels = Vec::new();
        self.hotkeys.retain(|h| {
            let ok = !seen_actions.contains(&h.action) && !seen_accels.contains(&h.accelerator);
            seen_actions.push(h.action.clone());
            seen_accels.push(h.accelerator.clone());
            ok
        });
        self.schema_version = CONFIG_SCHEMA_VERSION;
        *self != before
    }
}

/// Migrazioni `vN → vN+1` (SPEC §11).
pub fn migrate(mut v: serde_json::Value) -> Result<serde_json::Value, String> {
    let version = v.get("schema_version").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
    if version > CONFIG_SCHEMA_VERSION {
        return Err(format!("schema_version {version} più recente di questa versione di MicFX"));
    }
    if version == 0 && v.is_object() {
        v["schema_version"] = CONFIG_SCHEMA_VERSION.into();
    }
    Ok(v)
}

/// Unione profonda di `patch` in `base` (oggetti ricorsivi, il resto sostituito).
pub fn merge_json(base: &mut serde_json::Value, patch: &serde_json::Value) {
    match (base, patch) {
        (serde_json::Value::Object(b), serde_json::Value::Object(p)) => {
            for (k, v) in p {
                match b.get_mut(k) {
                    Some(slot) if slot.is_object() && v.is_object() => merge_json(slot, v),
                    _ => {
                        b.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        (b, p) => *b = p.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_example_parses() {
        let json = r#"{
          "schema_version": 1,
          "outputs": {
            "cable":   { "device_id": "{0.0.0.00000000}.{x}", "volume_db": 0.0 },
            "monitor": { "device_id": null, "volume_db": -6.0, "enabled": true }
          },
          "mic": { "mode": "system" },
          "playback": { "max_voices": 32, "default_mode": "overlap", "fade_in_ms": 2, "fade_out_ms": 8, "idle_stop_ms": 2000, "warm_hold_ms": 60000 },
          "loudness": { "target_lufs": -18.0, "ceiling_dbtp": -1.0, "trim_silence": true },
          "ptt": { "enabled": false, "scancode": null, "tail_ms": 200 },
          "hotkeys": [
            { "accelerator": "Ctrl+Alt+F1",  "action": { "type": "play", "sound_id": "8f1c" } },
            { "accelerator": "Ctrl+Alt+F12", "action": { "type": "stop_all" } }
          ],
          "ui": { "keepalive_s": 30, "window": { "x": 120, "y": 80, "w": 960, "h": 640 } },
          "startup": { "autostart": false }
        }"#;
        let s: Settings = serde_json::from_value(migrate(serde_json::from_str(json).unwrap()).unwrap()).unwrap();
        assert!(s.outputs.cable.enabled, "campo mancante → default");
        assert_eq!(s.hotkeys.len(), 2);
        assert_eq!(s.ui.window.unwrap().w, 960);
    }

    #[test]
    fn sanitize_clamps_and_dedups() {
        let mut s = Settings::default();
        s.outputs.monitor.volume_db = 99.0;
        s.ui.keepalive_s = 9999;
        s.hotkeys = vec![
            HotkeyBinding { accelerator: "F13".into(), action: HotkeyAction::StopAll },
            HotkeyBinding { accelerator: "F13".into(), action: HotkeyAction::MuteMonitor },
            HotkeyBinding { accelerator: "F14".into(), action: HotkeyAction::StopAll },
        ];
        assert!(s.sanitize());
        assert_eq!(s.outputs.monitor.volume_db, MAX_VOLUME_DB);
        assert_eq!(s.ui.keepalive_s, 300);
        assert_eq!(s.hotkeys.len(), 1);
        assert!(!s.sanitize(), "idempotente");
    }

    #[test]
    fn deep_merge() {
        let mut base = serde_json::to_value(Settings::default()).unwrap();
        merge_json(
            &mut base,
            &serde_json::json!({ "outputs": { "monitor": { "volume_db": -12.0 } }, "ui": { "prefs": { "view": "list" } } }),
        );
        let s: Settings = serde_json::from_value(base).unwrap();
        assert_eq!(s.outputs.monitor.volume_db, -12.0);
        assert!(s.outputs.monitor.enabled);
        assert_eq!(s.outputs.cable.volume_db, 0.0);
        assert_eq!(s.ui.prefs["view"], "list");
    }

    #[test]
    fn silence_at_min_volume() {
        let o = OutputSettings { device_id: None, volume_db: MIN_VOLUME_DB, enabled: true };
        assert_eq!(o.linear_gain(), 0.0);
        let o = OutputSettings { device_id: None, volume_db: 0.0, enabled: true };
        assert!((o.linear_gain() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn rejects_future_schema() {
        assert!(migrate(serde_json::json!({ "schema_version": 42 })).is_err());
    }
}
