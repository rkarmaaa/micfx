//! Libreria suoni di MicFX (docs/SPEC.md §7).
//!
//! - [`sbpcm`]: formato su disco PCM i16 48 kHz, letto via memory map.
//! - [`import`]: pipeline una tantum (decodifica, resampling, trim, loudness, dither).
//! - [`index`]: `index.json` con suoni e cartelle.
//! - [`persist`]: scrittura atomica con `.bak` e lettura con recupero (SPEC §11).

pub mod import;
pub mod index;
pub mod persist;
pub mod sbpcm;
pub mod time;

pub use import::{ImportError, ImportOptions, ImportedAudio, hash_file, import_to, name_from_path};
pub use index::{Folder, LibraryIndex, PlayMode, Sound, SoundSource};
pub use sbpcm::{SbPcmError, SbPcmFile, SbPcmHeader};

/// Estensioni accettate all'import (SPEC F04).
pub const SUPPORTED_EXTENSIONS: &[&str] = &["mp3", "wav", "flac", "ogg", "oga", "opus", "m4a", "aac", "mp4"];

/// Frequenza di campionamento interna di tutta la libreria.
pub const SAMPLE_RATE: u32 = 48_000;

pub fn is_supported_extension(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| SUPPORTED_EXTENSIONS.iter().any(|s| s.eq_ignore_ascii_case(e)))
        .unwrap_or(false)
}
