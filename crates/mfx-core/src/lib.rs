//! Core di MicFX: thread engine, stato, config, comandi ed eventi (docs/SPEC.md §4, §11, §12).
//!
//! La UI e il guscio Tauri parlano solo con [`CoreHandle`]. Ogni metodo accoda un comando al thread
//! engine e ne attende la risposta: l'engine è l'unico proprietario dello stato e l'unico scrittore.

pub mod api;
pub mod config;
pub mod engine;
pub mod paths;
pub mod platform;

#[cfg(test)]
mod tests;

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use crossbeam_channel::{Sender, bounded, unbounded};
use mfx_audio::event::Event;

pub use api::*;
pub use config::{Settings, WindowRect};
pub use mfx_platform::open_sound_control_panel;
pub use paths::Paths;

use engine::{Command, Engine};
use platform::{Platform, WindowsPlatform};

/// Scancode del tasto PTT premuto (0 = nessuno). Letto dal panic hook per il rilascio garantito.
pub(crate) static PTT_PRESSED: AtomicU32 = AtomicU32::new(0);

/// Rilascia il tasto PTT se l'engine l'ha lasciato premuto. Sicuro da chiamare da un panic hook.
pub fn emergency_release_ptt() {
    let sc = PTT_PRESSED.swap(0, Ordering::SeqCst);
    if sc != 0 {
        mfx_platform::input::send_scancode(sc as u16, false);
    }
}

#[derive(Clone)]
pub struct CoreHandle {
    tx: Sender<Command>,
    waker: Arc<Event>,
    join: Arc<Mutex<Option<JoinHandle<()>>>>,
}

impl CoreHandle {
    /// Avvia l'engine con dispositivi, hotkey e bus WASAPI reali.
    pub fn start(paths: Paths, sink: Arc<dyn EventSink>) -> std::io::Result<Self> {
        Self::start_with(paths, sink, |tx, waker| Box::new(WindowsPlatform::new(tx, waker)))
    }

    /// Avvia l'engine con una piattaforma a scelta (test).
    pub fn start_with(
        paths: Paths,
        sink: Arc<dyn EventSink>,
        make_platform: impl FnOnce(Sender<Command>, Arc<Event>) -> Box<dyn Platform> + Send + 'static,
    ) -> std::io::Result<Self> {
        let (tx, rx) = unbounded::<Command>();
        let waker = Arc::new(Event::new()?);
        let (ready_tx, ready_rx) = bounded::<()>(1);
        let (etx, ewaker) = (tx.clone(), waker.clone());
        let join = std::thread::Builder::new().name("mfx-engine".into()).spawn(move || {
            let _com = mfx_platform::com::ComGuard::mta();
            let platform = make_platform(etx.clone(), ewaker.clone());
            let engine = Engine::new(paths, sink, platform, ewaker, etx);
            let _ = ready_tx.send(());
            engine.run(rx);
        })?;
        ready_rx.recv().map_err(|_| std::io::Error::other("engine non avviato"))?;
        Ok(Self { tx, waker, join: Arc::new(Mutex::new(Some(join))) })
    }

    fn send(&self, cmd: Command) -> bool {
        let ok = self.tx.send(cmd).is_ok();
        self.waker.set();
        ok
    }

    fn call<R: Send + 'static>(&self, f: impl FnOnce(&mut Engine) -> R + Send + 'static) -> ApiResult<R> {
        let (rtx, rrx) = bounded(1);
        let sent = self.send(Command::Call(Box::new(move |e| {
            let _ = rtx.send(f(e));
        })));
        if !sent {
            return Err(ApiError::new(ErrorCode::Internal, "engine non attivo"));
        }
        rrx.recv().map_err(|_| ApiError::new(ErrorCode::Internal, "engine non attivo"))
    }

    fn call_r<R: Send + 'static>(&self, f: impl FnOnce(&mut Engine) -> ApiResult<R> + Send + 'static) -> ApiResult<R> {
        self.call(f)?
    }

    // ---------------- comandi della UI (SPEC §12)

    pub fn get_snapshot(&self) -> ApiResult<Snapshot> {
        self.call(|e| e.snapshot())
    }

    /// `None` se la modalità "play/stop" ha fermato il suono invece di avviarlo.
    pub fn play_sound(&self, sound_id: String) -> ApiResult<Option<u64>> {
        self.call_r(move |e| e.play(&sound_id, false))
    }

    pub fn preview_sound(&self, sound_id: String) -> ApiResult<Option<u64>> {
        self.call_r(move |e| e.play(&sound_id, true))
    }

    pub fn stop_sound(&self, sound_id: String) -> ApiResult<()> {
        self.call(move |e| e.stop_sound(&sound_id))
    }

    pub fn stop_all(&self) -> ApiResult<()> {
        self.call(|e| e.stop_all())
    }

    pub fn import_files(&self, paths: Vec<String>, folder_id: Option<String>) -> ApiResult<u64> {
        self.call_r(move |e| e.import_files(paths, folder_id))
    }

    pub fn update_sound(&self, sound_id: String, patch: SoundPatch) -> ApiResult<Sound> {
        self.call_r(move |e| e.update_sound(&sound_id, patch))
    }

    pub fn delete_sounds(&self, sound_ids: Vec<String>) -> ApiResult<()> {
        self.call_r(move |e| e.delete_sounds(&sound_ids))
    }

    pub fn restore_sounds(&self, sound_ids: Vec<String>) -> ApiResult<Vec<Sound>> {
        self.call_r(move |e| e.restore_sounds(&sound_ids))
    }

    pub fn upsert_folder(&self, id: Option<String>, name: String, order: Option<i32>) -> ApiResult<Folder> {
        self.call_r(move |e| e.upsert_folder(id, name, order))
    }

    pub fn delete_folder(&self, folder_id: String) -> ApiResult<()> {
        self.call_r(move |e| e.delete_folder(&folder_id))
    }

    pub fn set_hotkey(&self, action: HotkeyAction, accelerator: Option<String>) -> ApiResult<Option<Hotkey>> {
        self.call_r(move |e| e.set_hotkey(action, accelerator))
    }

    pub fn pause_hotkeys(&self, paused: bool) -> ApiResult<()> {
        self.call(move |e| e.pause_hotkeys(paused))
    }

    pub fn hotkeys_paused(&self) -> ApiResult<bool> {
        self.call(|e| e.hotkeys_paused())
    }

    pub fn set_output(
        &self,
        bus: BusKind,
        device_id: Option<Option<String>>,
        volume_db: Option<f32>,
        enabled: Option<bool>,
    ) -> ApiResult<Vec<StatusCode>> {
        self.call_r(move |e| e.set_output(bus, device_id, volume_db, enabled))
    }

    pub fn update_settings(&self, patch: serde_json::Value) -> ApiResult<Settings> {
        self.call_r(move |e| e.update_settings(patch))
    }

    pub fn get_settings(&self) -> ApiResult<Settings> {
        self.call(|e| e.settings())
    }

    pub fn get_diagnostics(&self) -> ApiResult<Diagnostics> {
        self.call(|e| e.diagnostics())
    }

    // ---------------- ciclo di vita della finestra (SPEC §10.2)

    pub fn ui_opened(&self) {
        let _ = self.call(|e| e.ui_opened());
    }

    /// La finestra è tornata in primo piano: rilegge le impostazioni di Discord (risposta con
    /// `discord:changed` e `status:changed`, solo se qualcosa è cambiato).
    pub fn recheck_setup(&self) -> ApiResult<()> {
        self.call(|e| e.check_discord())
    }

    pub fn ui_hidden(&self) {
        let _ = self.call(|e| e.ui_hidden());
    }

    pub fn ui_closed(&self) {
        let _ = self.call(|e| e.ui_closed());
    }

    pub fn save_window_rect(&self, rect: WindowRect) {
        let _ = self.call(move |e| e.save_window_rect(rect));
    }

    /// Ferma le voci con fade, rilascia il PTT, salva tutto, chiude i bus (SPEC §10.3).
    pub fn shutdown(&self) {
        let (dtx, drx) = bounded(1);
        if self.send(Command::Shutdown(dtx)) {
            let _ = drx.recv_timeout(std::time::Duration::from_secs(5));
        }
        if let Some(j) = self.join.lock().unwrap().take() {
            let _ = j.join();
        }
    }
}
