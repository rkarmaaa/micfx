//! Posizioni su disco (SPEC §7.4, §11).
//!
//! Le cartelle si chiamano `MicFX` e NON coincidono con l'identificativo del bundle: l'uninstaller
//! NSIS di Tauri, se l'utente spunta "elimina dati", cancella solo le cartelle col nome del bundle.
//! Così libreria e impostazioni sopravvivono a disinstallazione, reinstallazione e aggiornamenti.

use std::path::{Path, PathBuf};

pub const APP_DIR: &str = "MicFX";

#[derive(Debug, Clone)]
pub struct Paths {
    /// `%APPDATA%\MicFX`
    pub roaming: PathBuf,
    /// `%LOCALAPPDATA%\MicFX`
    pub local: PathBuf,
}

impl Paths {
    pub fn from_env() -> Self {
        let roaming = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|| roaming.clone());
        Self { roaming: roaming.join(APP_DIR), local: local.join(APP_DIR) }
    }

    /// Radice unica (test).
    pub fn under(root: &Path) -> Self {
        Self { roaming: root.join("roaming"), local: root.join("local") }
    }

    pub fn config_file(&self) -> PathBuf {
        self.roaming.join("config.json")
    }

    pub fn library_dir(&self) -> PathBuf {
        self.local.join("library")
    }

    pub fn index_file(&self) -> PathBuf {
        self.library_dir().join("index.json")
    }

    pub fn pcm_dir(&self) -> PathBuf {
        self.library_dir().join("pcm")
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.local.join("logs")
    }

    pub fn ensure(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.roaming)?;
        std::fs::create_dir_all(self.pcm_dir())?;
        std::fs::create_dir_all(self.logs_dir())?;
        Ok(())
    }
}
