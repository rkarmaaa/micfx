//! Contratto UI ↔ Core (SPEC §12). I tipi qui sono esportati in TypeScript con `ts-rs`
//! (`cargo test` scrive in `ui/src/lib/api/types/`).

use serde::{Deserialize, Deserializer, Serialize};
use ts_rs::TS;

pub use mfx_library::{Folder, PlayMode, Sound, SoundSource};

use crate::config::Settings;

/// Codici d'errore e di stato stabili (SPEC §15.1). I testi per l'utente li decide la UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ErrorCode {
    CableMissing,
    RateMismatch,
    DefaultIsCable,
    MonitorIsCable,
    CableBusy,
    DeviceLost,
    MicNotRouted,
    HotkeyConflict,
    HotkeyInvalid,
    ImportUnsupported,
    ImportDecodeFailed,
    ImportTooLong,
    ImportDuplicate,
    ImportEmpty,
    ImportNotFound,
    DiskFull,
    IoError,
    SoundNotFound,
    FolderNotFound,
    ConfigRecovered,
    LibraryRecovered,
    PttSendFailed,
    OutputsDisabled,
    InvalidArgument,
    Internal,
}

/// Stato di configurazione mostrato dalla UI (SPEC §9). Sottoinsieme di [`ErrorCode`].
pub type StatusCode = ErrorCode;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, thiserror::Error)]
#[error("{code:?}: {message}")]
#[ts(export)]
pub struct ApiError {
    pub code: ErrorCode,
    pub message: String,
}

impl ApiError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub is_default: bool,
    pub is_cable: bool,
    pub mix_rate: u32,
}

/// Microfono (endpoint di cattura) con lo stato di "Ascolta questo dispositivo".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Mic {
    pub id: String,
    pub name: String,
    pub is_default: bool,
    pub listen_enabled: bool,
    /// La voce arriva a CABLE Input (modo Sistema configurato).
    pub routed_to_cable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export)]
pub enum HotkeyAction {
    Play { sound_id: String },
    Preview { sound_id: String },
    StopAll,
    PauseHotkeys,
    MuteMonitor,
    MuteCable,
    OpenWindow,
}

impl HotkeyAction {
    pub fn sound_id(&self) -> Option<&str> {
        match self {
            HotkeyAction::Play { sound_id } | HotkeyAction::Preview { sound_id } => Some(sound_id),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Hotkey {
    pub action: HotkeyAction,
    /// Forma canonica, es. `"Ctrl+Alt+F1"`.
    pub accelerator: String,
    /// Registrata e funzionante adesso.
    pub active: bool,
    /// Un'altra applicazione usa già questa combinazione.
    pub conflict: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Playing {
    #[ts(type = "number")]
    pub play_id: u64,
    pub sound_id: String,
    #[ts(type = "number")]
    pub started_at_ms: u64,
    #[ts(type = "number")]
    pub duration_ms: u64,
    /// Solo in cuffia (anteprima, F03).
    pub preview: bool,
    pub buses: Vec<BusKind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum BusKind {
    Cable,
    Monitor,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Library {
    pub folders: Vec<Folder>,
    pub sounds: Vec<Sound>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AppInfo {
    pub version: String,
    pub data_dir: String,
    pub library_dir: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Snapshot {
    pub settings: Settings,
    pub devices: Vec<Device>,
    pub mics: Vec<Mic>,
    pub library: Library,
    pub hotkeys: Vec<Hotkey>,
    pub hotkeys_paused: bool,
    pub status: Vec<StatusCode>,
    pub playing: Vec<Playing>,
    pub app: AppInfo,
    /// Avvisi una tantum dall'ultimo avvio (es. `config_recovered`).
    pub notices: Vec<Notice>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Notice {
    pub code: ErrorCode,
    pub message: String,
}

// ---------- Patch ----------

fn double_option<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Some(Option::deserialize(d)?))
}

/// Campi assenti = invariati. `mode: null` = segui il predefinito; `folder_id: null` = radice.
/// `ui` viene unito in superficie: le chiavi a `null` vengono rimosse.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SoundPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub user_gain_db: Option<f32>,
    #[serde(default, deserialize_with = "double_option", skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable, as = "Option<PlayMode>")]
    pub mode: Option<Option<PlayMode>>,
    #[serde(default, deserialize_with = "double_option", skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable, as = "Option<String>")]
    pub folder_id: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub tags: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "Record<string, unknown>")]
    pub ui: Option<serde_json::Value>,
}

// ---------- Eventi ----------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PlaybackEnded {
    #[ts(type = "number")]
    pub play_id: u64,
    pub sound_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LibraryChanged {
    pub added: Vec<Sound>,
    pub updated: Vec<Sound>,
    pub removed: Vec<String>,
    pub folders: Vec<Folder>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ImportFileError {
    pub path: String,
    pub code: ErrorCode,
    pub message: String,
    /// Per `import_duplicate`: il suono già presente.
    pub existing_sound_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ImportProgress {
    #[ts(type = "number")]
    pub job_id: u64,
    pub done: u32,
    pub total: u32,
    /// File appena elaborato.
    pub current: String,
    pub errors: Vec<ImportFileError>,
    /// ID dei suoni aggiunti finora da questo job.
    pub added: Vec<String>,
    pub finished: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BusDiagnostics {
    pub bus: BusKind,
    pub state: String,
    pub device: String,
    pub voices: u32,
    #[ts(type = "number")]
    pub underruns: u64,
    #[ts(type = "number")]
    pub errors: u64,
    #[ts(type = "number")]
    pub blocks: u64,
    pub mix_rate: u32,
    pub buffer_frames: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Diagnostics {
    pub version: String,
    pub buses: Vec<BusDiagnostics>,
    pub playing: u32,
    pub mapped_sounds: u32,
    pub sounds: u32,
    pub config_path: String,
    pub library_path: String,
    pub ptt_down: bool,
}

/// Eventi del core. Quelli `Ui*` vanno alla finestra solo se esiste; gli altri al guscio.
#[derive(Debug, Clone)]
pub enum CoreEvent {
    PlaybackStarted(Playing),
    PlaybackEnded(PlaybackEnded),
    LibraryChanged(LibraryChanged),
    ImportProgress(ImportProgress),
    DevicesChanged {
        devices: Vec<Device>,
        mics: Vec<Mic>,
    },
    StatusChanged(Vec<StatusCode>),
    HotkeysChanged {
        hotkeys: Vec<Hotkey>,
        paused: bool,
    },
    SettingsChanged(Box<Settings>),
    Notice(Notice),
    // --- verso il guscio ---
    /// Il keepalive della UI è scaduto: distruggere la finestra (SPEC §10.2).
    UiExpired,
    /// Hotkey "apri finestra".
    OpenWindow,
}

impl CoreEvent {
    /// Nome dell'evento Tauri per la UI, `None` per gli eventi riservati al guscio.
    pub fn ui_name(&self) -> Option<&'static str> {
        Some(match self {
            CoreEvent::PlaybackStarted(_) => "playback:started",
            CoreEvent::PlaybackEnded(_) => "playback:ended",
            CoreEvent::LibraryChanged(_) => "library:changed",
            CoreEvent::ImportProgress(_) => "import:progress",
            CoreEvent::DevicesChanged { .. } => "devices:changed",
            CoreEvent::StatusChanged(_) => "status:changed",
            CoreEvent::HotkeysChanged { .. } => "hotkeys:changed",
            CoreEvent::SettingsChanged(_) => "settings:changed",
            CoreEvent::Notice(_) => "notice",
            CoreEvent::UiExpired | CoreEvent::OpenWindow => return None,
        })
    }

    pub fn ui_payload(&self) -> serde_json::Value {
        use serde_json::json;
        match self {
            CoreEvent::PlaybackStarted(p) => json!(p),
            CoreEvent::PlaybackEnded(p) => json!(p),
            CoreEvent::LibraryChanged(p) => json!(p),
            CoreEvent::ImportProgress(p) => json!(p),
            CoreEvent::DevicesChanged { devices, mics } => json!({ "devices": devices, "mics": mics }),
            CoreEvent::StatusChanged(s) => json!({ "status": s }),
            CoreEvent::HotkeysChanged { hotkeys, paused } => json!({ "hotkeys": hotkeys, "paused": paused }),
            CoreEvent::SettingsChanged(s) => json!(s),
            CoreEvent::Notice(n) => json!(n),
            CoreEvent::UiExpired | CoreEvent::OpenWindow => serde_json::Value::Null,
        }
    }
}

/// Riceve gli eventi dal thread engine. Deve inoltrare e tornare subito.
pub trait EventSink: Send + Sync {
    fn emit(&self, event: CoreEvent);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sound_patch_distinguishes_null_from_missing() {
        let p: SoundPatch = serde_json::from_str(r#"{"mode": null}"#).unwrap();
        assert_eq!(p.mode, Some(None));
        assert_eq!(p.folder_id, None);
        let p: SoundPatch = serde_json::from_str(r#"{"mode": "toggle", "folder_id": "f1"}"#).unwrap();
        assert_eq!(p.mode, Some(Some(PlayMode::Toggle)));
        assert_eq!(p.folder_id, Some(Some("f1".into())));
    }

    #[test]
    fn hotkey_action_json_shape() {
        let a = HotkeyAction::Play { sound_id: "x".into() };
        assert_eq!(serde_json::to_value(&a).unwrap(), serde_json::json!({"type": "play", "sound_id": "x"}));
        let b: HotkeyAction = serde_json::from_str(r#"{"type":"stop_all"}"#).unwrap();
        assert_eq!(b, HotkeyAction::StopAll);
    }
}

/// Patch di un'uscita per `set_output`. `device_id: null` = automatico / segui il predefinito.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct OutputPatch {
    #[serde(default, deserialize_with = "double_option", skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable, as = "Option<String>")]
    pub device_id: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub volume_db: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub enabled: Option<bool>,
}
