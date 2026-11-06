//! `index.json` della libreria (SPEC §11).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const INDEX_SCHEMA_VERSION: u32 = 1;

/// Modalità di riproduzione per suono (SPEC F07).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PlayMode {
    /// Più istanze dello stesso suono insieme.
    #[default]
    Overlap,
    /// Ferma le istanze in corso dello stesso suono e riparte.
    Restart,
    /// Se suona lo ferma, altrimenti lo avvia.
    Toggle,
    /// Ferma tutti gli altri suoni e parte.
    Exclusive,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Folder {
    pub id: String,
    pub name: String,
    pub order: i32,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SoundSource {
    pub path: String,
    pub xxh3: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Sound {
    pub id: String,
    pub name: String,
    /// Nome del file `.sbpcm` nella cartella `pcm`.
    pub file: String,
    pub channels: u16,
    #[ts(type = "number")]
    pub frames: u64,
    pub loudness_gain_db: f32,
    pub user_gain_db: f32,
    /// `null` = segue `settings.playback.default_mode`.
    #[serde(default)]
    pub mode: Option<PlayMode>,
    #[serde(default)]
    pub folder_id: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub source: SoundSource,
    /// Oggetto opaco della UI (emoji, ordine nella griglia, ...). Il core non lo interpreta.
    #[serde(default = "empty_object")]
    #[ts(type = "Record<string, unknown>")]
    pub ui: serde_json::Value,
    pub created_at: String,
}

impl Sound {
    pub fn duration_ms(&self) -> u64 {
        self.frames * 1000 / crate::SAMPLE_RATE as u64
    }
}

fn empty_object() -> serde_json::Value {
    serde_json::Value::Object(Default::default())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LibraryIndex {
    pub schema_version: u32,
    #[serde(default)]
    pub folders: Vec<Folder>,
    #[serde(default)]
    pub sounds: Vec<Sound>,
}

impl Default for LibraryIndex {
    fn default() -> Self {
        Self { schema_version: INDEX_SCHEMA_VERSION, folders: Vec::new(), sounds: Vec::new() }
    }
}

impl LibraryIndex {
    pub fn sound(&self, id: &str) -> Option<&Sound> {
        self.sounds.iter().find(|s| s.id == id)
    }

    pub fn sound_mut(&mut self, id: &str) -> Option<&mut Sound> {
        self.sounds.iter_mut().find(|s| s.id == id)
    }

    pub fn find_by_hash(&self, xxh3: &str) -> Option<&Sound> {
        self.sounds.iter().find(|s| s.source.xxh3 == xxh3)
    }
}

/// Migrazioni `vN → vN+1` in catena (SPEC §11). Oggi esiste solo la v1.
pub fn migrate(mut v: serde_json::Value) -> Result<serde_json::Value, String> {
    let version = v.get("schema_version").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
    if version > INDEX_SCHEMA_VERSION {
        return Err(format!("schema_version {version} più recente di questa versione di MicFX"));
    }
    if version == 0 {
        // v0 (file senza versione): stessa forma della v1.
        v["schema_version"] = INDEX_SCHEMA_VERSION.into();
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_spec_example() {
        let json = r#"{
          "schema_version": 1,
          "folders": [{ "id": "f1", "name": "Meme", "order": 0 }],
          "sounds": [{
            "id": "8f1c", "name": "Airhorn", "file": "8f1c.sbpcm",
            "channels": 2, "frames": 96000,
            "loudness_gain_db": -3.4, "user_gain_db": 0.0,
            "mode": "restart", "folder_id": "f1", "tags": ["meme"],
            "source": { "path": "C:\\a.mp3", "xxh3": "abc" },
            "ui": {},
            "created_at": "2026-11-06T14:00:00Z"
          }]
        }"#;
        let v = migrate(serde_json::from_str(json).unwrap()).unwrap();
        let idx: LibraryIndex = serde_json::from_value(v).unwrap();
        assert_eq!(idx.sounds[0].mode, Some(PlayMode::Restart));
        assert_eq!(idx.sounds[0].duration_ms(), 2000);
        assert_eq!(idx.folders[0].name, "Meme");
    }

    #[test]
    fn migrates_v0_and_rejects_future() {
        let v = migrate(serde_json::json!({ "sounds": [] })).unwrap();
        assert_eq!(v["schema_version"], 1);
        assert!(migrate(serde_json::json!({ "schema_version": 99 })).is_err());
    }
}
