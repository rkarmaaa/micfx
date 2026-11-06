# MicFX

Soundboard per Windows residente in tray: riproduce suoni dentro VB-Cable (li sentono gli amici su
Discord) e in cuffia. La specifica completa è in [docs/SPEC.md](docs/SPEC.md); le scelte che se ne
discostano sono in [docs/DECISIONS.md](docs/DECISIONS.md). Leggile prima di cambiare architettura.

## Comandi

```bash
npm install                 # una volta
npm run dev                 # app in sviluppo (Vite + tauri dev)
npm run build               # installer NSIS in target/release/bundle/nsis
cargo test --workspace      # test Rust; rigenera anche i tipi TS in ui/src/lib/api/types
cargo clippy --workspace --all-targets -- -D warnings
npm run check               # svelte-check della UI
npm run emoji               # rigenera ui/src/lib/emoji/data.json da emojibase-data
```

Debug della WebView reale: avvia con `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9333`
e collega un client CDP; in sviluppo lo store è su `window.__app`.

## Struttura

- `crates/mfx-library`: import (symphonia → rubato → trim → EBU R128 → i16 TPDF), formato `.sbpcm`, `index.json`, scrittura atomica.
- `crates/mfx-audio`: mixer (voci, fade, furto, limiter), bus WASAPI event-driven, `OfflineBus` per i test.
- `crates/mfx-platform`: hotkey (`RegisterHotKey`), parser accelerator, `SendInput` (PTT), dispositivi e notifiche.
- `crates/mfx-core`: thread engine, config, contratto IPC (`api.rs`, esportato con ts-rs), test end-to-end su piattaforma finta.
- `src-tauri`: guscio sottile: tray, finestra effimera, comandi IPC.
- `ui`: Svelte 5. UI volutamente spoglia: il design arriverà dal Figma. La UX (flussi, tastiera, stati, errori) è completa.

Dipendenze a senso unico: `src-tauri → mfx-core → mfx-audio, mfx-library, mfx-platform`.

## Regole real-time (SPEC §6.5)

1. Nel thread audio: nessuna allocazione, nessun `Mutex`, nessun log, nessun I/O, nessun drop che libera memoria.
2. Comunicazione solo via `rtrb` (comandi in entrata, eventi e rifiuti in uscita) e atomici.
3. Thread registrato in MMCSS con `AvSetMmThreadCharacteristicsW("Pro Audio")`.
4. Flush-to-zero e denormals-are-zero attivi sul thread.
5. Contatori atomici: underrun, errori, blocchi renderizzati. L'engine li legge solo quando la UI diagnostica li chiede.
6. `assert_no_alloc` avvolge `mixer.render` nei test (e con la feature `rt-checks`).
7. Profilo release: `opt-level = 3`, `lto = "fat"`, `codegen-units = 1`, `panic = "abort"`.

## Budget (SPEC §3)

A riposo (tray, UI chiusa): CPU 0,00 %, 0 context switch/s dopo 2 min, working set ≤ 20 MB, 0 processi
`msedgewebview2.exe` figli, assente da `powercfg /energy`. Solo eventi, mai polling, mai `timeBeginPeriod`.
Riproduzione: < 0,3 % CPU, 0 underrun, hotkey → primo campione p95 ≤ 40 ms. Misure: `tools/bench/measure.ps1`.

## Dati dell'utente

`%APPDATA%\MicFX\config.json` e `%LOCALAPPDATA%\MicFX\library\` (indice + `pcm\*.sbpcm`). Le cartelle
**non** hanno il nome dell'identificativo del bundle, così l'uninstaller non le cancella: libreria e
impostazioni sopravvivono ad aggiornamenti e reinstallazioni. Non cambiare questi percorsi senza una migrazione.

## Convenzioni

- Testi e commenti in italiano. Codici d'errore stabili in `ErrorCode`; i testi per l'utente li decide la UI (`ui/src/lib/util/messages.ts`).
- Ogni cambio al contratto IPC passa da `crates/mfx-core/src/api.rs` + `cargo test` per rigenerare i tipi TS.
- Gli eventi del core arrivano con le chiavi JSON in ordine alfabetico: nella UI non confrontare oggetti con `JSON.stringify`.
- Svelte 5: un figlio può rileggere le props mentre il suo `{#if}` si chiude; passa props null-safe (`x?.y ?? default`).
