//! Posizioni su disco (SPEC §7.4, §11).
//!
//! - `%APPDATA%\MicFX\config.json`: impostazioni.
//! - `%LOCALAPPDATA%\MicFX Data\library\`: indice e audio; `logs\` accanto.
//!
//! - macOS: `~/Library/Application Support/MicFX` (config) e `~/Library/Application Support/MicFX Data`
//!   (libreria, log). Il programma sta in `/Applications/MicFX.app`: aggiornare l'app non tocca i dati.
//!
//! Le cartelle dati sono separate da quella del programma (l'installer per utente usa
//! `%LOCALAPPDATA%\MicFX`) e non hanno il nome dell'identificativo del bundle, che l'uninstaller
//! cancella se l'utente spunta "elimina dati". Così libreria e impostazioni sopravvivono a
//! disinstallazione, reinstallazione e aggiornamenti.

use std::path::{Path, PathBuf};

pub const APP_DIR: &str = "MicFX";
pub const LOCAL_DATA_DIR: &str = "MicFX Data";

#[derive(Debug, Clone)]
pub struct Paths {
    /// `%APPDATA%\MicFX` (macOS: `~/Library/Application Support/MicFX`)
    pub roaming: PathBuf,
    /// `%LOCALAPPDATA%\MicFX Data` (macOS: `~/Library/Application Support/MicFX Data`)
    pub local: PathBuf,
}

impl Paths {
    #[cfg(target_os = "macos")]
    pub fn from_env() -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        let base = home.join("Library").join("Application Support");
        Self { roaming: base.join(APP_DIR), local: base.join(LOCAL_DATA_DIR) }
    }

    #[cfg(not(target_os = "macos"))]
    pub fn from_env() -> Self {
        let roaming = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        let local_base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|| roaming.clone());
        let paths = Self { roaming: roaming.join(APP_DIR), local: local_base.join(LOCAL_DATA_DIR) };
        // Le prime build tenevano la libreria in `%LOCALAPPDATA%\MicFX`, la cartella del programma.
        let legacy = local_base.join(APP_DIR);
        match paths.migrate_library_from(&legacy) {
            Ok(()) => paths,
            Err(e) => {
                // Spostamento non riuscito (file in uso): si continua a usare la vecchia libreria.
                tracing::warn!("libreria non spostata da {}: {e}", legacy.display());
                Self { local: legacy, ..paths }
            }
        }
    }

    /// Radice unica (test).
    pub fn under(root: &Path) -> Self {
        Self { roaming: root.join("roaming"), local: root.join("local") }
    }

    /// Sposta `old_root\library` in questa cartella dati, se c'è e se qui non c'è già una libreria.
    pub fn migrate_library_from(&self, old_root: &Path) -> std::io::Result<()> {
        let old = old_root.join("library");
        if !old.join("index.json").exists() && !old.join("pcm").exists() {
            return Ok(());
        }
        let new = self.library_dir();
        if new.join("index.json").exists() {
            return Ok(());
        }
        let new_pcm = self.pcm_dir();
        if new_pcm.read_dir().is_ok_and(|mut d| d.next().is_some()) {
            // Qui c'è già audio senza indice: non si mescolano due librerie.
            return Ok(());
        }
        std::fs::create_dir_all(&self.local)?;
        let _ = std::fs::remove_dir(&new_pcm);
        let _ = std::fs::remove_dir(&new);
        std::fs::rename(&old, &new)?;
        tracing::info!("libreria spostata in {}", new.display());
        Ok(())
    }

    pub fn config_file(&self) -> PathBuf {
        self.roaming.join("config.json")
    }

    pub fn library_dir(&self) -> PathBuf {
        self.local.join("library")
    }

    /// Microfoni silenziati da "Disattiva la mia voce": se il file c'è all'avvio, MicFX si è chiuso
    /// prima di riattivarli.
    pub fn muted_mics_file(&self) -> PathBuf {
        self.roaming.join("muted-mics.json")
    }

    pub fn index_file(&self) -> PathBuf {
        self.library_dir().join("index.json")
    }

    pub fn pcm_dir(&self) -> PathBuf {
        self.library_dir().join("pcm")
    }

    /// Quarantena: `.sbpcm` che l'indice non conosce. Non si cancellano mai da soli (vedi
    /// `recover_orphans`): un indice "sbagliato" ma valido non deve poter svuotare la libreria.
    pub fn orphans_dir(&self) -> PathBuf {
        self.library_dir().join("orphans")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_legacy_library_once() {
        let root = std::env::temp_dir().join(format!("mfx-paths-{}", uuid::Uuid::new_v4()));
        let legacy = root.join("MicFX");
        std::fs::create_dir_all(legacy.join("library").join("pcm")).unwrap();
        std::fs::write(legacy.join("library").join("index.json"), b"{}").unwrap();
        std::fs::write(legacy.join("library").join("pcm").join("a.sbpcm"), b"x").unwrap();
        std::fs::write(legacy.join("micfx.exe"), b"programma").unwrap();

        let p = Paths { roaming: root.join("roaming"), local: root.join("MicFX Data") };
        p.ensure().unwrap(); // cartelle nuove vuote: non devono bloccare lo spostamento
        p.migrate_library_from(&legacy).unwrap();
        assert!(p.index_file().exists());
        assert!(p.pcm_dir().join("a.sbpcm").exists());
        assert!(!legacy.join("library").exists());
        assert!(legacy.join("micfx.exe").exists(), "il programma resta dov'è");

        // Seconda volta: niente da fare.
        p.migrate_library_from(&legacy).unwrap();
        assert!(p.index_file().exists());
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn never_merges_two_libraries() {
        let root = std::env::temp_dir().join(format!("mfx-paths-{}", uuid::Uuid::new_v4()));
        let legacy = root.join("MicFX");
        std::fs::create_dir_all(legacy.join("library")).unwrap();
        std::fs::write(legacy.join("library").join("index.json"), b"{\"old\":1}").unwrap();
        let p = Paths { roaming: root.join("roaming"), local: root.join("MicFX Data") };
        p.ensure().unwrap();
        std::fs::write(p.index_file(), b"{\"new\":1}").unwrap();
        p.migrate_library_from(&legacy).unwrap();
        assert_eq!(std::fs::read_to_string(p.index_file()).unwrap(), "{\"new\":1}");
        assert!(legacy.join("library").join("index.json").exists());
        std::fs::remove_dir_all(root).ok();
    }
}
