//! Bus CoreAudio (SPEC §6.2, §6.3 su macOS): un'AUHAL per uscita, che esiste solo quando serve.
//!
//! Stessi stati del bus WASAPI: `Closed` → primo play → `Running` → 2 s di silenzio → `WarmIdle`
//! (unità inizializzata ma ferma, zero CPU) → 60 s senza play → `Closed`.
//!
//! CoreAudio chiama la callback di render sul proprio thread real-time. Lo stato real-time
//! ([`RtState`]: mixer, ring dei comandi e degli eventi) appartiene alla callback mentre l'unità
//! gira e al thread di controllo mentre è ferma: il passaggio è protetto da `gate`/`busy`.
//! Nella callback: nessuna allocazione, nessun lock, nessun log (SPEC §6.5).

use std::ffi::c_void;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;

use rtrb::{Consumer, Producer, PushError, RingBuffer};

use crate::coreaudio::sys::*;
use crate::coreaudio::{self, dispose_unit, float_format, new_hal_unit, set_unit_prop};
use crate::event::{Event, Wait};
use crate::mixer::{Mixer, SoundRef};
pub use crate::sink::BusTiming;
use crate::sink::{BusCmd, BusEvent, BusShared, BusState, BusStats, DeviceId, OutputBus, Wake};
use crate::{CHANNELS, SAMPLE_RATE};

/// Segnali tra callback, listener del HAL e thread di controllo. Solo atomici.
struct Ctl {
    event: Event,
    quit: AtomicBool,
    /// La callback ha visto `idle_stop_ms` di silenzio.
    idle: AtomicBool,
    /// Il dispositivo è sparito (`kAudioDevicePropertyDeviceIsAlive`).
    lost: AtomicBool,
    /// La callback ha ricevuto un `SetDevice`.
    device_changed: AtomicBool,
    /// La callback può usare lo stato real-time.
    gate: AtomicBool,
    /// La callback lo sta usando adesso.
    busy: AtomicBool,
    shared: Arc<BusShared>,
}

pub struct CoreAudioBus {
    tx: Producer<BusCmd>,
    rx: Consumer<BusEvent>,
    ctl: Arc<Ctl>,
    shared: Arc<BusShared>,
    thread: Option<JoinHandle<()>>,
}

impl CoreAudioBus {
    pub fn spawn(
        name: &'static str,
        device: DeviceId,
        gain: f32,
        ceiling_db: f32,
        timing: BusTiming,
        waker: Arc<dyn Wake>,
    ) -> std::io::Result<Self> {
        let (tx, cmd_rx) = RingBuffer::new(256);
        let (evt_tx, rx) = RingBuffer::new(512);
        let shared = BusShared::new(gain);
        let ctl = Arc::new(Ctl {
            event: Event::new()?,
            quit: AtomicBool::new(false),
            idle: AtomicBool::new(false),
            lost: AtomicBool::new(false),
            device_changed: AtomicBool::new(false),
            gate: AtomicBool::new(false),
            busy: AtomicBool::new(false),
            shared: shared.clone(),
        });
        // Pool di voci allocato qui, fuori dal thread audio.
        let rt = Box::new(RtState {
            cmd_rx,
            evt_tx,
            mixer: Mixer::new(shared.gain.clone(), ceiling_db),
            shared: shared.clone(),
            ctl: ctl.clone(),
            waker,
            new_device: None,
            idle_limit: timing.idle_stop_ms as u64 * SAMPLE_RATE as u64 / 1000,
            pushed: false,
        });
        let control = Control { device, timing, ctl: ctl.clone(), rt: Box::into_raw(rt) };
        let thread = std::thread::Builder::new().name(format!("mfx-bus-{name}")).spawn(move || control.run())?;
        Ok(Self { tx, rx, ctl, shared, thread: Some(thread) })
    }

    fn stop_thread(&mut self) {
        self.ctl.quit.store(true, Ordering::Release);
        self.ctl.event.set();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        // Gli Arc rimasti negli eventi muoiono qui, sul thread dell'engine.
        while self.rx.pop().is_ok() {}
    }
}

impl OutputBus for CoreAudioBus {
    #[allow(clippy::result_large_err)]
    fn send(&mut self, cmd: BusCmd) -> Result<(), BusCmd> {
        match self.tx.push(cmd) {
            Ok(()) => {
                self.ctl.event.set();
                Ok(())
            }
            Err(PushError::Full(c)) => Err(c),
        }
    }

    fn drain(&mut self, f: &mut dyn FnMut(BusEvent)) {
        while let Ok(e) = self.rx.pop() {
            f(e);
        }
    }

    fn set_gain(&self, lin: f32) {
        self.shared.gain.store(lin.to_bits(), Ordering::Relaxed);
    }

    fn stats(&self) -> BusStats {
        self.shared.stats()
    }

    fn shutdown(mut self: Box<Self>) {
        self.stop_thread();
    }
}

impl Drop for CoreAudioBus {
    fn drop(&mut self) {
        if self.thread.is_some() {
            self.stop_thread();
        }
    }
}

/// Stato usato dalla callback di render. Tutto preallocato.
struct RtState {
    cmd_rx: Consumer<BusCmd>,
    evt_tx: Producer<BusEvent>,
    mixer: Mixer,
    shared: Arc<BusShared>,
    ctl: Arc<Ctl>,
    waker: Arc<dyn Wake>,
    /// `SetDevice` ricevuto: lo applica il thread di controllo.
    new_device: Option<DeviceId>,
    idle_limit: u64,
    /// Eventi pubblicati nel giro corrente: l'engine si sveglia una volta sola.
    pushed: bool,
}

impl RtState {
    /// Applica i comandi in coda. Nessuna allocazione: si usa anche nella callback.
    fn process_cmds(&mut self) {
        let mut abort = false;
        let Self { cmd_rx, evt_tx, mixer, pushed, new_device, .. } = self;
        let mut ended = |play_id: u64, sound: SoundRef| push_ended(evt_tx, pushed, play_id, sound);
        while let Ok(cmd) = cmd_rx.pop() {
            match cmd {
                BusCmd::Mixer(c) => mixer.apply(c, &mut ended),
                BusCmd::SetDevice(d) => *new_device = Some(d),
                BusCmd::Retry => {}
                BusCmd::Abort => abort = true,
            }
        }
        if abort {
            mixer.kill_all(&mut ended);
        }
    }

    fn push(&mut self, e: BusEvent) {
        if self.evt_tx.push(e).is_ok() {
            self.pushed = true;
        }
    }

    fn flush_wake(&mut self) {
        if self.pushed {
            self.pushed = false;
            self.waker.wake();
        }
    }
}

#[inline]
fn push_ended(tx: &mut Producer<BusEvent>, pushed: &mut bool, play_id: u64, sound: SoundRef) -> Result<(), SoundRef> {
    match tx.push(BusEvent::Ended { play_id, sound }) {
        Ok(()) => {
            *pushed = true;
            Ok(())
        }
        Err(PushError::Full(BusEvent::Ended { sound, .. })) => Err(sound),
        Err(PushError::Full(_)) => unreachable!(),
    }
}

/// Callback di render di CoreAudio (thread real-time del HAL).
unsafe extern "C" fn render_cb(
    ref_con: *mut c_void,
    _flags: *mut u32,
    _ts: *const AudioTimeStamp,
    _bus: u32,
    frames: u32,
    data: *mut AudioBufferList,
) -> OSStatus {
    // SAFETY: `ref_con` è il RtState registrato in Stream::open; vive finché l'unità esiste.
    // `gate`/`busy` garantiscono che il thread di controllo non lo tocchi in questo momento.
    let rt = unsafe { &mut *(ref_con as *mut RtState) };
    // SAFETY: CoreAudio passa un AudioBufferList valido con un buffer interleaved (formato impostato).
    let buf = unsafe { &mut (*data).buffers[0] };
    let samples = (frames as usize * CHANNELS).min(buf.data_size as usize / 4);
    // SAFETY: il buffer di CoreAudio è scrivibile per `data_size` byte.
    let out = unsafe { std::slice::from_raw_parts_mut(buf.data as *mut f32, samples) };

    // SAFETY: il Ctl è tenuto vivo dall'Arc dentro `rt`; il riferimento non prende in prestito `rt`.
    let ctl: &Ctl = unsafe { &*Arc::as_ptr(&rt.ctl) };
    ctl.busy.store(true, Ordering::SeqCst);
    if !ctl.gate.load(Ordering::SeqCst) {
        ctl.busy.store(false, Ordering::SeqCst);
        out.fill(0.0);
        return 0;
    }
    crate::enable_ftz_daz();
    rt.process_cmds();
    {
        let RtState { evt_tx, mixer, pushed, .. } = rt;
        #[cfg(feature = "rt-checks")]
        assert_no_alloc::assert_no_alloc(|| {
            mixer.render(out, &mut |play_id, sound| push_ended(evt_tx, pushed, play_id, sound))
        });
        #[cfg(not(feature = "rt-checks"))]
        mixer.render(out, &mut |play_id, sound| push_ended(evt_tx, pushed, play_id, sound));
    }
    rt.shared.blocks.fetch_add(1, Ordering::Relaxed);
    rt.shared.voices.store(rt.mixer.active_voices(), Ordering::Relaxed);
    let mut wake_ctl = false;
    if rt.new_device.is_some() && !ctl.device_changed.swap(true, Ordering::AcqRel) {
        wake_ctl = true;
    }
    if rt.mixer.idle_frames() >= rt.idle_limit && !ctl.idle.swap(true, Ordering::AcqRel) {
        wake_ctl = true;
    }
    rt.flush_wake();
    ctl.busy.store(false, Ordering::SeqCst);
    if wake_ctl {
        ctl.event.set();
    }
    0
}

/// Listener del HAL sul dispositivo aperto (thread di notifica di CoreAudio).
unsafe extern "C" fn device_listener(
    _id: AudioObjectID,
    n: u32,
    addrs: *const AudioObjectPropertyAddress,
    client: *mut c_void,
) -> OSStatus {
    // SAFETY: `client` è il Ctl tenuto vivo dallo Stream che ha registrato il listener.
    let ctl = unsafe { &*(client as *const Ctl) };
    // SAFETY: CoreAudio passa `n` indirizzi validi.
    let addrs = unsafe { std::slice::from_raw_parts(addrs, n as usize) };
    for a in addrs {
        match a.selector {
            DEVICE_PROCESSOR_OVERLOAD => {
                ctl.shared.underruns.fetch_add(1, Ordering::Relaxed);
            }
            DEVICE_IS_ALIVE => {
                ctl.lost.store(true, Ordering::Release);
                ctl.event.set();
            }
            _ => {}
        }
    }
    0
}

const LISTENED: [u32; 2] = [DEVICE_IS_ALIVE, DEVICE_PROCESSOR_OVERLOAD];

enum OpenError {
    Busy,
    Failed(OSStatus),
}

/// AUHAL aperta su un dispositivo, inizializzata (ferma o in esecuzione).
struct Stream {
    unit: AudioUnit,
    device: AudioObjectID,
    ctl: Arc<Ctl>,
}

impl Stream {
    fn open(device: &DeviceId, rt: *mut RtState, ctl: &Arc<Ctl>) -> Result<(Self, u32), OpenError> {
        let dev = if device.is_default() {
            coreaudio::default_device(true)
        } else {
            coreaudio::device_by_uid(&device.to_string_lossy())
        }
        .ok_or(OpenError::Failed(ERR_BAD_DEVICE))?;
        if coreaudio::hogged_by_other(dev) {
            return Err(OpenError::Busy);
        }
        let unit = new_hal_unit().map_err(OpenError::Failed)?;
        // Da qui in poi `s` distrugge l'unità anche in caso di errore.
        let s = Self { unit, device: dev, ctl: ctl.clone() };
        set_unit_prop(unit, AU_PROP_CURRENT_DEVICE, AU_SCOPE_GLOBAL, 0, &dev).map_err(OpenError::Failed)?;
        // Il formato si imposta sul lato "input" dell'elemento di uscita: CoreAudio converte verso il
        // formato del dispositivo (frequenza compresa). La UI segnala `rate_mismatch` se diverso.
        let fmt = float_format(SAMPLE_RATE as f64, CHANNELS as u32);
        set_unit_prop(unit, AU_PROP_STREAM_FORMAT, AU_SCOPE_INPUT, 0, &fmt).map_err(OpenError::Failed)?;
        let cb = AURenderCallbackStruct { input_proc: render_cb, input_proc_ref_con: rt.cast() };
        set_unit_prop(unit, AU_PROP_SET_RENDER_CALLBACK, AU_SCOPE_INPUT, 0, &cb).map_err(OpenError::Failed)?;
        // SAFETY: unità configurata.
        let r = unsafe { AudioUnitInitialize(unit) };
        if r != 0 {
            return Err(OpenError::Failed(r));
        }
        for sel in LISTENED {
            let addr = AudioObjectPropertyAddress { selector: sel, scope: SCOPE_GLOBAL, element: ELEMENT_MAIN };
            // SAFETY: il Ctl resta vivo finché lo Stream (che ne tiene un Arc) rimuove il listener.
            unsafe { AudioObjectAddPropertyListener(dev, &addr, device_listener, Arc::as_ptr(ctl) as *mut c_void) };
        }
        let rate = coreaudio::nominal_rate(dev).unwrap_or(0.0).round() as u32;
        Ok((s, rate))
    }

    fn start(&self) -> Result<(), OSStatus> {
        self.ctl.gate.store(true, Ordering::SeqCst);
        // SAFETY: unità inizializzata.
        let r = unsafe { AudioOutputUnitStart(self.unit) };
        if r == 0 {
            Ok(())
        } else {
            self.ctl.gate.store(false, Ordering::SeqCst);
            Err(r)
        }
    }

    /// Ferma l'unità e attende che la callback abbia finito: dopo, lo stato real-time è del chiamante.
    fn stop(&self) {
        self.ctl.gate.store(false, Ordering::SeqCst);
        // SAFETY: unità inizializzata.
        unsafe { AudioOutputUnitStop(self.unit) };
        while self.ctl.busy.load(Ordering::SeqCst) {
            std::hint::spin_loop();
        }
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        self.stop();
        for sel in LISTENED {
            let addr = AudioObjectPropertyAddress { selector: sel, scope: SCOPE_GLOBAL, element: ELEMENT_MAIN };
            // SAFETY: stessi parametri della registrazione.
            unsafe {
                AudioObjectRemovePropertyListener(
                    self.device,
                    &addr,
                    device_listener,
                    Arc::as_ptr(&self.ctl) as *mut c_void,
                )
            };
        }
        // SAFETY: unità creata in open() e non più usata.
        unsafe { dispose_unit(self.unit) };
    }
}

/// Thread di controllo: apre, avvia, mette in pausa e chiude l'unità. Mai nel percorso audio.
struct Control {
    device: DeviceId,
    timing: BusTiming,
    ctl: Arc<Ctl>,
    rt: *mut RtState,
}

// SAFETY: `rt` passa al thread di controllo insieme alla struttura e non è condiviso altrove
// se non con la callback, con l'accesso regolato da `gate`/`busy`.
unsafe impl Send for Control {}

impl Control {
    fn run(mut self) {
        let mut stream: Option<Stream> = None;
        let mut state = BusState::Closed;
        loop {
            self.ctl.shared.state.store(state as u8, Ordering::Relaxed);
            let timeout = (state == BusState::WarmIdle).then_some(self.timing.warm_hold_ms);
            let wait = self.ctl.event.wait(timeout);
            if self.ctl.quit.load(Ordering::Acquire) || matches!(wait, Wait::Failed) {
                break;
            }
            if matches!(wait, Wait::Timeout) {
                if state == BusState::WarmIdle {
                    stream = None;
                    state = BusState::Closed;
                }
                continue;
            }
            if state == BusState::Running {
                let lost = self.ctl.lost.swap(false, Ordering::AcqRel);
                let changed = self.ctl.device_changed.swap(false, Ordering::AcqRel);
                let idle = self.ctl.idle.load(Ordering::Acquire);
                if !(lost || changed || idle) {
                    // Solo comandi nuovi: li applica la callback al prossimo blocco.
                    continue;
                }
                if let Some(s) = stream.as_ref() {
                    s.stop();
                }
                state = BusState::WarmIdle;
                if lost {
                    stream = None;
                    state = BusState::Closed;
                    self.ctl.shared.errors.fetch_add(1, Ordering::Relaxed);
                    self.rt().push(BusEvent::Lost { hresult: ERR_BAD_DEVICE });
                }
            }
            self.ctl.idle.store(false, Ordering::Release);
            self.ctl.lost.store(false, Ordering::Release);
            // Unità ferma: lo stato real-time è di questo thread.
            self.rt().process_cmds();
            // SetDevice riapre sempre: serve anche per "segui il predefinito" quando cambia il predefinito.
            if let Some(d) = self.rt().new_device.take() {
                self.device = d;
                stream = None;
                state = BusState::Closed;
            }
            self.ctl.device_changed.store(false, Ordering::Release);
            let voices = self.rt().mixer.active_voices();
            self.ctl.shared.voices.store(voices, Ordering::Relaxed);
            if self.rt().mixer.has_voices() {
                self.start(&mut stream, &mut state);
            }
            self.rt().flush_wake();
        }
        drop(stream);
        self.ctl.shared.state.store(BusState::Closed as u8, Ordering::Relaxed);
        // SAFETY: unità distrutta, la callback non può più girare; Box creato in spawn().
        drop(unsafe { Box::from_raw(self.rt) });
    }

    fn rt(&mut self) -> &mut RtState {
        // SAFETY: chiamato solo a unità ferma (vedi run()).
        unsafe { &mut *self.rt }
    }

    fn start(&mut self, stream: &mut Option<Stream>, state: &mut BusState) {
        if stream.is_none() {
            match Stream::open(&self.device, self.rt, &self.ctl) {
                Ok((s, mix_rate)) => {
                    self.ctl.shared.mix_rate.store(mix_rate, Ordering::Relaxed);
                    self.rt().push(BusEvent::Opened { mix_rate });
                    *stream = Some(s);
                }
                Err(e) => {
                    self.ctl.shared.errors.fetch_add(1, Ordering::Relaxed);
                    let ev = match e {
                        OpenError::Busy => BusEvent::Busy,
                        OpenError::Failed(hr) => BusEvent::Lost { hresult: hr },
                    };
                    self.rt().push(ev);
                    *state = BusState::Closed;
                    return;
                }
            }
        }
        let s = stream.as_ref().expect("stream aperto");
        match s.start() {
            Ok(()) => *state = BusState::Running,
            Err(hr) => {
                *stream = None;
                *state = BusState::Closed;
                self.ctl.shared.errors.fetch_add(1, Ordering::Relaxed);
                self.rt().push(BusEvent::Lost { hresult: hr });
            }
        }
    }
}
