//! Bus WASAPI (SPEC §6.2, §6.3): uno stream shared/event-driven per uscita, che esiste solo quando serve.
//!
//! Stati: `Closed` (nessun client) → primo play → `Running` → 2 s di silenzio → `WarmIdle` (client
//! inizializzato ma fermo, zero CPU) → 60 s senza play → `Closed`. Un dispositivo invalidato chiude il
//! bus e l'engine riceve `Lost`.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;

use rtrb::{Consumer, Producer, PushError, RingBuffer};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Media::Audio::{
    AUDCLNT_E_DEVICE_IN_USE, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
    AUDCLNT_STREAMFLAGS_EVENTCALLBACK, AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, IAudioClient, IAudioRenderClient,
    IMMDeviceEnumerator, MMDeviceEnumerator, WAVEFORMATEX, WAVEFORMATEXTENSIBLE, WAVEFORMATEXTENSIBLE_0, eConsole,
    eRender,
};
use windows::Win32::Media::KernelStreaming::{SPEAKER_FRONT_LEFT, SPEAKER_FRONT_RIGHT, WAVE_FORMAT_EXTENSIBLE};
use windows::Win32::Media::Multimedia::KSDATAFORMAT_SUBTYPE_IEEE_FLOAT;
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize,
};
use windows::Win32::System::Threading::{AvRevertMmThreadCharacteristics, AvSetMmThreadCharacteristicsW};
use windows::core::{PCWSTR, w};

use crate::event::{Event, Wait, wait_any};
use crate::mixer::{Mixer, SoundRef};
pub use crate::sink::BusTiming;
use crate::sink::{BusCmd, BusEvent, BusShared, BusState, BusStats, DeviceId, OutputBus, Wake};
use crate::{CHANNELS, SAMPLE_RATE};

/// Durata del buffer WASAPI richiesta: 20 ms (il periodo resta quello di default, ~10 ms).
const BUFFER_HNS: i64 = 200_000;

pub struct WasapiBus {
    tx: Producer<BusCmd>,
    rx: Consumer<BusEvent>,
    cmd_event: Arc<Event>,
    quit: Arc<AtomicBool>,
    shared: Arc<BusShared>,
    thread: Option<JoinHandle<()>>,
}

impl WasapiBus {
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
        let cmd_event = Arc::new(Event::new()?);
        let quit = Arc::new(AtomicBool::new(false));
        let shared = BusShared::new(gain);
        // Pool di voci allocato qui, fuori dal thread audio.
        let mixer = Mixer::new(shared.gain.clone(), ceiling_db);
        let ctx = RenderThread {
            device,
            cmd_rx,
            evt_tx,
            cmd_event: cmd_event.clone(),
            quit: quit.clone(),
            shared: shared.clone(),
            waker,
            mixer,
            timing,
            pushed: false,
        };
        let thread = std::thread::Builder::new().name(format!("mfx-bus-{name}")).spawn(move || ctx.run())?;
        Ok(Self { tx, rx, cmd_event, quit, shared, thread: Some(thread) })
    }

    fn stop_thread(&mut self) {
        self.quit.store(true, Ordering::Release);
        self.cmd_event.set();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        // Gli Arc rimasti negli eventi muoiono qui, sul thread dell'engine.
        while self.rx.pop().is_ok() {}
    }
}

impl OutputBus for WasapiBus {
    #[allow(clippy::result_large_err)]
    fn send(&mut self, cmd: BusCmd) -> Result<(), BusCmd> {
        match self.tx.push(cmd) {
            Ok(()) => {
                self.cmd_event.set();
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

impl Drop for WasapiBus {
    fn drop(&mut self) {
        if self.thread.is_some() {
            self.stop_thread();
        }
    }
}

struct Stream {
    client: IAudioClient,
    render: IAudioRenderClient,
    buffer_frames: u32,
}

struct RenderThread {
    device: DeviceId,
    cmd_rx: Consumer<BusCmd>,
    evt_tx: Producer<BusEvent>,
    cmd_event: Arc<Event>,
    quit: Arc<AtomicBool>,
    shared: Arc<BusShared>,
    waker: Arc<dyn Wake>,
    mixer: Mixer,
    timing: BusTiming,
    /// Eventi pubblicati nel giro corrente: l'engine si sveglia una volta sola.
    pushed: bool,
}

impl RenderThread {
    fn run(mut self) {
        // SAFETY: inizializzazione COM del thread, bilanciata da CoUninitialize in fondo.
        let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let mut task_index = 0u32;
        // SAFETY: nome di task MMCSS statico; il handle viene revocato in fondo.
        let mmcss = unsafe { AvSetMmThreadCharacteristicsW(w!("Pro Audio"), &mut task_index) }.ok();
        crate::enable_ftz_daz();

        let buffer_event = match Event::new() {
            Ok(e) => e,
            Err(_) => return,
        };
        let mut stream: Option<Stream> = None;
        let mut state = BusState::Closed;
        let idle_frames = self.timing.idle_stop_ms as u64 * SAMPLE_RATE as u64 / 1000;

        loop {
            self.set_state(state);
            let timeout = (state == BusState::WarmIdle).then_some(self.timing.warm_hold_ms);
            let handles = [buffer_event.handle(), self.cmd_event.handle()];
            let wait = wait_any(&handles, timeout);
            if self.quit.load(Ordering::Acquire) {
                break;
            }
            match wait {
                Wait::Signaled(0) => {
                    if state == BusState::Running
                        && let Some(s) = stream.as_ref()
                    {
                        match self.render_period(s, false) {
                            Ok(()) => {
                                if self.mixer.idle_frames() >= idle_frames {
                                    // Pausa calda: client fermo, nessun evento, zero CPU.
                                    // SAFETY: client inizializzato.
                                    unsafe {
                                        let _ = s.client.Stop();
                                        let _ = s.client.Reset();
                                    }
                                    state = BusState::WarmIdle;
                                }
                            }
                            Err(hr) => {
                                self.lost(&mut stream, &mut state, hr);
                            }
                        }
                    }
                }
                Wait::Signaled(_) => self.handle_cmds(&mut stream, &mut state, buffer_event.handle()),
                Wait::Timeout => {
                    if state == BusState::WarmIdle {
                        stream = None;
                        state = BusState::Closed;
                    }
                }
                Wait::Failed => break,
            }
            self.flush_wake();
        }

        if let Some(s) = stream.take() {
            // SAFETY: client inizializzato.
            unsafe {
                let _ = s.client.Stop();
            }
        }
        self.set_state(BusState::Closed);
        if let Some(h) = mmcss {
            // SAFETY: handle restituito da AvSetMmThreadCharacteristicsW.
            let _ = unsafe { AvRevertMmThreadCharacteristics(h) };
        }
        if com.is_ok() {
            // SAFETY: bilancia CoInitializeEx riuscito.
            unsafe { CoUninitialize() };
        }
        // Thread in uscita: i drop qui non disturbano più nessuno stream.
    }

    fn set_state(&self, s: BusState) {
        self.shared.state.store(s as u8, Ordering::Relaxed);
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

    fn lost(&mut self, stream: &mut Option<Stream>, state: &mut BusState, hr: i32) {
        *stream = None;
        *state = BusState::Closed;
        self.shared.errors.fetch_add(1, Ordering::Relaxed);
        self.push(BusEvent::Lost { hresult: hr });
    }

    fn handle_cmds(&mut self, stream: &mut Option<Stream>, state: &mut BusState, buffer_event: HANDLE) {
        let mut new_device = None;
        let mut abort = false;
        {
            let Self { cmd_rx, evt_tx, mixer, pushed, .. } = self;
            let mut ended = |play_id: u64, sound: SoundRef| match evt_tx.push(BusEvent::Ended { play_id, sound }) {
                Ok(()) => {
                    *pushed = true;
                    Ok(())
                }
                Err(PushError::Full(BusEvent::Ended { sound, .. })) => Err(sound),
                Err(PushError::Full(_)) => unreachable!(),
            };
            while let Ok(cmd) = cmd_rx.pop() {
                match cmd {
                    BusCmd::Mixer(c) => mixer.apply(c, &mut ended),
                    BusCmd::SetDevice(d) => new_device = Some(d),
                    BusCmd::Retry => {}
                    BusCmd::Abort => abort = true,
                }
            }
            if abort {
                mixer.kill_all(&mut ended);
            }
        }
        // SetDevice riapre sempre: serve anche per "segui il predefinito" quando cambia il predefinito.
        if let Some(d) = new_device {
            self.device = d;
            if let Some(s) = stream.take() {
                // SAFETY: client inizializzato.
                unsafe {
                    let _ = s.client.Stop();
                }
            }
            *state = BusState::Closed;
        }
        self.shared.voices.store(self.mixer.active_voices(), Ordering::Relaxed);
        if *state == BusState::Running || !self.mixer.has_voices() {
            return;
        }
        if stream.is_none() {
            // SAFETY: COM inizializzato su questo thread; evento valido.
            match unsafe { open_stream(&self.device, buffer_event) } {
                Ok((s, mix_rate)) => {
                    self.shared.mix_rate.store(mix_rate, Ordering::Relaxed);
                    self.shared.buffer_frames.store(s.buffer_frames, Ordering::Relaxed);
                    self.push(BusEvent::Opened { mix_rate });
                    *stream = Some(s);
                }
                Err(e) => {
                    self.shared.errors.fetch_add(1, Ordering::Relaxed);
                    if e.code() == AUDCLNT_E_DEVICE_IN_USE {
                        self.push(BusEvent::Busy);
                    } else {
                        self.push(BusEvent::Lost { hresult: e.code().0 });
                    }
                    return;
                }
            }
        }
        let s = stream.as_ref().expect("stream aperto");
        // Prefill dell'intero buffer prima di Start(): il primo periodo non è silenzio.
        let started = self.render_period(s, true).and_then(|()| {
            // SAFETY: client inizializzato.
            unsafe { s.client.Start() }.map_err(|e| e.code().0)
        });
        match started {
            Ok(()) => *state = BusState::Running,
            Err(hr) => self.lost(stream, state, hr),
        }
    }

    /// Riempie lo spazio libero del buffer WASAPI. Nessuna allocazione.
    /// `prefill`: buffer vuoto prima di `Start()`, non è un underrun.
    fn render_period(&mut self, s: &Stream, prefill: bool) -> Result<(), i32> {
        // SAFETY: interfacce valide; il buffer restituito da GetBuffer è scrivibile per `free` frame.
        unsafe {
            let padding = s.client.GetCurrentPadding().map_err(|e| e.code().0)?;
            let free = s.buffer_frames.saturating_sub(padding);
            if free == 0 {
                return Ok(());
            }
            if !prefill && padding == 0 && self.mixer.active_voices() > 0 {
                self.shared.underruns.fetch_add(1, Ordering::Relaxed);
            }
            let ptr = s.render.GetBuffer(free).map_err(|e| e.code().0)?;
            let out = std::slice::from_raw_parts_mut(ptr as *mut f32, free as usize * CHANNELS);
            {
                let Self { evt_tx, mixer, pushed, .. } = self;
                #[cfg(feature = "rt-checks")]
                assert_no_alloc::assert_no_alloc(|| {
                    mixer.render(out, &mut |play_id, sound| push_ended(evt_tx, pushed, play_id, sound))
                });
                #[cfg(not(feature = "rt-checks"))]
                mixer.render(out, &mut |play_id, sound| push_ended(evt_tx, pushed, play_id, sound));
            }
            s.render.ReleaseBuffer(free, 0).map_err(|e| e.code().0)?;
        }
        self.shared.blocks.fetch_add(1, Ordering::Relaxed);
        self.shared.voices.store(self.mixer.active_voices(), Ordering::Relaxed);
        Ok(())
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

/// Apre l'endpoint in shared mode event-driven con formato fisso float/48 kHz/stereo.
unsafe fn open_stream(device: &DeviceId, buffer_event: HANDLE) -> windows::core::Result<(Stream, u32)> {
    unsafe {
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let dev = if device.is_default() {
            enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?
        } else {
            enumerator.GetDevice(PCWSTR(device.as_wide_nul().as_ptr()))?
        };
        let client: IAudioClient = dev.Activate(CLSCTX_ALL, None)?;
        let mix = client.GetMixFormat()?;
        let mix_rate = (*mix).nSamplesPerSec;
        CoTaskMemFree(Some(mix as *const _));

        let block_align = (CHANNELS * 4) as u16;
        let fmt = WAVEFORMATEXTENSIBLE {
            Format: WAVEFORMATEX {
                wFormatTag: WAVE_FORMAT_EXTENSIBLE as u16,
                nChannels: CHANNELS as u16,
                nSamplesPerSec: SAMPLE_RATE,
                nAvgBytesPerSec: SAMPLE_RATE * block_align as u32,
                nBlockAlign: block_align,
                wBitsPerSample: 32,
                cbSize: (size_of::<WAVEFORMATEXTENSIBLE>() - size_of::<WAVEFORMATEX>()) as u16,
            },
            Samples: WAVEFORMATEXTENSIBLE_0 { wValidBitsPerSample: 32 },
            dwChannelMask: SPEAKER_FRONT_LEFT | SPEAKER_FRONT_RIGHT,
            SubFormat: KSDATAFORMAT_SUBTYPE_IEEE_FLOAT,
        };
        // Se il mix format dell'endpoint è diverso, Windows converte (AUTOCONVERTPCM) e la UI
        // segnala `rate_mismatch`.
        let flags = AUDCLNT_STREAMFLAGS_EVENTCALLBACK
            | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
            | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY;
        client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            flags,
            BUFFER_HNS,
            0,
            &fmt as *const WAVEFORMATEXTENSIBLE as *const WAVEFORMATEX,
            None,
        )?;
        client.SetEventHandle(buffer_event)?;
        let buffer_frames = client.GetBufferSize()?;
        let render: IAudioRenderClient = client.GetService()?;
        Ok((Stream { client, render, buffer_frames }, mix_rate))
    }
}
