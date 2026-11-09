//! Ricrea i `.sbpcm` mancanti di una libreria dai file originali (`source.path` nell'indice), con lo
//! stesso nome di file: nomi, emoji, cartelle e scorciatoie restano validi.
//!
//! cargo run --release -p mfx-library --example restore_pcm -- <index.json> <cartella di uscita>
//!
//! Non tocca la libreria: scrive solo nella cartella di uscita. Poi si copiano i file in `library\pcm`.

use std::path::{Path, PathBuf};

use mfx_library::LibraryIndex;
use mfx_library::import::{ImportOptions, import_to};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("uso: restore_pcm <index.json> <cartella di uscita>");
        std::process::exit(2);
    }
    let index: LibraryIndex =
        serde_json::from_slice(&std::fs::read(&args[1]).expect("indice illeggibile")).expect("indice non valido");
    let out = PathBuf::from(&args[2]);
    std::fs::create_dir_all(&out).expect("cartella di uscita");
    let opts = ImportOptions::default();
    let (mut ok, mut missing, mut failed, mut changed) = (0, Vec::new(), Vec::new(), Vec::new());
    for s in &index.sounds {
        let src = Path::new(&s.source.path);
        if !src.is_file() {
            missing.push(format!("{}  ←  {}", s.name, s.source.path));
            continue;
        }
        if mfx_library::hash_file(src).ok().as_deref() != Some(s.source.xxh3.as_str()) {
            changed.push(format!("{}  ←  {}", s.name, s.source.path));
        }
        match import_to(src, &out.join(&s.file), &opts) {
            Ok(_) => ok += 1,
            Err(e) => failed.push(format!("{}: {e}", s.name)),
        }
    }
    println!("Suoni nell'indice: {}", index.sounds.len());
    println!("Ricreati: {ok}");
    println!("\nOriginale non trovato ({}):", missing.len());
    for m in &missing {
        println!("  {m}");
    }
    println!("\nOriginale cambiato dopo l'import, ricreato comunque ({}):", changed.len());
    for m in &changed {
        println!("  {m}");
    }
    println!("\nErrori ({}):", failed.len());
    for m in &failed {
        println!("  {m}");
    }
}
