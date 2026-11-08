//! Thread engine (SPEC §4): unico proprietario dello stato e unico scrittore dei file.
//!
//! Dorme su un evento Win32 con timeout = prossima scadenza dei timer one-shot (debounce config,
//! coda PTT, keepalive UI, retry dei bus), altrimenti infinito. Chi gli scrive accoda e chiama `set()`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};
use mfx_audio::event::Event;
use mfx_audio::{BusCmd, BusEvent, DeviceId, MixerCmd, OutputBus, SampleSource, SoundRef, Wake};
use mfx_library::persist::{self, ReadOutcome};
use mfx_library::{Folder, ImportError, ImportOptions, LibraryIndex, PlayMode, SbPcmFile, Sound, SoundSource};
use mfx_platform::Accelerator;
use mfx_platform::devices::DeviceEvent;
use mfx_platform::discord::DiscordVoice;

use crate::api::*;
use crate::config::{self, HotkeyBinding, OutputSettings, Settings, WindowRect};
use crate::paths::Paths;
use crate::platform::{BusParams, MicEndpoint, MicRoute, Platform, RegisterError};

const SAVE_DEBOUNCE: Duration = Duration::from_millis(500);
const DEVICE_DEBOUNCE: Duration = Duration::from_millis(150);
const TRASH_HOLD: Duration = Duration::from_secs(20);
const RETRY_BACKOFF: [Duration; 3] = [Duration::from_millis(250), Duration::from_secs(1), Duration::from_secs(5)];
const MAX_IMPORT_FILES: usize = 500;

pub enum Command {
    Call(Box<dyn FnOnce(&mut Engine) + Send>),
    Hotkey(i32),
    Device(DeviceEvent),
    ImportItem { job_id: u64, path: PathBuf, result: Result<NewSound, ImportFileError> },
    ImportDone { job_id: u64 },
    Shutdown(Sender<()>),
}

pub struct NewSound {
    pub id: String,
    pub name: String,
    pub file: String,
    pub channels: u16,
    pub frames: u64,
    pub loudness_gain_db: f32,
    pub source: SoundSource,
}

struct MappedSound(SbPcmFile);

impl SampleSource for MappedSound {
    fn samples(&self) -> &[i16] {
        self.0.samples()
    }
    fn channels(&self) -> u16 {
        self.0.header().channels
    }
}

#[derive(Default)]
struct Timers {
    save_config: Option<Instant>,
    save_index: Option<Instant>,
    ptt_release: Option<Instant>,
    ui_keepalive: Option<Instant>,
    trash_purge: Option<Instant>,
    devices_refresh: Option<Instant>,
    retry: [Option<Instant>; 2],
}

impl Timers {
    fn next(&self) -> Option<Instant> {
        [
            self.save_config,
            self.save_index,
            self.ptt_release,
            self.ui_keepalive,
            self.trash_purge,
            self.devices_refresh,
            self.retry[0],
            self.retry[1],
        ]
        .into_iter()
        .flatten()
        .min()
    }
}

fn due(slot: &mut Option<Instant>, now: Instant) -> bool {
    if slot.is_some_and(|t| t <= now) {
        *slot = None;
        true
    } else {
        false
    }
}

struct BusSlot {
    kind: BusKind,
    bus: Option<Box<dyn OutputBus>>,
    /// Endpoint effettivo: `None` = nessuna uscita disponibile (es. VB-Cable assente).
    device: Option<DeviceId>,
    device_name: String,
    retries: usize,
}

struct PlayState {
    sound_id: String,
    started_at_ms: u64,
    duration_ms: u64,
    preview: bool,
    buses: Vec<BusKind>,
    pending: Vec<BusKind>,
}

struct HotkeySlot {
    binding: HotkeyBinding,
    id: i32,
    active: bool,
    conflict: bool,
}

struct TrashEntry {
    sound: Sound,
    position: usize,
    hotkeys: Vec<HotkeyBinding>,
    at: Instant,
}

struct Job {
    total: u32,
    done: u32,
    errors: Vec<ImportFileError>,
    added: Vec<String>,
    folder_id: Option<String>,
}

pub struct Engine {
    paths: Paths,
    settings: Settings,
    index: LibraryIndex,
    sink: Arc<dyn EventSink>,
    platform: Box<dyn Platform>,
    waker: Arc<Event>,
    tx: Sender<Command>,
    buses: [BusSlot; 2],
    devices: Vec<Device>,
    mics: Vec<Mic>,
    raw_mics: Vec<MicEndpoint>,
    status: Vec<StatusCode>,
    cable_busy: bool,
    plays: BTreeMap<u64, PlayState>,
    next_play_id: u64,
    mapped: HashMap<String, Arc<dyn SampleSource>>,
    hotkeys: Vec<HotkeySlot>,
    next_hotkey_id: i32,
    hotkeys_paused: bool,
    ptt_down: Option<u16>,
    /// Voce silenziata perché suona un effetto con `mute_voice`.
    voice_muted: bool,
    /// Microfoni silenziati da MicFX (modo Sistema), da riattivare quando la voce torna.
    muted_mics: Vec<String>,
    timers: Timers,
    ui_open: bool,
    trash: Vec<TrashEntry>,
    pending_deletes: Vec<PathBuf>,
    jobs: HashMap<u64, Job>,
    next_job_id: u64,
    notices: Vec<Notice>,
    index_trusted: bool,
    /// Ultime impostazioni di Discord lette; `None` = Discord non trovato o illeggibile.
    discord: Option<DiscordVoice>,
    /// Una lettura è in corso; `discord_again` = rifarla appena finisce.
    discord_checking: bool,
    discord_again: bool,
}

impl Engine {
    pub fn new(
        paths: Paths,
        sink: Arc<dyn EventSink>,
        platform: Box<dyn Platform>,
        waker: Arc<Event>,
        tx: Sender<Command>,
    ) -> Self {
        if let Err(e) = paths.ensure() {
            tracing::error!("impossibile creare le cartelle dati: {e}");
        }
        let mut notices = Vec::new();
        let mut settings = match persist::read_json::<Settings>(&paths.config_file(), config::migrate) {
            ReadOutcome::Loaded(s) => s,
            ReadOutcome::Missing => Settings::default(),
            ReadOutcome::Recovered(s) => {
                tracing::warn!("config_recovered: config.json non valido, ripristinato il backup");
                notices.push(Notice {
                    code: ErrorCode::ConfigRecovered,
                    message: "Le impostazioni erano danneggiate: è stata ripristinata l'ultima copia valida.".into(),
                });
                s
            }
            ReadOutcome::Corrupt => {
                tracing::warn!("config_recovered: config.json e backup non validi, uso i default");
                notices.push(Notice {
                    code: ErrorCode::ConfigRecovered,
                    message: "Le impostazioni erano danneggiate e sono state reimpostate.".into(),
                });
                Settings::default()
            }
        };
        let config_dirty = settings.sanitize();

        let mut index_trusted = false;
        let index = match persist::read_json::<LibraryIndex>(&paths.index_file(), mfx_library::index::migrate) {
            ReadOutcome::Loaded(i) => {
                index_trusted = true;
                i
            }
            ReadOutcome::Recovered(i) => {
                notices.push(Notice {
                    code: ErrorCode::LibraryRecovered,
                    message: "L'indice della libreria era danneggiato: è stata ripristinata l'ultima copia valida."
                        .into(),
                });
                i
            }
            ReadOutcome::Missing => LibraryIndex::default(),
            ReadOutcome::Corrupt => LibraryIndex::default(),
        };

        let slot = |kind| BusSlot { kind, bus: None, device: None, device_name: String::new(), retries: 0 };
        let mut e = Self {
            paths,
            settings,
            index,
            sink,
            platform,
            waker,
            tx,
            buses: [slot(BusKind::Cable), slot(BusKind::Monitor)],
            devices: Vec::new(),
            mics: Vec::new(),
            raw_mics: Vec::new(),
            status: Vec::new(),
            cable_busy: false,
            plays: BTreeMap::new(),
            next_play_id: 1,
            mapped: HashMap::new(),
            hotkeys: Vec::new(),
            next_hotkey_id: 1,
            hotkeys_paused: false,
            ptt_down: None,
            voice_muted: false,
            muted_mics: Vec::new(),
            timers: Timers::default(),
            ui_open: false,
            trash: Vec::new(),
            pending_deletes: Vec::new(),
            jobs: HashMap::new(),
            next_job_id: 1,
            notices,
            index_trusted,
            discord: None,
            discord_checking: false,
            discord_again: false,
        };
        e.recover_orphans();
        e.restore_mics_after_crash();
        if config_dirty {
            e.schedule_config_save();
        }
        e.refresh_devices(false);
        e.register_all_hotkeys();
        e
    }

    pub fn run(mut self, rx: Receiver<Command>) {
        loop {
            let timeout = self.timers.next().map(|t| {
                let ms = t.saturating_duration_since(Instant::now()).as_millis();
                ms.min(u32::MAX as u128 - 1) as u32
            });
            self.waker.wait(timeout);
            while let Ok(cmd) = rx.try_recv() {
                if let Command::Shutdown(done) = cmd {
                    self.shutdown();
                    let _ = done.send(());
                    return;
                }
                self.handle(cmd);
            }
            self.drain_buses();
            self.fire_timers();
        }
    }

    fn handle(&mut self, cmd: Command) {
        match cmd {
            Command::Call(f) => f(self),
            Command::Hotkey(id) => self.on_hotkey(id),
            Command::Device(ev) => match ev {
                DeviceEvent::DefaultChanged { render: true } => {
                    // Il bus Monitor che segue il predefinito si riapre sul nuovo (SPEC §9).
                    self.timers.devices_refresh = Some(Instant::now() + DEVICE_DEBOUNCE);
                    self.reopen_default_monitor();
                }
                _ => self.timers.devices_refresh = Some(Instant::now() + DEVICE_DEBOUNCE),
            },
            Command::ImportItem { job_id, path, result } => self.on_import_item(job_id, path, result),
            Command::ImportDone { job_id } => self.on_import_done(job_id),
            Command::Shutdown(_) => unreachable!(),
        }
    }

    fn emit(&self, ev: CoreEvent) {
        if ev.ui_name().is_some() && !self.ui_open {
            return;
        }
        self.sink.emit(ev);
    }

    // ------------------------------------------------------------------ timer

    fn fire_timers(&mut self) {
        let now = Instant::now();
        if due(&mut self.timers.save_config, now) {
            self.save_config_now();
        }
        if due(&mut self.timers.save_index, now) {
            self.save_index_now();
        }
        if due(&mut self.timers.ptt_release, now) && !self.cable_playing() {
            self.ptt_release();
        }
        if due(&mut self.timers.ui_keepalive, now) {
            self.sink.emit(CoreEvent::UiExpired);
        }
        if due(&mut self.timers.trash_purge, now) {
            self.purge_trash(false);
        }
        if due(&mut self.timers.devices_refresh, now) {
            self.refresh_devices(true);
        }
        for i in 0..2 {
            if due(&mut self.timers.retry[i], now) {
                self.retry_bus(i);
            }
        }
    }

    fn schedule_config_save(&mut self) {
        self.timers.save_config = Some(Instant::now() + SAVE_DEBOUNCE);
    }

    fn schedule_index_save(&mut self) {
        self.timers.save_index = Some(Instant::now() + SAVE_DEBOUNCE);
    }

    fn save_config_now(&mut self) {
        let mut s = self.settings.clone();
        // Le hotkey dei suoni nel cestino non si salvano (tornano solo con "Annulla").
        s.hotkeys = self.hotkeys.iter().map(|h| h.binding.clone()).collect();
        match serde_json::to_vec_pretty(&s) {
            Ok(bytes) => {
                if let Err(e) = persist::write_atomic(&self.paths.config_file(), &bytes) {
                    tracing::error!("salvataggio config fallito: {e}");
                    self.notice(ErrorCode::IoError, format!("Impossibile salvare le impostazioni: {e}"));
                }
            }
            Err(e) => tracing::error!("serializzazione config: {e}"),
        }
    }

    fn save_index_now(&mut self) {
        match serde_json::to_vec_pretty(&self.index) {
            Ok(bytes) => {
                if let Err(e) = persist::write_atomic(&self.paths.index_file(), &bytes) {
                    tracing::error!("salvataggio indice fallito: {e}");
                    self.notice(ErrorCode::IoError, format!("Impossibile salvare la libreria: {e}"));
                } else {
                    self.index_trusted = true;
                }
            }
            Err(e) => tracing::error!("serializzazione indice: {e}"),
        }
    }

    fn notice(&mut self, code: ErrorCode, message: String) {
        let n = Notice { code, message };
        if self.ui_open {
            self.sink.emit(CoreEvent::Notice(n));
        } else {
            self.notices.push(n);
        }
    }

    // ------------------------------------------------------------------ dispositivi

    fn refresh_devices(&mut self, emit: bool) {
        let devices = self.platform.render_endpoints();
        let raw_mics = self.platform.capture_endpoints();
        let changed = devices != self.devices || raw_mics != self.raw_mics;
        self.devices = devices;
        self.raw_mics = raw_mics;
        self.apply_effective_devices();
        if changed && emit {
            self.emit(CoreEvent::DevicesChanged { devices: self.devices.clone(), mics: self.mics.clone() });
            if self.discord.is_some() {
                // Il nome del dispositivo di input di Discord si risolve sugli endpoint attivi.
                self.emit(CoreEvent::DiscordChanged(self.discord_view()));
            }
        }
        self.update_status();
    }

    fn effective_cable(&self) -> Option<&Device> {
        let wanted = self.settings.outputs.cable.device_id.as_deref();
        wanted
            .and_then(|id| self.devices.iter().find(|d| d.id == id))
            .or_else(|| self.devices.iter().find(|d| d.is_cable))
    }

    fn effective_monitor(&self) -> Option<(DeviceId, String)> {
        let cable_id = self.effective_cable().map(|d| d.id.clone());
        if let Some(id) = self.settings.outputs.monitor.device_id.as_deref()
            && let Some(d) = self.devices.iter().find(|d| d.id == id)
            && Some(&d.id) != cable_id.as_ref()
        {
            return DeviceId::new(&d.id).map(|x| (x, d.name.clone()));
        }
        // Segue il predefinito, ma mai su CABLE Input: si sentirebbe doppio su Discord e niente in cuffia.
        match self.devices.iter().find(|d| d.is_default) {
            Some(d) if Some(&d.id) == cable_id.as_ref() => None,
            Some(d) => Some((DeviceId::DEFAULT, d.name.clone())),
            None if self.devices.is_empty() => None,
            None => Some((DeviceId::DEFAULT, String::new())),
        }
    }

    fn apply_effective_devices(&mut self) {
        let cable = self.effective_cable().and_then(|d| DeviceId::new(&d.id).map(|x| (x, d.name.clone())));
        let monitor = self.effective_monitor();
        let cable_id = self.effective_cable().map(|d| d.id.clone());
        let default_render = self.devices.iter().find(|d| d.is_default).map(|d| d.id.clone());
        self.mics = self
            .raw_mics
            .iter()
            .filter(|m| !m.is_cable)
            .map(|m| Mic {
                id: m.id.clone(),
                name: m.name.clone(),
                is_default: m.is_default,
                listen_enabled: m.listen_enabled,
                routed_to_cable: cable_id.as_deref().is_some_and(|c| {
                    m.listen_enabled
                        && match &m.listen_target {
                            Some(t) => t.eq_ignore_ascii_case(c),
                            None => default_render.as_deref().is_some_and(|d| d.eq_ignore_ascii_case(c)),
                        }
                }),
            })
            .collect();
        for (i, target) in [cable, monitor].into_iter().enumerate() {
            let slot = &mut self.buses[i];
            let new_dev = target.as_ref().map(|t| t.0);
            slot.device_name = target.map(|t| t.1).unwrap_or_default();
            if slot.device == new_dev {
                continue;
            }
            slot.device = new_dev;
            slot.retries = 0;
            self.timers.retry[i] = None;
            match (new_dev, slot.bus.as_mut()) {
                (Some(d), Some(bus)) => {
                    let _ = bus.send(BusCmd::SetDevice(d));
                }
                (None, Some(_)) => {
                    // Uscita sparita: le voci su questo bus si chiudono.
                    if let Some(bus) = slot.bus.as_mut() {
                        let _ = bus.send(BusCmd::Abort);
                    }
                }
                _ => {}
            }
        }
        self.update_mic_bridge();
    }

    /// Bridge del microfono (modo Bridge): il microfono scelto, se c'è ancora, altrimenti il predefinito,
    /// mai il cavo. L'ID si risolve qui, così cambiare il predefinito sposta anche il bridge.
    fn update_mic_bridge(&mut self) {
        let route = if self.settings.mic.mode == config::MicMode::Bridge {
            let cable = self.effective_cable().map(|c| c.id.clone());
            let mics = || self.raw_mics.iter().filter(|m| !m.is_cable);
            let wanted = self.settings.mic.device_id.as_deref();
            let mic = wanted
                .and_then(|id| mics().find(|m| m.id == id))
                .or_else(|| mics().find(|m| m.is_default))
                .or_else(|| mics().next())
                .map(|m| m.id.clone());
            match (cable, mic) {
                (Some(cable_id), Some(mic_id)) => Some(MicRoute { mic_id: Some(mic_id), cable_id }),
                _ => None,
            }
        } else {
            None
        };
        self.platform.set_mic_bridge(route);
    }

    fn reopen_default_monitor(&mut self) {
        let slot = &mut self.buses[BusKind::Monitor as usize];
        if slot.device == Some(DeviceId::DEFAULT)
            && let Some(bus) = slot.bus.as_mut()
        {
            let _ = bus.send(BusCmd::SetDevice(DeviceId::DEFAULT));
        }
    }

    fn update_status(&mut self) {
        let mut s = Vec::new();
        let cable = self.effective_cable().cloned();
        match &cable {
            None => s.push(ErrorCode::CableMissing),
            Some(c) => {
                if c.is_default {
                    s.push(ErrorCode::DefaultIsCable);
                }
                if self.cable_busy {
                    s.push(ErrorCode::CableBusy);
                }
                if self.settings.mic.mode == config::MicMode::System && !self.mics.iter().any(|m| m.routed_to_cable) {
                    s.push(ErrorCode::MicNotRouted);
                }
                if let Some(d) = self.discord_view() {
                    if d.input_is_cable == Some(false) {
                        s.push(ErrorCode::DiscordWrongInput);
                    }
                    if d.krisp || d.noise_suppression {
                        s.push(ErrorCode::DiscordFiltersSounds);
                    }
                    if d.push_to_talk && !self.settings.ptt.enabled {
                        s.push(ErrorCode::DiscordPushToTalk);
                    }
                }
            }
        }
        let monitor_rate = match self.effective_monitor() {
            Some((d, _)) if d.is_default() => self.devices.iter().find(|x| x.is_default).map(|x| x.mix_rate),
            Some(_) => self
                .settings
                .outputs
                .monitor
                .device_id
                .as_deref()
                .and_then(|id| self.devices.iter().find(|x| x.id == id))
                .map(|x| x.mix_rate),
            None => None,
        };
        let mismatch = |r: Option<u32>| r.is_some_and(|r| r != 0 && r != mfx_audio::SAMPLE_RATE);
        if mismatch(cable.as_ref().map(|c| c.mix_rate)) || mismatch(monitor_rate) {
            s.push(ErrorCode::RateMismatch);
        }
        s.sort();
        if s != self.status {
            self.status = s;
            self.emit(CoreEvent::StatusChanged(self.status.clone()));
        }
    }

    // ------------------------------------------------------------------ Discord

    /// Rilegge le impostazioni di Discord su un thread a parte (qualche MB su disco): l'engine
    /// resta libero per le hotkey. Più richieste ravvicinate diventano al massimo una lettura in più.
    pub(crate) fn check_discord(&mut self) {
        if self.discord_checking {
            self.discord_again = true;
            return;
        }
        let read = self.platform.discord_reader();
        let (tx, waker) = (self.tx.clone(), self.waker.clone());
        let spawned = std::thread::Builder::new().name("mfx-discord".into()).spawn(move || {
            let v = read();
            let _ = tx.send(Command::Call(Box::new(move |e: &mut Engine| e.on_discord(v))));
            waker.set();
        });
        match spawned {
            Ok(_) => self.discord_checking = true,
            Err(e) => tracing::warn!("lettura impostazioni Discord non avviata: {e}"),
        }
    }

    fn on_discord(&mut self, v: Option<DiscordVoice>) {
        self.discord_checking = false;
        if self.discord != v {
            match &v {
                Some(d) => tracing::info!(
                    "Discord: input {}, krisp {}, soppressione {}, vad avanzato {}, eco {}, agc {}, ptt {}",
                    d.input_device_id,
                    d.krisp,
                    d.noise_suppression,
                    d.advanced_vad,
                    d.echo_cancellation,
                    d.automatic_gain_control,
                    d.push_to_talk
                ),
                None => tracing::info!("Discord: impostazioni voce non trovate"),
            }
            self.discord = v;
            self.emit(CoreEvent::DiscordChanged(self.discord_view()));
            self.update_status();
        }
        if std::mem::take(&mut self.discord_again) {
            self.check_discord();
        }
    }

    fn discord_view(&self) -> Option<DiscordInfo> {
        let d = self.discord.as_ref()?;
        let is_default = d.input_device_id.eq_ignore_ascii_case("default");
        let input = if is_default {
            self.raw_mics.iter().find(|m| m.is_default)
        } else {
            self.raw_mics.iter().find(|m| m.id.eq_ignore_ascii_case(&d.input_device_id))
        };
        Some(DiscordInfo {
            client: d.client.to_string(),
            input_name: input.map(|m| m.name.clone()),
            input_is_cable: input.map(|m| m.is_cable),
            input_is_default: is_default,
            push_to_talk: d.push_to_talk,
            krisp: d.krisp,
            noise_suppression: d.noise_suppression,
            advanced_vad: d.advanced_vad && !d.push_to_talk,
            echo_cancellation: d.echo_cancellation,
            automatic_gain_control: d.automatic_gain_control,
        })
    }

    // ------------------------------------------------------------------ bus

    fn ensure_bus(&mut self, kind: BusKind) -> bool {
        let i = kind as usize;
        if self.buses[i].bus.is_some() {
            return true;
        }
        let Some(device) = self.buses[i].device else { return false };
        let out = self.settings.outputs.get(kind);
        let params = BusParams {
            gain: out.linear_gain(),
            ceiling_db: self.settings.loudness.ceiling_dbtp,
            idle_stop_ms: self.settings.playback.idle_stop_ms,
            warm_hold_ms: self.settings.playback.warm_hold_ms,
        };
        let waker: Arc<dyn Wake> = self.waker.clone();
        match self.platform.create_bus(kind, device, params, waker) {
            Ok(b) => {
                self.buses[i].bus = Some(b);
                true
            }
            Err(e) => {
                tracing::error!("bus {kind:?} non creato: {e}");
                false
            }
        }
    }

    fn send_bus(&mut self, kind: BusKind, cmd: BusCmd) -> bool {
        match self.buses[kind as usize].bus.as_mut() {
            Some(b) => b.send(cmd).is_ok(),
            None => false,
        }
    }

    fn drain_buses(&mut self) {
        for i in 0..2 {
            let mut events = Vec::new();
            if let Some(b) = self.buses[i].bus.as_mut() {
                b.drain(&mut |e| events.push(e));
            }
            let kind = self.buses[i].kind;
            for e in events {
                self.on_bus_event(kind, e);
            }
        }
    }

    fn on_bus_event(&mut self, kind: BusKind, e: BusEvent) {
        let i = kind as usize;
        match e {
            BusEvent::Ended { play_id, sound } => {
                drop(sound); // l'Arc muore qui, fuori dal thread audio
                self.play_ended_on(kind, play_id);
            }
            BusEvent::Opened { mix_rate } => {
                tracing::info!("bus {kind:?} aperto (mix {mix_rate} Hz)");
                self.buses[i].retries = 0;
                self.timers.retry[i] = None;
                if kind == BusKind::Cable && self.cable_busy {
                    self.cable_busy = false;
                    self.update_status();
                }
            }
            BusEvent::Lost { hresult } => {
                tracing::warn!("bus {kind:?} perso (0x{:08X})", hresult as u32);
                let slot = &mut self.buses[i];
                if slot.retries < RETRY_BACKOFF.len() {
                    self.timers.retry[i] = Some(Instant::now() + RETRY_BACKOFF[slot.retries]);
                    slot.retries += 1;
                } else {
                    slot.retries = 0;
                    let _ = self.send_bus(kind, BusCmd::Abort);
                    let what = if kind == BusKind::Cable { "il microfono virtuale" } else { "le cuffie" };
                    self.notice(
                        ErrorCode::DeviceLost,
                        format!("Impossibile riprodurre verso {what}: dispositivo non disponibile."),
                    );
                }
            }
            BusEvent::Busy => {
                let _ = self.send_bus(kind, BusCmd::Abort);
                if kind == BusKind::Cable {
                    self.cable_busy = true;
                    self.update_status();
                } else {
                    self.notice(
                        ErrorCode::DeviceLost,
                        "Le cuffie sono usate in modo esclusivo da un'altra app.".into(),
                    );
                }
            }
        }
    }

    fn retry_bus(&mut self, i: usize) {
        let kind = self.buses[i].kind;
        let has_plays = self.plays.values().any(|p| p.pending.contains(&kind));
        if has_plays {
            let _ = self.send_bus(kind, BusCmd::Retry);
        } else {
            self.buses[i].retries = 0;
        }
    }

    // ------------------------------------------------------------------ riproduzione

    fn map_sound(&mut self, sound_id: &str) -> ApiResult<Arc<dyn SampleSource>> {
        if let Some(a) = self.mapped.get(sound_id) {
            return Ok(a.clone());
        }
        let sound = self.index.sound(sound_id).ok_or_else(|| not_found(sound_id))?;
        let path = self.paths.pcm_dir().join(&sound.file);
        let f = SbPcmFile::open(&path).map_err(|e| {
            ApiError::new(
                ErrorCode::SoundNotFound,
                format!("File audio di \"{}\" mancante o illeggibile: {e}", sound.name),
            )
        })?;
        let a: Arc<dyn SampleSource> = Arc::new(MappedSound(f));
        self.mapped.insert(sound_id.to_string(), a.clone());
        Ok(a)
    }

    fn plays_of(&self, sound_id: &str) -> Vec<u64> {
        self.plays.iter().filter(|(_, p)| p.sound_id == sound_id).map(|(id, _)| *id).collect()
    }

    pub(crate) fn play(&mut self, sound_id: &str, preview: bool) -> ApiResult<Option<u64>> {
        let sound = self.index.sound(sound_id).ok_or_else(|| not_found(sound_id))?.clone();
        let mode = if preview { PlayMode::Restart } else { sound.mode.unwrap_or(self.settings.playback.default_mode) };
        let existing: Vec<u64> = self
            .plays
            .iter()
            .filter(|(_, p)| p.sound_id == sound_id && p.preview == preview)
            .map(|(id, _)| *id)
            .collect();
        match mode {
            PlayMode::Overlap => {}
            PlayMode::Restart => self.stop_plays(&existing),
            PlayMode::Toggle => {
                if !existing.is_empty() {
                    self.stop_plays(&existing);
                    return Ok(None);
                }
            }
            PlayMode::Exclusive => self.stop_all(),
        }

        let mut targets = Vec::new();
        if !preview && self.settings.outputs.cable.enabled && self.buses[0].device.is_some() {
            targets.push(BusKind::Cable);
        }
        if (preview || self.settings.outputs.monitor.enabled) && self.buses[1].device.is_some() {
            targets.push(BusKind::Monitor);
        }
        if targets.is_empty() {
            let msg = if preview {
                "Nessun dispositivo di ascolto disponibile.".to_string()
            } else if !self.settings.outputs.cable.enabled && !self.settings.outputs.monitor.enabled {
                "Microfono e cuffie sono disattivati: attivane almeno uno per sentire i suoni.".to_string()
            } else {
                "Nessuna uscita audio disponibile.".to_string()
            };
            return Err(ApiError::new(ErrorCode::OutputsDisabled, msg));
        }

        let src = self.map_sound(sound_id)?;
        let gain = mfx_audio::db_to_lin(sound.loudness_gain_db + sound.user_gain_db);
        let play_id = self.next_play_id;
        self.next_play_id += 1;

        let mut sent = Vec::new();
        for kind in targets {
            if !self.ensure_bus(kind) {
                continue;
            }
            let cmd = BusCmd::Mixer(MixerCmd::Play { play_id, sound: SoundRef::new(src.clone()), gain });
            if self.send_bus(kind, cmd) {
                sent.push(kind);
            }
        }
        if sent.is_empty() {
            return Err(ApiError::new(ErrorCode::DeviceLost, "Impossibile avviare l'audio."));
        }
        if sent.contains(&BusKind::Cable) {
            self.ptt_press();
        }
        let p = PlayState {
            sound_id: sound_id.to_string(),
            started_at_ms: mfx_library::time::unix_ms(),
            duration_ms: sound.duration_ms(),
            preview,
            buses: sent.clone(),
            pending: sent,
        };
        let playing = playing_of(play_id, &p);
        self.plays.insert(play_id, p);
        self.update_voice_mute();
        self.emit(CoreEvent::PlaybackStarted(playing));
        Ok(Some(play_id))
    }

    fn stop_plays(&mut self, ids: &[u64]) {
        for id in ids {
            let Some(p) = self.plays.get(id) else { continue };
            for kind in p.pending.clone() {
                let _ = self.send_bus(kind, BusCmd::Mixer(MixerCmd::Stop { play_id: *id }));
            }
        }
    }

    pub(crate) fn stop_sound(&mut self, sound_id: &str) {
        let ids = self.plays_of(sound_id);
        self.stop_plays(&ids);
    }

    pub(crate) fn stop_all(&mut self) {
        for kind in [BusKind::Cable, BusKind::Monitor] {
            let _ = self.send_bus(kind, BusCmd::Mixer(MixerCmd::StopAll));
        }
    }

    fn play_ended_on(&mut self, kind: BusKind, play_id: u64) {
        let finished = match self.plays.get_mut(&play_id) {
            Some(p) => {
                p.pending.retain(|k| *k != kind);
                p.pending.is_empty()
            }
            None => false,
        };
        if finished && let Some(p) = self.plays.remove(&play_id) {
            self.emit(CoreEvent::PlaybackEnded(PlaybackEnded { play_id, sound_id: p.sound_id }));
        }
        if kind == BusKind::Cable {
            self.update_voice_mute();
        }
        if kind == BusKind::Cable && self.ptt_down.is_some() && !self.cable_playing() {
            self.timers.ptt_release = Some(Instant::now() + Duration::from_millis(self.settings.ptt.tail_ms as u64));
        }
        if !self.pending_deletes.is_empty() {
            self.retry_pending_deletes();
        }
    }

    fn cable_playing(&self) -> bool {
        self.plays.values().any(|p| p.pending.contains(&BusKind::Cable))
    }

    // ------------------------------------------------------------------ "Disattiva la mia voce"

    /// La voce va silenziata finché nel cavo suona almeno un effetto con `mute_voice`. L'opzione si
    /// legge dal suono a ogni controllo: cambiarla mentre suona ha effetto subito.
    fn update_voice_mute(&mut self) {
        let want = self.plays.values().any(|p| {
            p.pending.contains(&BusKind::Cable) && self.index.sound(&p.sound_id).is_some_and(|s| s.mute_voice)
        });
        if want == self.voice_muted {
            return;
        }
        self.voice_muted = want;
        // Modo Bridge (macOS): la voce passa dall'app, basta azzerarla.
        self.platform.set_bridge_muted(want);
        if !want {
            self.unmute_mics();
            return;
        }
        // Modo Sistema: Windows manda il microfono nel cavo da sé, quindi si silenzia il microfono.
        // Solo quelli che MicFX trova attivi: chi era già muto resta com'era alla fine.
        let routed: Vec<String> = self.mics.iter().filter(|m| m.routed_to_cable).map(|m| m.id.clone()).collect();
        for id in routed {
            if self.platform.set_mic_mute(&id, true) == Some(false) {
                self.muted_mics.push(id);
            }
        }
        if !self.muted_mics.is_empty() {
            // Se MicFX si chiude prima di riattivarli, al prossimo avvio si riattivano da qui.
            match serde_json::to_vec(&self.muted_mics) {
                Ok(bytes) => {
                    if let Err(e) = persist::write_atomic(&self.paths.muted_mics_file(), &bytes) {
                        tracing::warn!("elenco microfoni silenziati non salvato: {e}");
                    }
                }
                Err(e) => tracing::warn!("elenco microfoni silenziati: {e}"),
            }
        }
    }

    fn unmute_mics(&mut self) {
        if self.muted_mics.is_empty() {
            return;
        }
        for id in std::mem::take(&mut self.muted_mics) {
            if self.platform.set_mic_mute(&id, false).is_none() {
                tracing::warn!("microfono {id} non riattivato");
            }
        }
        let _ = std::fs::remove_file(self.paths.muted_mics_file());
    }

    fn restore_mics_after_crash(&mut self) {
        let path = self.paths.muted_mics_file();
        let Ok(bytes) = std::fs::read(&path) else { return };
        let ids: Vec<String> = serde_json::from_slice(&bytes).unwrap_or_default();
        tracing::info!("riattivo {} microfoni silenziati prima della chiusura", ids.len());
        self.muted_mics = ids;
        self.unmute_mics();
        let _ = std::fs::remove_file(path);
    }

    // ------------------------------------------------------------------ PTT (SPEC §8.2)

    fn ptt_press(&mut self) {
        self.timers.ptt_release = None;
        if self.ptt_down.is_some() || !self.settings.ptt.enabled {
            return;
        }
        let Some(sc) = self.settings.ptt.scancode else { return };
        if self.platform.send_key(sc, true) {
            self.ptt_down = Some(sc);
            crate::PTT_PRESSED.store(sc as u32, std::sync::atomic::Ordering::SeqCst);
        } else {
            let msg = if cfg!(target_os = "macos") {
                "macOS ha bloccato il tasto push-to-talk: consenti MicFX in Impostazioni di Sistema → Privacy e                  sicurezza → Accessibilità."
            } else {
                "Windows ha bloccato il tasto push-to-talk: Discord è avviato come amministratore?"
            };
            self.notice(ErrorCode::PttSendFailed, msg.into());
        }
    }

    pub(crate) fn ptt_release(&mut self) {
        self.timers.ptt_release = None;
        if let Some(sc) = self.ptt_down.take() {
            let _ = self.platform.send_key(sc, false);
            crate::PTT_PRESSED.store(0, std::sync::atomic::Ordering::SeqCst);
        }
    }

    // ------------------------------------------------------------------ hotkey

    fn register_all_hotkeys(&mut self) {
        let bindings = std::mem::take(&mut self.settings.hotkeys);
        for b in bindings {
            // Hotkey di suoni che non esistono più: si scartano.
            if let Some(sid) = b.action.sound_id()
                && self.index.sound(sid).is_none()
            {
                self.schedule_config_save();
                continue;
            }
            let id = self.next_hotkey_id;
            self.next_hotkey_id += 1;
            self.hotkeys.push(HotkeySlot { binding: b, id, active: false, conflict: false });
            let i = self.hotkeys.len() - 1;
            self.register_slot(i);
        }
    }

    fn register_slot(&mut self, i: usize) {
        let slot = &self.hotkeys[i];
        let paused_out = self.hotkeys_paused && slot.binding.action != HotkeyAction::PauseHotkeys;
        let (id, accel) = (slot.id, Accelerator::parse(&slot.binding.accelerator));
        let (active, conflict) = match accel {
            Err(_) => (false, false),
            Ok(_) if paused_out => (false, false),
            Ok(a) => match self.platform.register_hotkey(id, a) {
                Ok(()) => (true, false),
                Err(RegisterError::Conflict) => (false, true),
                Err(RegisterError::Failed(e)) => {
                    tracing::warn!("hotkey {} non registrata: {e}", self.hotkeys[i].binding.accelerator);
                    (false, false)
                }
            },
        };
        let slot = &mut self.hotkeys[i];
        slot.active = active;
        if !paused_out {
            slot.conflict = conflict;
        }
    }

    /// Il sistema ha rifiutato in ritardo una hotkey già data per registrata (macOS: Carbon risponde
    /// sul thread principale).
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub(crate) fn hotkey_rejected(&mut self, id: i32, conflict: bool) {
        let Some(slot) = self.hotkeys.iter_mut().find(|h| h.id == id) else { return };
        if !slot.active {
            return;
        }
        slot.active = false;
        slot.conflict = conflict;
        self.emit_hotkeys();
    }

    fn hotkey_views(&self) -> Vec<Hotkey> {
        self.hotkeys
            .iter()
            .map(|h| Hotkey {
                action: h.binding.action.clone(),
                accelerator: h.binding.accelerator.clone(),
                active: h.active,
                conflict: h.conflict,
            })
            .collect()
    }

    fn emit_hotkeys(&self) {
        self.emit(CoreEvent::HotkeysChanged { hotkeys: self.hotkey_views(), paused: self.hotkeys_paused });
        // Il guscio aggiorna la spunta nel menu tray anche a finestra chiusa.
        if !self.ui_open {
            self.sink.emit(CoreEvent::HotkeysChanged { hotkeys: Vec::new(), paused: self.hotkeys_paused });
        }
    }

    fn remove_hotkey_at(&mut self, i: usize) -> HotkeyBinding {
        let slot = self.hotkeys.remove(i);
        if slot.active {
            self.platform.unregister_hotkey(slot.id);
        }
        slot.binding
    }

    pub(crate) fn set_hotkey(
        &mut self,
        action: HotkeyAction,
        accelerator: Option<String>,
    ) -> ApiResult<Option<Hotkey>> {
        if let Some(sid) = action.sound_id()
            && self.index.sound(sid).is_none()
        {
            return Err(not_found(sid));
        }
        let canonical = match accelerator.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            None => None,
            Some(s) => Some(
                Accelerator::parse(s).map_err(|e| ApiError::new(ErrorCode::HotkeyInvalid, e.to_string()))?.canonical(),
            ),
        };
        if let Some(i) = self.hotkeys.iter().position(|h| h.binding.action == action) {
            self.remove_hotkey_at(i);
        }
        let result = match canonical {
            None => None,
            Some(acc) => {
                // Una combinazione appartiene a una sola azione: l'ultima vince.
                if let Some(i) = self.hotkeys.iter().position(|h| h.binding.accelerator == acc) {
                    self.remove_hotkey_at(i);
                }
                let id = self.next_hotkey_id;
                self.next_hotkey_id += 1;
                self.hotkeys.push(HotkeySlot {
                    binding: HotkeyBinding { accelerator: acc, action },
                    id,
                    active: false,
                    conflict: false,
                });
                let i = self.hotkeys.len() - 1;
                self.register_slot(i);
                self.hotkey_views().pop()
            }
        };
        self.schedule_config_save();
        self.emit_hotkeys();
        Ok(result)
    }

    pub(crate) fn pause_hotkeys(&mut self, paused: bool) {
        if paused == self.hotkeys_paused {
            return;
        }
        self.hotkeys_paused = paused;
        for i in 0..self.hotkeys.len() {
            if self.hotkeys[i].binding.action == HotkeyAction::PauseHotkeys {
                continue;
            }
            if paused {
                if self.hotkeys[i].active {
                    self.platform.unregister_hotkey(self.hotkeys[i].id);
                    self.hotkeys[i].active = false;
                }
            } else {
                self.register_slot(i);
            }
        }
        if paused {
            self.ptt_release();
        }
        self.emit_hotkeys();
    }

    pub(crate) fn hotkeys_paused(&self) -> bool {
        self.hotkeys_paused
    }

    fn on_hotkey(&mut self, id: i32) {
        let Some(slot) = self.hotkeys.iter().find(|h| h.id == id) else { return };
        let action = slot.binding.action.clone();
        let res = match action {
            HotkeyAction::Play { sound_id } => self.play(&sound_id, false).map(|_| ()),
            HotkeyAction::Preview { sound_id } => self.play(&sound_id, true).map(|_| ()),
            HotkeyAction::StopAll => {
                self.stop_all();
                Ok(())
            }
            HotkeyAction::PauseHotkeys => {
                let p = !self.hotkeys_paused;
                self.pause_hotkeys(p);
                Ok(())
            }
            HotkeyAction::MuteMonitor => {
                let on = !self.settings.outputs.monitor.enabled;
                self.set_output(BusKind::Monitor, None, None, Some(on)).map(|_| ())
            }
            HotkeyAction::MuteCable => {
                let on = !self.settings.outputs.cable.enabled;
                self.set_output(BusKind::Cable, None, None, Some(on)).map(|_| ())
            }
            HotkeyAction::OpenWindow => {
                self.sink.emit(CoreEvent::OpenWindow);
                Ok(())
            }
        };
        if let Err(e) = res {
            tracing::info!("hotkey: {e}");
            self.notice(e.code, e.message);
        }
    }

    // ------------------------------------------------------------------ uscite e impostazioni

    pub(crate) fn set_output(
        &mut self,
        bus: BusKind,
        device_id: Option<Option<String>>,
        volume_db: Option<f32>,
        enabled: Option<bool>,
    ) -> ApiResult<Vec<StatusCode>> {
        if let Some(Some(id)) = &device_id {
            if !self.devices.iter().any(|d| &d.id == id) {
                return Err(ApiError::new(ErrorCode::DeviceLost, "Dispositivo non trovato."));
            }
            if bus == BusKind::Monitor && self.effective_cable().is_some_and(|c| &c.id == id) {
                return Err(ApiError::new(
                    ErrorCode::MonitorIsCable,
                    "Le cuffie non possono essere CABLE Input: è il microfono virtuale.",
                ));
            }
        }
        let old = self.settings.clone();
        let out = self.settings.outputs.get_mut(bus);
        if let Some(d) = device_id {
            out.device_id = d;
        }
        if let Some(v) = volume_db {
            if !v.is_finite() {
                return Err(ApiError::new(ErrorCode::InvalidArgument, "Volume non valido."));
            }
            out.volume_db = v.clamp(config::MIN_VOLUME_DB, config::MAX_VOLUME_DB);
        }
        if let Some(e) = enabled {
            out.enabled = e;
        }
        self.after_settings_change(&old);
        Ok(self.status.clone())
    }

    pub(crate) fn update_settings(&mut self, patch: serde_json::Value) -> ApiResult<Settings> {
        if !patch.is_object() {
            return Err(ApiError::new(ErrorCode::InvalidArgument, "Patch non valida."));
        }
        let mut patch = patch;
        if let Some(obj) = patch.as_object_mut() {
            // Le hotkey passano da set_hotkey, la versione dello schema la decide il core.
            obj.remove("hotkeys");
            obj.remove("schema_version");
        }
        let mut base = serde_json::to_value(&self.settings).map_err(internal)?;
        config::merge_json(&mut base, &patch);
        let mut next: Settings =
            serde_json::from_value(base).map_err(|e| ApiError::new(ErrorCode::InvalidArgument, e.to_string()))?;
        next.hotkeys = self.settings.hotkeys.clone();
        next.sanitize();
        if let Some(key) = next.ptt.key.clone() {
            match Accelerator::parse(&key) {
                Ok(a) if a.mods.is_empty() => {
                    next.ptt.key = Some(a.key.name.to_string());
                    next.ptt.scancode = Some(a.key.scancode);
                }
                _ => return Err(ApiError::new(ErrorCode::HotkeyInvalid, "Tasto push-to-talk non valido.")),
            }
        } else {
            next.ptt.scancode = None;
        }
        if next.outputs.monitor.device_id.is_some()
            && next.outputs.monitor.device_id == self.effective_cable().map(|c| c.id.clone())
        {
            return Err(ApiError::new(ErrorCode::MonitorIsCable, "Le cuffie non possono essere CABLE Input."));
        }
        let old = std::mem::replace(&mut self.settings, next);
        self.after_settings_change(&old);
        Ok(self.settings.clone())
    }

    fn after_settings_change(&mut self, old: &Settings) {
        if *old == self.settings {
            return;
        }
        for kind in [BusKind::Cable, BusKind::Monitor] {
            let (o, n): (&OutputSettings, &OutputSettings) = (old.outputs.get(kind), self.settings.outputs.get(kind));
            if o.volume_db != n.volume_db
                && let Some(b) = self.buses[kind as usize].bus.as_ref()
            {
                b.set_gain(n.linear_gain());
            }
            if o.enabled && !n.enabled {
                // Uscita spenta: si zittisce subito (anteprime comprese sul monitor).
                let _ = self.send_bus(kind, BusCmd::Mixer(MixerCmd::StopAll));
            }
        }
        if old.outputs.cable.device_id != self.settings.outputs.cable.device_id
            || old.outputs.monitor.device_id != self.settings.outputs.monitor.device_id
            || old.mic != self.settings.mic
        {
            self.apply_effective_devices();
        }
        if !self.settings.ptt.enabled || old.ptt.scancode != self.settings.ptt.scancode {
            self.ptt_release();
            if self.settings.ptt.enabled && self.cable_playing() {
                self.ptt_press();
            }
        }
        self.update_status();
        self.schedule_config_save();
        self.emit(CoreEvent::SettingsChanged(Box::new(self.settings_view())));
    }

    fn settings_view(&self) -> Settings {
        let mut s = self.settings.clone();
        s.hotkeys = self.hotkeys.iter().map(|h| h.binding.clone()).collect();
        s
    }

    pub(crate) fn settings(&self) -> Settings {
        self.settings_view()
    }

    pub(crate) fn save_window_rect(&mut self, rect: WindowRect) {
        if self.settings.ui.window != Some(rect) {
            self.settings.ui.window = Some(rect);
            self.schedule_config_save();
        }
    }

    // ------------------------------------------------------------------ UI

    pub(crate) fn ui_opened(&mut self) {
        self.ui_open = true;
        self.timers.ui_keepalive = None;
        // L'utente potrebbe aver cambiato Discord mentre la finestra era chiusa.
        self.check_discord();
    }

    pub(crate) fn ui_hidden(&mut self) {
        let secs = self.settings.ui.keepalive_s as u64;
        self.timers.ui_keepalive = Some(Instant::now() + Duration::from_secs(secs));
    }

    pub(crate) fn ui_closed(&mut self) {
        self.ui_open = false;
        self.timers.ui_keepalive = None;
    }

    pub(crate) fn snapshot(&mut self) -> Snapshot {
        self.ui_open = true;
        Snapshot {
            settings: self.settings_view(),
            devices: self.devices.clone(),
            mics: self.mics.clone(),
            library: Library { folders: self.sorted_folders(), sounds: self.index.sounds.clone() },
            hotkeys: self.hotkey_views(),
            hotkeys_paused: self.hotkeys_paused,
            status: self.status.clone(),
            playing: self.plays.iter().map(|(id, p)| playing_of(*id, p)).collect(),
            app: AppInfo {
                version: env!("CARGO_PKG_VERSION").to_string(),
                data_dir: self.paths.local.display().to_string(),
                library_dir: self.paths.library_dir().display().to_string(),
            },
            notices: std::mem::take(&mut self.notices),
            discord: self.discord_view(),
        }
    }

    pub(crate) fn diagnostics(&self) -> Diagnostics {
        let buses = self
            .buses
            .iter()
            .map(|s| {
                let st = s.bus.as_ref().map(|b| b.stats()).unwrap_or_default();
                BusDiagnostics {
                    bus: s.kind,
                    state: if s.bus.is_none() { "none".into() } else { format!("{:?}", st.state).to_lowercase() },
                    device: s.device_name.clone(),
                    voices: st.voices,
                    underruns: st.underruns,
                    errors: st.errors,
                    blocks: st.blocks,
                    mix_rate: st.mix_rate,
                    buffer_frames: st.buffer_frames,
                }
            })
            .collect();
        Diagnostics {
            version: env!("CARGO_PKG_VERSION").to_string(),
            buses,
            playing: self.plays.len() as u32,
            mapped_sounds: self.mapped.len() as u32,
            sounds: self.index.sounds.len() as u32,
            config_path: self.paths.config_file().display().to_string(),
            library_path: self.paths.index_file().display().to_string(),
            ptt_down: self.ptt_down.is_some(),
        }
    }

    // ------------------------------------------------------------------ libreria

    fn sorted_folders(&self) -> Vec<Folder> {
        let mut f = self.index.folders.clone();
        f.sort_by_key(|x| x.order);
        f
    }

    fn library_changed(&self, added: Vec<Sound>, updated: Vec<Sound>, removed: Vec<String>) {
        self.emit(CoreEvent::LibraryChanged(LibraryChanged {
            added,
            updated,
            removed,
            folders: self.sorted_folders(),
        }));
    }

    pub(crate) fn update_sound(&mut self, sound_id: &str, patch: SoundPatch) -> ApiResult<Sound> {
        if let Some(Some(fid)) = &patch.folder_id
            && !self.index.folders.iter().any(|f| &f.id == fid)
        {
            return Err(ApiError::new(ErrorCode::FolderNotFound, "Cartella non trovata."));
        }
        let s = self.index.sound_mut(sound_id).ok_or_else(|| not_found(sound_id))?;
        if let Some(name) = patch.name {
            let name: String = name.split_whitespace().collect::<Vec<_>>().join(" ");
            if name.is_empty() {
                return Err(ApiError::new(ErrorCode::InvalidArgument, "Il nome non può essere vuoto."));
            }
            s.name = name.chars().take(80).collect();
        }
        if let Some(g) = patch.user_gain_db {
            if !g.is_finite() {
                return Err(ApiError::new(ErrorCode::InvalidArgument, "Guadagno non valido."));
            }
            s.user_gain_db = g.clamp(-30.0, 12.0);
        }
        if let Some(m) = patch.mode {
            s.mode = m;
        }
        let mute_changed = patch.mute_voice.is_some_and(|v| v != s.mute_voice);
        if let Some(v) = patch.mute_voice {
            s.mute_voice = v;
        }
        if let Some(f) = patch.folder_id {
            s.folder_id = f;
        }
        if let Some(t) = patch.tags {
            s.tags = t.into_iter().map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect();
        }
        if let Some(ui) = patch.ui {
            let serde_json::Value::Object(p) = ui else {
                return Err(ApiError::new(ErrorCode::InvalidArgument, "ui deve essere un oggetto."));
            };
            if !s.ui.is_object() {
                s.ui = serde_json::Value::Object(Default::default());
            }
            let obj = s.ui.as_object_mut().expect("oggetto");
            for (k, v) in p {
                if v.is_null() {
                    obj.remove(&k);
                } else {
                    obj.insert(k, v);
                }
            }
        }
        let out = s.clone();
        self.schedule_index_save();
        if mute_changed {
            self.update_voice_mute();
        }
        self.library_changed(Vec::new(), vec![out.clone()], Vec::new());
        Ok(out)
    }

    pub(crate) fn delete_sounds(&mut self, ids: &[String]) -> ApiResult<()> {
        let mut removed = Vec::new();
        let now = Instant::now();
        for id in ids {
            let Some(pos) = self.index.sounds.iter().position(|s| &s.id == id) else { continue };
            self.stop_sound(id);
            let sound = self.index.sounds.remove(pos);
            let mut hk = Vec::new();
            while let Some(i) = self.hotkeys.iter().position(|h| h.binding.action.sound_id() == Some(id.as_str())) {
                hk.push(self.remove_hotkey_at(i));
            }
            self.mapped.remove(id);
            removed.push(id.clone());
            self.trash.push(TrashEntry { sound, position: pos, hotkeys: hk, at: now });
        }
        if removed.is_empty() {
            return Ok(());
        }
        self.timers.trash_purge = Some(now + TRASH_HOLD);
        self.schedule_index_save();
        self.schedule_config_save();
        self.library_changed(Vec::new(), Vec::new(), removed);
        self.emit_hotkeys();
        Ok(())
    }

    /// "Annulla" dopo un'eliminazione: i suoni tornano dove erano, con le loro hotkey.
    pub(crate) fn restore_sounds(&mut self, ids: &[String]) -> ApiResult<Vec<Sound>> {
        let mut restored = Vec::new();
        let mut entries: Vec<TrashEntry> = Vec::new();
        self.trash.retain_mut(|t| {
            if ids.contains(&t.sound.id) {
                entries.push(TrashEntry {
                    sound: t.sound.clone(),
                    position: t.position,
                    hotkeys: std::mem::take(&mut t.hotkeys),
                    at: t.at,
                });
                false
            } else {
                true
            }
        });
        entries.sort_by_key(|e| e.position);
        for e in entries {
            let mut sound = e.sound;
            if let Some(f) = &sound.folder_id
                && !self.index.folders.iter().any(|x| &x.id == f)
            {
                sound.folder_id = None;
            }
            let pos = e.position.min(self.index.sounds.len());
            self.index.sounds.insert(pos, sound.clone());
            for b in e.hotkeys {
                if self.hotkeys.iter().any(|h| h.binding.accelerator == b.accelerator) {
                    continue;
                }
                let id = self.next_hotkey_id;
                self.next_hotkey_id += 1;
                self.hotkeys.push(HotkeySlot { binding: b, id, active: false, conflict: false });
                let i = self.hotkeys.len() - 1;
                self.register_slot(i);
            }
            restored.push(sound);
        }
        if !restored.is_empty() {
            self.schedule_index_save();
            self.schedule_config_save();
            self.library_changed(restored.clone(), Vec::new(), Vec::new());
            self.emit_hotkeys();
        }
        Ok(restored)
    }

    fn purge_trash(&mut self, all: bool) {
        let now = Instant::now();
        let mut keep = Vec::new();
        for t in std::mem::take(&mut self.trash) {
            if all || now.duration_since(t.at) >= TRASH_HOLD {
                let path = self.paths.pcm_dir().join(&t.sound.file);
                if std::fs::remove_file(&path).is_err() && path.exists() {
                    self.pending_deletes.push(path);
                }
            } else {
                keep.push(t);
            }
        }
        if !keep.is_empty() {
            let next = keep.iter().map(|t| t.at + TRASH_HOLD).min();
            self.timers.trash_purge = next;
        }
        self.trash = keep;
    }

    fn retry_pending_deletes(&mut self) {
        self.pending_deletes.retain(|p| std::fs::remove_file(p).is_err() && p.exists());
    }

    pub(crate) fn upsert_folder(&mut self, id: Option<String>, name: String, order: Option<i32>) -> ApiResult<Folder> {
        let name: String = name.split_whitespace().collect::<Vec<_>>().join(" ");
        if name.is_empty() {
            return Err(ApiError::new(ErrorCode::InvalidArgument, "Il nome della cartella non può essere vuoto."));
        }
        let name: String = name.chars().take(40).collect();
        // Due schede con lo stesso nome non si distinguono: il nome è unico, senza badare alle maiuscole.
        let key = name.to_lowercase();
        if self.index.folders.iter().any(|f| Some(f.id.as_str()) != id.as_deref() && f.name.to_lowercase() == key) {
            return Err(ApiError::new(ErrorCode::FolderExists, format!("Esiste già una cartella «{name}».")));
        }
        let folder = match self.index.folders.iter_mut().find(|f| Some(f.id.as_str()) == id.as_deref()) {
            Some(f) => {
                f.name = name;
                if let Some(o) = order {
                    f.order = o;
                }
                f.clone()
            }
            None => {
                // Un ID sconosciuto ricrea la cartella con quell'ID: è l'"Annulla" di un'eliminazione,
                // così preferenze e import in corso che la nominano tornano validi.
                let id = match id {
                    Some(id) if valid_folder_id(&id) => id,
                    Some(_) => return Err(ApiError::new(ErrorCode::InvalidArgument, "ID della cartella non valido.")),
                    None => uuid::Uuid::new_v4().simple().to_string(),
                };
                let order = order.unwrap_or_else(|| self.index.folders.iter().map(|f| f.order + 1).max().unwrap_or(0));
                let f = Folder { id, name, order };
                self.index.folders.push(f.clone());
                f
            }
        };
        self.schedule_index_save();
        self.library_changed(Vec::new(), Vec::new(), Vec::new());
        Ok(folder)
    }

    pub(crate) fn delete_folder(&mut self, folder_id: &str) -> ApiResult<()> {
        let before = self.index.folders.len();
        self.index.folders.retain(|f| f.id != folder_id);
        if before == self.index.folders.len() {
            return Err(ApiError::new(ErrorCode::FolderNotFound, "Cartella non trovata."));
        }
        let mut moved = Vec::new();
        for s in self.index.sounds.iter_mut().filter(|s| s.folder_id.as_deref() == Some(folder_id)) {
            s.folder_id = None;
            moved.push(s.clone());
        }
        self.schedule_index_save();
        self.library_changed(Vec::new(), moved, Vec::new());
        Ok(())
    }

    // ------------------------------------------------------------------ import (SPEC §7.1)

    pub(crate) fn import_files(&mut self, paths: Vec<String>, folder_id: Option<String>) -> ApiResult<u64> {
        if let Some(f) = &folder_id
            && !self.index.folders.iter().any(|x| &x.id == f)
        {
            return Err(ApiError::new(ErrorCode::FolderNotFound, "Cartella non trovata."));
        }
        let files = expand_paths(paths.iter().map(PathBuf::from));
        if files.is_empty() {
            return Err(ApiError::new(ErrorCode::ImportUnsupported, "Nessun file audio da importare."));
        }
        let job_id = self.next_job_id;
        self.next_job_id += 1;
        let known: HashMap<String, String> =
            self.index.sounds.iter().map(|s| (s.source.xxh3.clone(), s.id.clone())).collect();
        let opts = ImportOptions {
            target_lufs: self.settings.loudness.target_lufs as f64,
            ceiling_dbtp: self.settings.loudness.ceiling_dbtp as f64,
            trim_silence: self.settings.loudness.trim_silence,
            max_duration_s: self.settings.library.max_duration_s,
        };
        let pcm_dir = self.paths.pcm_dir();
        let total = files.len() as u32;
        self.jobs.insert(job_id, Job { total, done: 0, errors: Vec::new(), added: Vec::new(), folder_id });
        let (tx, waker) = (self.tx.clone(), self.waker.clone());
        let spawned = std::thread::Builder::new().name("mfx-import".into()).spawn(move || {
            let mut known = known;
            for path in files {
                let result = import_one(&path, &opts, &pcm_dir, &mut known);
                let _ = tx.send(Command::ImportItem { job_id, path, result });
                waker.set();
            }
            let _ = tx.send(Command::ImportDone { job_id });
            waker.set();
        });
        if let Err(e) = spawned {
            self.jobs.remove(&job_id);
            return Err(internal(e));
        }
        self.emit(CoreEvent::ImportProgress(ImportProgress {
            job_id,
            done: 0,
            total,
            current: String::new(),
            errors: Vec::new(),
            added: Vec::new(),
            finished: false,
        }));
        Ok(job_id)
    }

    fn on_import_item(&mut self, job_id: u64, path: PathBuf, result: Result<NewSound, ImportFileError>) {
        let Some(job) = self.jobs.get(&job_id) else { return };
        let folder_id = job.folder_id.clone().filter(|f| self.index.folders.iter().any(|x| &x.id == f));
        let mut added = Vec::new();
        let mut error = None;
        match result {
            Ok(n) => {
                if let Some(existing) = self.index.find_by_hash(&n.source.xxh3) {
                    // Importato nel frattempo da un altro job.
                    let _ = std::fs::remove_file(self.paths.pcm_dir().join(&n.file));
                    error = Some(ImportFileError {
                        path: path.display().to_string(),
                        code: ErrorCode::ImportDuplicate,
                        message: format!("Già nella libreria come \"{}\".", existing.name),
                        existing_sound_id: Some(existing.id.clone()),
                    });
                } else {
                    let sound = Sound {
                        id: n.id,
                        name: n.name,
                        file: n.file,
                        channels: n.channels,
                        frames: n.frames,
                        loudness_gain_db: n.loudness_gain_db,
                        user_gain_db: 0.0,
                        mode: None,
                        mute_voice: false,
                        folder_id,
                        tags: Vec::new(),
                        source: n.source,
                        ui: serde_json::Value::Object(Default::default()),
                        created_at: mfx_library::time::now_rfc3339(),
                    };
                    self.index.sounds.push(sound.clone());
                    added.push(sound);
                }
            }
            Err(mut e) => {
                if e.code == ErrorCode::ImportDuplicate
                    && let Some(id) = &e.existing_sound_id
                    && let Some(s) = self.index.sound(id)
                {
                    e.message = format!("Già nella libreria come \"{}\".", s.name);
                }
                error = Some(e);
            }
        }
        if !added.is_empty() {
            self.schedule_index_save();
            self.library_changed(added.clone(), Vec::new(), Vec::new());
        }
        let job = self.jobs.get_mut(&job_id).expect("job");
        job.done += 1;
        job.added.extend(added.iter().map(|s| s.id.clone()));
        if let Some(e) = error {
            tracing::info!("import {}: {}", e.path, e.message);
            job.errors.push(e);
        }
        let progress = ImportProgress {
            job_id,
            done: job.done,
            total: job.total,
            current: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            errors: job.errors.clone(),
            added: job.added.clone(),
            finished: false,
        };
        self.emit(CoreEvent::ImportProgress(progress));
    }

    fn on_import_done(&mut self, job_id: u64) {
        let Some(job) = self.jobs.remove(&job_id) else { return };
        tracing::info!("import {job_id}: {} aggiunti, {} errori", job.added.len(), job.errors.len());
        self.emit(CoreEvent::ImportProgress(ImportProgress {
            job_id,
            done: job.done,
            total: job.total,
            current: String::new(),
            errors: job.errors,
            added: job.added,
            finished: true,
        }));
    }

    /// Pulizia e recupero all'avvio: file temporanei, `.sbpcm` orfani, indice perso.
    fn recover_orphans(&mut self) {
        let dir = self.paths.pcm_dir();
        let Ok(entries) = std::fs::read_dir(&dir) else { return };
        let known: HashSet<String> = self.index.sounds.iter().map(|s| s.file.clone()).collect();
        let mut orphans = Vec::new();
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.ends_with(".tmp") {
                let _ = std::fs::remove_file(e.path());
            } else if name.ends_with(".sbpcm") && !known.contains(&name) {
                orphans.push(e.path());
            }
        }
        if orphans.is_empty() {
            return;
        }
        if self.index_trusted && !self.index.sounds.is_empty() {
            // Indice valido: gli orfani sono eliminazioni non completate.
            for p in orphans {
                let _ = std::fs::remove_file(p);
            }
            return;
        }
        // Indice perso o recuperato da backup: i file audio tornano in libreria invece di sparire.
        let mut n = 0;
        for p in orphans {
            let Ok(f) = SbPcmFile::open(&p) else { continue };
            let h = f.header();
            let file = p.file_name().unwrap().to_string_lossy().into_owned();
            let id = file.trim_end_matches(".sbpcm").to_string();
            n += 1;
            self.index.sounds.push(Sound {
                id: if self.index.sound(&id).is_none() { id } else { uuid::Uuid::new_v4().simple().to_string() },
                name: format!("Suono recuperato {n}"),
                file,
                channels: h.channels,
                frames: h.frames,
                loudness_gain_db: 0.0,
                user_gain_db: 0.0,
                mode: None,
                mute_voice: false,
                folder_id: None,
                tags: Vec::new(),
                source: SoundSource::default(),
                ui: serde_json::Value::Object(Default::default()),
                created_at: mfx_library::time::now_rfc3339(),
            });
        }
        if n > 0 {
            tracing::warn!("library_recovered: {n} suoni ricostruiti dai file .sbpcm");
            self.notices.push(Notice {
                code: ErrorCode::LibraryRecovered,
                message: format!("Recuperati {n} suoni dalla cartella della libreria. Rinominali per ritrovarli."),
            });
            self.save_index_now();
        }
    }

    fn shutdown(&mut self) {
        self.stop_all();
        // Lascia ai bus il tempo del fade-out (8 ms) prima di chiuderli.
        if self.buses.iter().any(|b| b.bus.is_some()) && !self.plays.is_empty() {
            std::thread::sleep(Duration::from_millis(30));
        }
        self.ptt_release();
        for slot in self.buses.iter_mut() {
            if let Some(b) = slot.bus.take() {
                b.shutdown();
            }
        }
        self.plays.clear();
        self.update_voice_mute();
        self.purge_trash(true);
        self.save_config_now();
        if self.index_trusted || !self.index.sounds.is_empty() {
            self.save_index_now();
        }
        for h in self.hotkeys.iter().filter(|h| h.active) {
            self.platform.unregister_hotkey(h.id);
        }
        self.platform.set_mic_bridge(None);
        tracing::info!("engine fermo");
    }
}

fn playing_of(play_id: u64, p: &PlayState) -> Playing {
    Playing {
        play_id,
        sound_id: p.sound_id.clone(),
        started_at_ms: p.started_at_ms,
        duration_ms: p.duration_ms,
        preview: p.preview,
        buses: p.buses.clone(),
    }
}

fn not_found(id: &str) -> ApiError {
    ApiError::new(ErrorCode::SoundNotFound, format!("Suono non trovato: {id}"))
}

/// ID accettati per ricreare una cartella: quelli generati dal core (UUID semplice) e simili.
fn valid_folder_id(id: &str) -> bool {
    (1..=64).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn internal(e: impl std::fmt::Display) -> ApiError {
    ApiError::new(ErrorCode::Internal, e.to_string())
}

/// File e cartelle trascinati → elenco di file audio (le cartelle si esplorano ricorsivamente).
fn expand_paths(paths: impl Iterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack: Vec<PathBuf> = paths.collect();
    stack.reverse();
    while let Some(p) = stack.pop() {
        if out.len() >= MAX_IMPORT_FILES {
            break;
        }
        if p.is_dir() {
            if let Ok(rd) = std::fs::read_dir(&p) {
                let mut children: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
                children.sort();
                children.reverse();
                stack.extend(children.into_iter().filter(|c| c.is_dir() || mfx_library::is_supported_extension(c)));
            }
        } else {
            out.push(p);
        }
    }
    out
}

fn import_one(
    path: &Path,
    opts: &ImportOptions,
    pcm_dir: &Path,
    known: &mut HashMap<String, String>,
) -> Result<NewSound, ImportFileError> {
    let fail = |code: ErrorCode, message: String| ImportFileError {
        path: path.display().to_string(),
        code,
        message,
        existing_sound_id: None,
    };
    if !mfx_library::is_supported_extension(path) {
        return Err(fail(
            ErrorCode::ImportUnsupported,
            "Formato non supportato. Usa MP3, WAV, FLAC, OGG o M4A.".into(),
        ));
    }
    let hash = mfx_library::hash_file(path).map_err(|e| map_import_error(path, e))?;
    if let Some(existing) = known.get(&hash) {
        return Err(ImportFileError {
            path: path.display().to_string(),
            code: ErrorCode::ImportDuplicate,
            message: "Già nella libreria.".into(),
            existing_sound_id: Some(existing.clone()),
        });
    }
    let id = uuid::Uuid::new_v4().simple().to_string();
    let file = format!("{id}.sbpcm");
    let dst = pcm_dir.join(&file);
    let r = mfx_library::import_to(path, &dst, opts).map_err(|e| {
        let _ = std::fs::remove_file(&dst);
        map_import_error(path, e)
    })?;
    known.insert(hash.clone(), id.clone());
    Ok(NewSound {
        id,
        name: mfx_library::name_from_path(path),
        file,
        channels: r.channels,
        frames: r.frames,
        loudness_gain_db: r.loudness_gain_db,
        source: SoundSource { path: path.display().to_string(), xxh3: hash },
    })
}

fn map_import_error(path: &Path, e: ImportError) -> ImportFileError {
    let (code, message) = match &e {
        ImportError::Unsupported => {
            (ErrorCode::ImportUnsupported, "Formato non supportato o file danneggiato.".to_string())
        }
        ImportError::DecodeFailed(m) => {
            (ErrorCode::ImportDecodeFailed, format!("Impossibile decodificare il file ({m})."))
        }
        ImportError::TooLong { max_s } => (
            ErrorCode::ImportTooLong,
            format!("Il file supera la durata massima di {} minuti.", (*max_s as f32 / 60.0).ceil() as u32),
        ),
        ImportError::Empty => (ErrorCode::ImportEmpty, "Il file non contiene audio.".to_string()),
        ImportError::DiskFull => (ErrorCode::DiskFull, "Spazio su disco esaurito.".to_string()),
        ImportError::NotFound => (ErrorCode::ImportNotFound, "File non trovato.".to_string()),
        ImportError::Io(io) => (ErrorCode::IoError, format!("Errore di lettura: {io}")),
    };
    ImportFileError { path: path.display().to_string(), code, message, existing_sound_id: None }
}
