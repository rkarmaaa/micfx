//! Scrittura atomica dei JSON (SPEC §11): `.tmp` → flush → `ReplaceFileW` con `.bak`.
//! Lettura: JSON non valido → `.bak` → default, con esito esplicito per il log.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;

pub fn bak_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".bak");
    PathBuf::from(s)
}

fn tmp_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".tmp");
    PathBuf::from(s)
}

/// Scrive `bytes` in `path` senza mai lasciare un file a metà. La versione precedente resta in `.bak`.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = tmp_path(path);
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    if path.exists() { replace_with_backup(path, &tmp, &bak_path(path)) } else { std::fs::rename(&tmp, path) }
}

#[cfg(windows)]
fn replace_with_backup(target: &Path, replacement: &Path, backup: &Path) -> std::io::Result<()> {
    use windows::Win32::Storage::FileSystem::{REPLACEFILE_IGNORE_MERGE_ERRORS, ReplaceFileW};
    use windows::core::HSTRING;
    let t = HSTRING::from(target.as_os_str());
    let r = HSTRING::from(replacement.as_os_str());
    let b = HSTRING::from(backup.as_os_str());
    // SAFETY: stringhe valide per tutta la durata della chiamata.
    let res = unsafe { ReplaceFileW(&t, &r, &b, REPLACEFILE_IGNORE_MERGE_ERRORS, None, None) };
    match res {
        Ok(()) => Ok(()),
        Err(e) => {
            // Ripiego (es. file system senza supporto): copia di backup + rename.
            tracing::warn!("ReplaceFileW fallita ({e}), ripiego su copy+rename");
            let _ = std::fs::copy(target, backup);
            std::fs::rename(replacement, target)
        }
    }
}

#[cfg(not(windows))]
fn replace_with_backup(target: &Path, replacement: &Path, backup: &Path) -> std::io::Result<()> {
    let _ = std::fs::copy(target, backup);
    std::fs::rename(replacement, target)
}

pub enum ReadOutcome<T> {
    /// File principale valido.
    Loaded(T),
    /// Principale illeggibile o non valido, recuperato dal `.bak`.
    Recovered(T),
    /// Nessun file: primo avvio.
    Missing,
    /// Né principale né `.bak` validi: si parte dai default.
    Corrupt,
}

/// Legge un JSON applicando `migrate` al valore grezzo prima della deserializzazione.
pub fn read_json<T: DeserializeOwned>(
    path: &Path,
    migrate: impl Fn(serde_json::Value) -> Result<serde_json::Value, String>,
) -> ReadOutcome<T> {
    let parse = |p: &Path| -> Option<T> {
        let bytes = std::fs::read(p).ok()?;
        let raw: serde_json::Value = match serde_json::from_slice(&bytes) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("{}: JSON non valido: {e}", p.display());
                return None;
            }
        };
        let migrated = match migrate(raw) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("{}: migrazione fallita: {e}", p.display());
                return None;
            }
        };
        match serde_json::from_value(migrated) {
            Ok(v) => Some(v),
            Err(e) => {
                tracing::warn!("{}: schema non valido: {e}", p.display());
                None
            }
        }
    };
    let main_exists = path.exists();
    if main_exists && let Some(v) = parse(path) {
        return ReadOutcome::Loaded(v);
    }
    let bak = bak_path(path);
    if bak.exists()
        && let Some(v) = parse(&bak)
    {
        return ReadOutcome::Recovered(v);
    }
    if main_exists || bak.exists() { ReadOutcome::Corrupt } else { ReadOutcome::Missing }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> PathBuf {
        let d = std::env::temp_dir().join(format!("mfx-persist-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn writes_keep_backup_and_recover() {
        let d = dir();
        let p = d.join("c.json");
        write_atomic(&p, br#"{"a":1}"#).unwrap();
        write_atomic(&p, br#"{"a":2}"#).unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), r#"{"a":2}"#);
        assert_eq!(std::fs::read_to_string(bak_path(&p)).unwrap(), r#"{"a":1}"#);

        std::fs::write(&p, b"{rotto").unwrap();
        match read_json::<serde_json::Value>(&p, Ok) {
            ReadOutcome::Recovered(v) => assert_eq!(v["a"], 1),
            _ => panic!("atteso recupero dal .bak"),
        }
        std::fs::write(bak_path(&p), b"{rotto").unwrap();
        assert!(matches!(read_json::<serde_json::Value>(&p, Ok), ReadOutcome::Corrupt));
        assert!(matches!(read_json::<serde_json::Value>(&d.join("x.json"), Ok), ReadOutcome::Missing));
        std::fs::remove_dir_all(d).ok();
    }
}
