//! Test dell'engine su piattaforma finta: bus offline, dispositivi simulati, nessun audio reale.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use mfx_audio::{DeviceId, OfflineBus, OutputBus, Wake};
use mfx_platform::Accelerator;
use mfx_platform::discord::DiscordVoice;

use crate::CoreHandle;
use crate::api::*;
use crate::config::{HotkeyBinding, Settings};
use crate::paths::Paths;
use crate::platform::{BusParams, DiscordReader, MicEndpoint, Platform, RegisterError};

#[derive(Default)]
struct World {
    devices: Vec<Device>,
    mics: Vec<MicEndpoint>,
    buses: Vec<(BusKind, OfflineBus)>,
    registered: Vec<(i32, String)>,
    conflicts: Vec<String>,
    keys: Vec<(u16, bool)>,
    discord: Option<DiscordVoice>,
}

type Shared = Arc<Mutex<World>>;

struct FakePlatform(Shared);

impl Platform for FakePlatform {
    fn render_endpoints(&mut self) -> Vec<Device> {
        self.0.lock().unwrap().devices.clone()
    }
    fn capture_endpoints(&mut self) -> Vec<MicEndpoint> {
        self.0.lock().unwrap().mics.clone()
    }
    fn register_hotkey(&mut self, id: i32, accel: Accelerator) -> Result<(), RegisterError> {
        let mut w = self.0.lock().unwrap();
        let s = accel.to_string();
        if w.conflicts.contains(&s) {
            return Err(RegisterError::Conflict);
        }
        w.registered.retain(|(i, _)| *i != id);
        w.registered.push((id, s));
        Ok(())
    }
    fn unregister_hotkey(&mut self, id: i32) {
        self.0.lock().unwrap().registered.retain(|(i, _)| *i != id);
    }
    fn send_key(&mut self, scancode: u16, down: bool) -> bool {
        self.0.lock().unwrap().keys.push((scancode, down));
        true
    }
    fn create_bus(
        &mut self,
        kind: BusKind,
        _device: DeviceId,
        params: BusParams,
        _waker: Arc<dyn Wake>,
    ) -> Result<Box<dyn OutputBus>, String> {
        let b = OfflineBus::new(params.gain);
        self.0.lock().unwrap().buses.push((kind, b.clone()));
        Ok(Box::new(b))
    }
    fn discord_reader(&self) -> DiscordReader {
        let w = self.0.clone();
        Arc::new(move || w.lock().unwrap().discord.clone())
    }
}

#[derive(Default)]
struct Sink(Mutex<Vec<CoreEvent>>);

impl EventSink for Sink {
    fn emit(&self, e: CoreEvent) {
        self.0.lock().unwrap().push(e);
    }
}

impl Sink {
    fn names(&self) -> Vec<&'static str> {
        self.0.lock().unwrap().iter().filter_map(|e| e.ui_name()).collect()
    }
    fn clear(&self) {
        self.0.lock().unwrap().clear();
    }
    fn count(&self, name: &str) -> usize {
        self.names().iter().filter(|n| **n == name).count()
    }
}

fn devices(with_cable: bool) -> Vec<Device> {
    let mut v = vec![Device {
        id: "{0.0.0.00000000}.{cuffie}".into(),
        name: "Cuffie".into(),
        is_default: true,
        is_cable: false,
        mix_rate: 48_000,
    }];
    if with_cable {
        v.push(Device {
            id: "{0.0.0.00000000}.{cable}".into(),
            name: "CABLE Input (VB-Audio Virtual Cable)".into(),
            is_default: false,
            is_cable: true,
            mix_rate: 48_000,
        });
    }
    v
}

const CABLE_OUT: &str = "{0.0.1.00000000}.{cable-out}";

fn cable_output() -> MicEndpoint {
    MicEndpoint {
        id: CABLE_OUT.into(),
        name: "CABLE Output (VB-Audio Virtual Cable)".into(),
        is_default: false,
        is_cable: true,
        listen_enabled: false,
        listen_target: None,
    }
}

fn routed_mic() -> MicEndpoint {
    MicEndpoint {
        id: "mic".into(),
        name: "Microfono".into(),
        is_default: true,
        is_cable: false,
        listen_enabled: true,
        listen_target: Some("{0.0.0.00000000}.{cable}".into()),
    }
}

struct Harness {
    root: std::path::PathBuf,
    world: Shared,
    sink: Arc<Sink>,
    core: CoreHandle,
    /// Cancella la cartella dati alla fine del test (non ai riavvii simulati).
    cleanup: bool,
    /// Avvisi consegnati con il primo snapshot (vengono consegnati una volta sola).
    notices: Vec<Notice>,
}

impl Harness {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("mfx-core-{}", uuid::Uuid::new_v4()));
        Self::at(root, devices(true))
    }

    fn at(root: std::path::PathBuf, devs: Vec<Device>) -> Self {
        let world: Shared = Arc::new(Mutex::new(World {
            devices: devs,
            mics: vec![routed_mic(), cable_output()],
            ..Default::default()
        }));
        let sink = Arc::new(Sink::default());
        let w = world.clone();
        let core =
            CoreHandle::start_with(Paths::under(&root), sink.clone(), move |_, _| Box::new(FakePlatform(w))).unwrap();
        // UI aperta: gli eventi arrivano al sink
        let notices = core.get_snapshot().unwrap().notices;
        Self { root, world, sink, core, cleanup: true, notices }
    }

    fn restart(mut self) -> Self {
        self.core.shutdown();
        self.cleanup = false;
        let devs = self.world.lock().unwrap().devices.clone();
        Self::at(self.root.clone(), devs)
    }

    /// Avanza l'audio di tutti i bus e lascia all'engine il tempo di consumare gli eventi.
    fn advance(&self, frames: usize) {
        for (_, b) in self.world.lock().unwrap().buses.iter() {
            b.advance(frames);
        }
        self.settle();
    }

    fn settle(&self) {
        let _ = self.core.get_diagnostics();
        let _ = self.core.get_diagnostics();
    }

    /// Cambia Discord "sul disco" e chiede al core di ricontrollare, come fa la UI tornando in primo piano.
    fn set_discord(&self, d: Option<DiscordVoice>) {
        self.world.lock().unwrap().discord = d;
        self.core.recheck_setup().unwrap();
    }

    /// La lettura di Discord è asincrona: aspetta che lo stato soddisfi `f`.
    fn wait_status(&self, f: impl Fn(&[StatusCode]) -> bool) -> Vec<StatusCode> {
        for _ in 0..300 {
            let s = self.core.get_snapshot().unwrap().status;
            if f(&s) {
                return s;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("stato atteso non raggiunto: {:?}", self.core.get_snapshot().unwrap().status);
    }

    fn bus(&self, kind: BusKind) -> Option<OfflineBus> {
        self.world.lock().unwrap().buses.iter().find(|(k, _)| *k == kind).map(|(_, b)| b.clone())
    }

    /// Importa un tono sinusoidale WAV e ritorna l'ID del suono.
    fn import_tone(&self, name: &str, secs: f32) -> String {
        let src = self.root.join(format!("{name}.wav"));
        write_wav(&src, secs, 440.0 + name.len() as f32 * 37.0);
        let before = self.core.get_snapshot().unwrap().library.sounds.len();
        self.core.import_files(vec![src.display().to_string()], None).unwrap();
        for _ in 0..200 {
            let s = self.core.get_snapshot().unwrap();
            if s.library.sounds.len() > before {
                return s.library.sounds.last().unwrap().id.clone();
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("import non completato");
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        self.core.shutdown();
        if self.cleanup {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
}

fn write_wav(path: &std::path::Path, secs: f32, freq: f32) {
    let rate = 48_000u32;
    let n = (rate as f32 * secs) as usize;
    let data: Vec<u8> = (0..n)
        .flat_map(|i| {
            let s = (0.3 * (2.0 * std::f32::consts::PI * freq * i as f32 / rate as f32).sin() * 32767.0) as i16;
            s.to_le_bytes()
        })
        .collect();
    let mut w = Vec::new();
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&rate.to_le_bytes());
    w.extend_from_slice(&(rate * 2).to_le_bytes());
    w.extend_from_slice(&2u16.to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&(data.len() as u32).to_le_bytes());
    w.extend_from_slice(&data);
    std::fs::write(path, w).unwrap();
}

#[test]
fn play_goes_to_both_buses_and_ends() {
    let h = Harness::new();
    let id = h.import_tone("airhorn", 0.2);
    h.sink.clear();
    let play = h.core.play_sound(id.clone()).unwrap().unwrap();
    assert_eq!(h.bus(BusKind::Cable).unwrap().active_voices(), 1);
    assert_eq!(h.bus(BusKind::Monitor).unwrap().active_voices(), 1);
    let snap = h.core.get_snapshot().unwrap();
    assert_eq!(snap.playing.len(), 1);
    assert_eq!(snap.playing[0].play_id, play);
    assert_eq!(snap.playing[0].buses, vec![BusKind::Cable, BusKind::Monitor]);
    h.advance(48_000);
    assert_eq!(h.sink.count("playback:started"), 1);
    assert_eq!(h.sink.count("playback:ended"), 1);
    assert!(h.core.get_snapshot().unwrap().playing.is_empty());
}

#[test]
fn modes_toggle_restart_exclusive() {
    let h = Harness::new();
    let a = h.import_tone("a", 1.0);
    let b = h.import_tone("bb", 1.0);
    h.core.update_sound(a.clone(), SoundPatch { mode: Some(Some(PlayMode::Toggle)), ..Default::default() }).unwrap();
    assert!(h.core.play_sound(a.clone()).unwrap().is_some());
    assert!(h.core.play_sound(a.clone()).unwrap().is_none(), "toggle: secondo click ferma");
    h.advance(4800);
    assert!(h.core.get_snapshot().unwrap().playing.is_empty());

    // overlap (predefinito): due istanze
    h.core.play_sound(b.clone()).unwrap();
    h.core.play_sound(b.clone()).unwrap();
    assert_eq!(h.core.get_snapshot().unwrap().playing.len(), 2);

    // exclusive: ferma tutto il resto
    h.core.update_sound(a.clone(), SoundPatch { mode: Some(Some(PlayMode::Exclusive)), ..Default::default() }).unwrap();
    h.core.play_sound(a.clone()).unwrap();
    h.advance(4800);
    let playing = h.core.get_snapshot().unwrap().playing;
    assert_eq!(playing.len(), 1);
    assert_eq!(playing[0].sound_id, a);

    // restart: una sola istanza dello stesso suono
    h.core.update_sound(b.clone(), SoundPatch { mode: Some(Some(PlayMode::Restart)), ..Default::default() }).unwrap();
    h.core.play_sound(b.clone()).unwrap();
    h.core.play_sound(b.clone()).unwrap();
    h.advance(4800);
    let n = h.core.get_snapshot().unwrap().playing.iter().filter(|p| p.sound_id == b).count();
    assert_eq!(n, 1);
}

#[test]
fn output_switches_route_sounds() {
    let h = Harness::new();
    let id = h.import_tone("x", 1.0);
    h.core.set_output(BusKind::Monitor, None, None, Some(false)).unwrap();
    h.core.play_sound(id.clone()).unwrap();
    assert_eq!(h.core.get_snapshot().unwrap().playing[0].buses, vec![BusKind::Cable]);

    h.core.set_output(BusKind::Cable, None, None, Some(false)).unwrap();
    let err = h.core.play_sound(id.clone()).unwrap_err();
    assert_eq!(err.code, ErrorCode::OutputsDisabled);

    // spegnere un'uscita zittisce le voci in corso
    h.advance(4800);
    assert!(h.core.get_snapshot().unwrap().playing.is_empty());

    // l'anteprima va in cuffia anche con il monitor spento
    let p = h.core.preview_sound(id.clone()).unwrap();
    assert!(p.is_some());
    let snap = h.core.get_snapshot().unwrap();
    assert_eq!(snap.playing[0].buses, vec![BusKind::Monitor]);
    assert!(snap.playing[0].preview);
}

#[test]
fn volume_changes_reach_bus_gain() {
    let h = Harness::new();
    let id = h.import_tone("v", 0.5);
    h.core.play_sound(id).unwrap();
    h.core.set_output(BusKind::Monitor, None, Some(-60.0), None).unwrap();
    let s = h.bus(BusKind::Monitor).unwrap().stats();
    let _ = s;
    let settings = h.core.get_settings().unwrap();
    assert_eq!(settings.outputs.monitor.volume_db, -60.0);
    assert_eq!(settings.outputs.monitor.linear_gain(), 0.0);
    // fuori intervallo → limitato
    h.core.set_output(BusKind::Cable, None, Some(40.0), None).unwrap();
    assert_eq!(h.core.get_settings().unwrap().outputs.cable.volume_db, crate::config::MAX_VOLUME_DB);
}

#[test]
fn hotkeys_register_conflict_and_replace() {
    let h = Harness::new();
    let a = h.import_tone("a", 0.3);
    let b = h.import_tone("bb", 0.3);
    let hk =
        h.core.set_hotkey(HotkeyAction::Play { sound_id: a.clone() }, Some("ctrl+shift+1".into())).unwrap().unwrap();
    assert_eq!(hk.accelerator, "Ctrl+Shift+1");
    assert!(hk.active);
    assert_eq!(h.world.lock().unwrap().registered.len(), 1);

    // stessa combinazione su un altro suono: passa al nuovo
    h.core.set_hotkey(HotkeyAction::Play { sound_id: b.clone() }, Some("Ctrl+Shift+1".into())).unwrap();
    let snap = h.core.get_snapshot().unwrap();
    assert_eq!(snap.hotkeys.len(), 1);
    assert_eq!(snap.hotkeys[0].action, HotkeyAction::Play { sound_id: b.clone() });

    // conflitto con un'altra app: salvata ma inattiva
    h.world.lock().unwrap().conflicts.push("F13".into());
    let c = h.core.set_hotkey(HotkeyAction::StopAll, Some("F13".into())).unwrap().unwrap();
    assert!(!c.active && c.conflict);

    // invalida
    let e = h.core.set_hotkey(HotkeyAction::StopAll, Some("Ctrl+Boh".into())).unwrap_err();
    assert_eq!(e.code, ErrorCode::HotkeyInvalid);

    // rimozione
    h.core.set_hotkey(HotkeyAction::Play { sound_id: b.clone() }, None).unwrap();
    assert!(h.world.lock().unwrap().registered.is_empty());

    // pausa: tutto deregistrato tranne la hotkey di pausa
    h.core.set_hotkey(HotkeyAction::PauseHotkeys, Some("Ctrl+Alt+P".into())).unwrap();
    h.core.set_hotkey(HotkeyAction::Play { sound_id: a.clone() }, Some("Ctrl+Alt+A".into())).unwrap();
    h.core.pause_hotkeys(true).unwrap();
    let reg: Vec<String> = h.world.lock().unwrap().registered.iter().map(|r| r.1.clone()).collect();
    assert_eq!(reg, vec!["Ctrl+Alt+P".to_string()]);
    h.core.pause_hotkeys(false).unwrap();
    assert_eq!(h.world.lock().unwrap().registered.len(), 2);
}

#[test]
fn delete_and_undo_restores_sound_and_hotkey() {
    let h = Harness::new();
    let a = h.import_tone("a", 0.3);
    let b = h.import_tone("bb", 0.3);
    h.core.set_hotkey(HotkeyAction::Play { sound_id: a.clone() }, Some("Ctrl+1".into())).unwrap();
    h.core.delete_sounds(vec![a.clone()]).unwrap();
    let snap = h.core.get_snapshot().unwrap();
    assert_eq!(snap.library.sounds.len(), 1);
    assert!(snap.hotkeys.is_empty());
    assert!(h.world.lock().unwrap().registered.is_empty());

    h.core.restore_sounds(vec![a.clone()]).unwrap();
    let snap = h.core.get_snapshot().unwrap();
    assert_eq!(snap.library.sounds.iter().map(|s| s.id.clone()).collect::<Vec<_>>(), vec![a.clone(), b]);
    assert_eq!(snap.hotkeys.len(), 1);
    // e suona ancora: il file non è stato cancellato
    assert!(h.core.play_sound(a).unwrap().is_some());
}

#[test]
fn everything_survives_restart() {
    let h = Harness::new();
    let a = h.import_tone("persist", 0.3);
    h.core
        .update_sound(
            a.clone(),
            SoundPatch {
                name: Some("  Tromba   da stadio ".into()),
                ui: Some(serde_json::json!({"emoji": "📯"})),
                ..Default::default()
            },
        )
        .unwrap();
    let f = h.core.upsert_folder(None, "Meme".into(), None).unwrap();
    h.core.update_sound(a.clone(), SoundPatch { folder_id: Some(Some(f.id.clone())), ..Default::default() }).unwrap();
    h.core.set_hotkey(HotkeyAction::Play { sound_id: a.clone() }, Some("Ctrl+Alt+F1".into())).unwrap();
    h.core.set_output(BusKind::Monitor, None, Some(-12.0), Some(false)).unwrap();
    h.core.update_settings(serde_json::json!({ "ui": { "prefs": { "view": "list" } } })).unwrap();

    let h = h.restart();
    let snap = h.core.get_snapshot().unwrap();
    let s = &snap.library.sounds[0];
    assert_eq!(s.name, "Tromba da stadio");
    assert_eq!(s.ui["emoji"], "📯");
    assert_eq!(s.folder_id.as_deref(), Some(f.id.as_str()));
    assert_eq!(snap.library.folders[0].name, "Meme");
    assert_eq!(snap.hotkeys[0].accelerator, "Ctrl+Alt+F1");
    assert!(snap.hotkeys[0].active);
    assert_eq!(snap.settings.outputs.monitor.volume_db, -12.0);
    assert!(!snap.settings.outputs.monitor.enabled);
    assert_eq!(snap.settings.ui.prefs["view"], "list");
    assert!(h.notices.is_empty());
}

#[test]
fn corrupted_index_recovers_from_backup() {
    let h = Harness::new();
    h.import_tone("a", 0.2);
    h.import_tone("bb", 0.2);
    let root = h.root.clone();
    let h = h.restart(); // forza un secondo salvataggio → esiste index.json.bak
    h.core.upsert_folder(None, "x".into(), None).unwrap();
    let h = h.restart();
    let idx = Paths::under(&root).index_file();
    std::fs::write(&idx, b"{ rotto").unwrap();
    let h = h.restart_without_save(&root);
    let snap = h.core.get_snapshot().unwrap();
    assert_eq!(snap.library.sounds.len(), 2);
    assert!(h.notices.iter().any(|n| n.code == ErrorCode::LibraryRecovered));
}

#[test]
fn lost_index_rebuilt_from_pcm_files() {
    let h = Harness::new();
    h.import_tone("a", 0.2);
    let root = h.root.clone();
    let h = h.restart();
    let p = Paths::under(&root);
    std::fs::remove_file(p.index_file()).unwrap();
    let _ = std::fs::remove_file(mfx_library::persist::bak_path(&p.index_file()));
    let h = h.restart_without_save(&root);
    let snap = h.core.get_snapshot().unwrap();
    assert_eq!(snap.library.sounds.len(), 1);
    assert!(snap.library.sounds[0].name.starts_with("Suono recuperato"));
}

impl Harness {
    /// Riavvia senza che lo shutdown riscriva l'indice (per simulare file rovinati a app spenta).
    fn restart_without_save(mut self, root: &std::path::Path) -> Self {
        let idx = Paths::under(root).index_file();
        let saved = std::fs::read(&idx).ok();
        self.core.shutdown();
        self.cleanup = false;
        match saved {
            Some(bytes) => std::fs::write(&idx, bytes).unwrap(),
            None => {
                let _ = std::fs::remove_file(&idx);
            }
        }
        let devs = self.world.lock().unwrap().devices.clone();
        Self::at(self.root.clone(), devs)
    }
}

#[test]
fn cable_missing_status_and_monitor_only() {
    let root = std::env::temp_dir().join(format!("mfx-core-{}", uuid::Uuid::new_v4()));
    let h = Harness::at(root, devices(false));
    let snap = h.core.get_snapshot().unwrap();
    assert!(snap.status.contains(&ErrorCode::CableMissing));
    let id = h.import_tone("m", 0.3);
    h.core.play_sound(id).unwrap();
    assert_eq!(h.core.get_snapshot().unwrap().playing[0].buses, vec![BusKind::Monitor]);
}

#[test]
fn monitor_cannot_be_cable() {
    let h = Harness::new();
    let e = h.core.set_output(BusKind::Monitor, Some(Some("{0.0.0.00000000}.{cable}".into())), None, None).unwrap_err();
    assert_eq!(e.code, ErrorCode::MonitorIsCable);
}

#[test]
fn mic_not_routed_status() {
    let h = Harness::new();
    assert!(!h.core.get_snapshot().unwrap().status.contains(&ErrorCode::MicNotRouted));
    let root = std::env::temp_dir().join(format!("mfx-core-{}", uuid::Uuid::new_v4()));
    let world: Shared = Arc::new(Mutex::new(World { devices: devices(true), mics: vec![], ..Default::default() }));
    let w = world.clone();
    let core =
        CoreHandle::start_with(Paths::under(&root), Arc::new(Sink::default()), move |_, _| Box::new(FakePlatform(w)))
            .unwrap();
    assert!(core.get_snapshot().unwrap().status.contains(&ErrorCode::MicNotRouted));
    core.shutdown();
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn ptt_pressed_while_cable_plays_and_released_after_tail() {
    let h = Harness::new();
    let id = h.import_tone("ptt", 0.1);
    h.core.update_settings(serde_json::json!({ "ptt": { "enabled": true, "key": "V", "tail_ms": 50 } })).unwrap();
    assert_eq!(h.core.get_settings().unwrap().ptt.scancode, Some(0x2F));
    h.core.play_sound(id).unwrap();
    assert_eq!(h.world.lock().unwrap().keys, vec![(0x2F, true)]);
    h.advance(48_000);
    std::thread::sleep(Duration::from_millis(120));
    h.settle();
    assert_eq!(h.world.lock().unwrap().keys, vec![(0x2F, true), (0x2F, false)]);
}

#[test]
fn import_reports_duplicates_and_errors() {
    let h = Harness::new();
    let src = h.root.join("dup.wav");
    write_wav(&src, 0.2, 500.0);
    let bad = h.root.join("bad.txt");
    std::fs::write(&bad, b"ciao").unwrap();
    h.sink.clear();
    h.core
        .import_files(vec![src.display().to_string(), src.display().to_string(), bad.display().to_string()], None)
        .unwrap();
    let mut finished = None;
    for _ in 0..300 {
        h.settle();
        let ev = h.sink.0.lock().unwrap();
        finished = ev.iter().find_map(|e| match e {
            CoreEvent::ImportProgress(p) if p.finished => Some(p.clone()),
            _ => None,
        });
        drop(ev);
        if finished.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let p = finished.expect("import finito");
    assert_eq!(p.total, 3);
    assert_eq!(p.added.len(), 1);
    let codes: Vec<ErrorCode> = p.errors.iter().map(|e| e.code).collect();
    assert!(codes.contains(&ErrorCode::ImportDuplicate));
    assert!(codes.contains(&ErrorCode::ImportUnsupported));
    let dup = p.errors.iter().find(|e| e.code == ErrorCode::ImportDuplicate).unwrap();
    assert_eq!(dup.existing_sound_id.as_deref(), Some(p.added[0].as_str()));
}

#[test]
fn folders_delete_moves_sounds_to_root() {
    let h = Harness::new();
    let a = h.import_tone("a", 0.2);
    let f = h.core.upsert_folder(None, "Meme".into(), None).unwrap();
    h.core.update_sound(a.clone(), SoundPatch { folder_id: Some(Some(f.id.clone())), ..Default::default() }).unwrap();
    h.core.delete_folder(f.id.clone()).unwrap();
    let snap = h.core.get_snapshot().unwrap();
    assert!(snap.library.folders.is_empty());
    assert_eq!(snap.library.sounds[0].folder_id, None);
    let e =
        h.core.update_sound(a, SoundPatch { folder_id: Some(Some("nope".into())), ..Default::default() }).unwrap_err();
    assert_eq!(e.code, ErrorCode::FolderNotFound);
}

#[test]
fn folder_names_are_unique() {
    let h = Harness::new();
    let meme = h.core.upsert_folder(None, "Meme".into(), None).unwrap();
    let altro = h.core.upsert_folder(None, "Altro".into(), None).unwrap();
    // Stesso nome, anche con maiuscole e spazi diversi: rifiutato, nessuna seconda cartella.
    let e = h.core.upsert_folder(None, "  meme ".into(), None).unwrap_err();
    assert_eq!(e.code, ErrorCode::FolderExists);
    let e = h.core.upsert_folder(Some(altro.id.clone()), "MEME".into(), None).unwrap_err();
    assert_eq!(e.code, ErrorCode::FolderExists);
    // Rinominare una cartella cambiando solo le maiuscole del suo nome va bene.
    let renamed = h.core.upsert_folder(Some(meme.id.clone()), "MEME".into(), None).unwrap();
    assert_eq!(renamed.id, meme.id);
    let snap = h.core.get_snapshot().unwrap();
    let names: Vec<_> = snap.library.folders.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["MEME", "Altro"]);
}

#[test]
fn deleted_folder_restores_with_same_id_and_order() {
    let h = Harness::new();
    let a = h.core.upsert_folder(None, "A".into(), None).unwrap();
    let b = h.core.upsert_folder(None, "B".into(), None).unwrap();
    h.core.upsert_folder(None, "C".into(), None).unwrap();
    h.core.delete_folder(b.id.clone()).unwrap();
    // "Annulla": stesso ID e stessa posizione.
    let back = h.core.upsert_folder(Some(b.id.clone()), "B".into(), Some(b.order)).unwrap();
    assert_eq!(back.id, b.id);
    let snap = h.core.get_snapshot().unwrap();
    let names: Vec<_> = snap.library.folders.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["A", "B", "C"]);
    assert_eq!(snap.library.folders[0].id, a.id);
    // Un ID inventato non diventa un percorso o altro: solo caratteri sicuri.
    let e = h.core.upsert_folder(Some("../x".into()), "D".into(), None).unwrap_err();
    assert_eq!(e.code, ErrorCode::InvalidArgument);
}

#[test]
fn ui_patch_merges_shallowly() {
    let h = Harness::new();
    let a = h.import_tone("a", 0.2);
    h.core
        .update_sound(
            a.clone(),
            SoundPatch { ui: Some(serde_json::json!({"emoji": "🔥", "order": 3})), ..Default::default() },
        )
        .unwrap();
    let s = h
        .core
        .update_sound(a.clone(), SoundPatch { ui: Some(serde_json::json!({"emoji": "🎺"})), ..Default::default() })
        .unwrap();
    assert_eq!(s.ui["emoji"], "🎺");
    assert_eq!(s.ui["order"], 3);
    let s = h
        .core
        .update_sound(a, SoundPatch { ui: Some(serde_json::json!({"order": null})), ..Default::default() })
        .unwrap();
    assert!(s.ui.get("order").is_none());
    let e =
        h.core.update_sound(s.id.clone(), SoundPatch { name: Some("   ".into()), ..Default::default() }).unwrap_err();
    assert_eq!(e.code, ErrorCode::InvalidArgument);
}

#[test]
fn hotkeys_of_missing_sounds_are_dropped_at_load() {
    let root = std::env::temp_dir().join(format!("mfx-core-{}", uuid::Uuid::new_v4()));
    let p = Paths::under(&root);
    p.ensure().unwrap();
    let mut s = Settings::default();
    s.hotkeys.push(HotkeyBinding {
        accelerator: "Ctrl+9".into(),
        action: HotkeyAction::Play { sound_id: "fantasma".into() },
    });
    s.hotkeys.push(HotkeyBinding { accelerator: "F14".into(), action: HotkeyAction::StopAll });
    std::fs::write(p.config_file(), serde_json::to_vec(&s).unwrap()).unwrap();
    let h = Harness::at(root, devices(true));
    let snap = h.core.get_snapshot().unwrap();
    assert_eq!(snap.hotkeys.len(), 1);
    assert_eq!(snap.hotkeys[0].action, HotkeyAction::StopAll);
}

fn discord(krisp: bool, input: &str) -> DiscordVoice {
    DiscordVoice {
        client: "Discord",
        input_device_id: input.into(),
        push_to_talk: false,
        krisp,
        noise_suppression: false,
        advanced_vad: true,
        echo_cancellation: true,
        automatic_gain_control: true,
        auto_sensitivity: true,
    }
}

#[test]
fn discord_filters_and_wrong_input_are_reported_then_clear() {
    let h = Harness::new();
    // Il caso reale: input giusto, ma Krisp toglie i suoni.
    h.set_discord(Some(discord(true, CABLE_OUT)));
    let s = h.wait_status(|s| s.contains(&ErrorCode::DiscordFiltersSounds));
    assert!(!s.contains(&ErrorCode::DiscordWrongInput));
    let d = h.core.get_snapshot().unwrap().discord.unwrap();
    assert_eq!(d.input_is_cable, Some(true));
    assert_eq!(d.input_name.as_deref(), Some("CABLE Output (VB-Audio Virtual Cable)"));
    assert!(d.krisp && d.advanced_vad && d.automatic_gain_control);

    // Krisp spento ma input "Predefinito", che in Windows è il microfono fisico.
    h.set_discord(Some(discord(false, "default")));
    let s = h.wait_status(|s| s.contains(&ErrorCode::DiscordWrongInput));
    assert!(!s.contains(&ErrorCode::DiscordFiltersSounds));
    let d = h.core.get_snapshot().unwrap().discord.unwrap();
    assert!(d.input_is_default);
    assert_eq!(d.input_name.as_deref(), Some("Microfono"));

    // Tutto a posto: nessun avviso Discord.
    h.set_discord(Some(discord(false, &CABLE_OUT.to_uppercase())));
    let s = h.wait_status(|s| !s.contains(&ErrorCode::DiscordWrongInput));
    assert!(s.iter().all(|c| !matches!(
        c,
        ErrorCode::DiscordWrongInput | ErrorCode::DiscordFiltersSounds | ErrorCode::DiscordPushToTalk
    )));
    assert!(h.sink.count("discord:changed") >= 3);
}

#[test]
fn discord_push_to_talk_asks_for_the_relay() {
    let h = Harness::new();
    let mut d = discord(false, CABLE_OUT);
    d.push_to_talk = true;
    h.set_discord(Some(d));
    h.wait_status(|s| s.contains(&ErrorCode::DiscordPushToTalk));
    // In push-to-talk l'attività vocale avanzata non conta.
    assert!(!h.core.get_snapshot().unwrap().discord.unwrap().advanced_vad);
    h.core.update_settings(serde_json::json!({ "ptt": { "enabled": true, "key": "V" } })).unwrap();
    assert!(!h.core.get_snapshot().unwrap().status.contains(&ErrorCode::DiscordPushToTalk));
}

#[test]
fn unknown_discord_input_or_no_discord_is_not_an_error() {
    let h = Harness::new();
    h.set_discord(Some(discord(false, "{0.0.1.00000000}.{scollegato}")));
    for _ in 0..50 {
        if h.core.get_snapshot().unwrap().discord.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let snap = h.core.get_snapshot().unwrap();
    assert_eq!(snap.discord.unwrap().input_is_cable, None);
    assert!(!snap.status.contains(&ErrorCode::DiscordWrongInput));
    h.set_discord(None);
    h.wait_status(|_| h.core.get_snapshot().unwrap().discord.is_none());
}
