//! Bridge del microfono su Windows (SPEC §6.6 "Bridge"): MicFX cattura la voce e la manda in CABLE Input.
//!
//! Serve alla riduzione del rumore: con "Ascolta questo dispositivo" Windows inoltra il microfono da
//! sé e l'app non lo può filtrare. Un thread MMCSS con due stream WASAPI shared/event-driven:
//!
//! - cattura sul microfono in float mono 48 kHz (Windows converte frequenza e canali);
//! - riproduzione su CABLE Input in float stereo 48 kHz, accanto al bus dei suoni (Windows somma i
//!   due stream: i suoni non passano dal filtro).
//!
//! La voce passa da un ring `rtrb` tra i due eventi ([`VoiceOut`] tiene la latenza a 20 ms). Nel
//! thread: niente allocazioni, lock, log o I/O (SPEC §6.5); un errore del dispositivo chiude il
//! bridge e [`MicBridge::is_alive`] lo dice all'engine.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{SyncSender, sync_channel};
use std::thread::JoinHandle;

use rtrb::Producer;
use windows::Win32::Media::Audio::{
    AUDCLNT_BUFFERFLAGS_SILENT, IAudioCaptureClient, IAudioClient, IAudioRenderClient, eCapture, eRender,
};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::System::Threading::{AvRevertMmThreadCharacteristics, AvSetMmThreadCharacteristicsW};
use windows::core::w;

use crate::CHANNELS;
use crate::bus::open_client;
use crate::event::{Event, Wait, wait_any};
use crate::sink::DeviceId;
use crate::voice::{self, Denoise, VoiceOut};

/// Silenzio per i pacchetti che il driver segna come muti.
static ZEROS: [f32; 1024] = [0.0; 1024];

pub struct MicBridge {
    stop: Arc<Event>,
    alive: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl MicBridge {
    /// Avvia il bridge. `mic_id = None` = microfono predefinito di Windows. Con `muted` a `true` la
    /// voce non passa (rampa di 10 ms); con `denoise` passa dalla riduzione del rumore.
    pub fn start(mic_id: Option<&str>, cable_id: &str, denoise: bool, muted: Arc<AtomicBool>) -> Result<Self, String> {
        let mic = match mic_id {
            Some(id) => DeviceId::new(id).ok_or("ID del microfono troppo lungo")?,
            None => DeviceId::DEFAULT,
        };
        let cable = DeviceId::new(cable_id).ok_or("ID del cavo troppo lungo")?;
        let stop = Arc::new(Event::new().map_err(|e| e.to_string())?);
        let alive = Arc::new(AtomicBool::new(true));
        // La rete si costruisce qui (alloca), prima del thread audio.
        let denoise = denoise.then(Denoise::new);
        let (tx, rx) = voice::ring();
        let (ready_tx, ready_rx) = sync_channel(1);
        let ctx =
            Bridge { mic, cable, denoise, tx, out: VoiceOut::new(rx, muted), stop: stop.clone(), alive: alive.clone() };
        let thread = std::thread::Builder::new()
            .name("mfx-voice".into())
            .spawn(move || ctx.run(ready_tx))
            .map_err(|e| e.to_string())?;
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self { stop, alive, thread: Some(thread) }),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => Err("thread del bridge terminato".into()),
        }
    }

    /// `false` dopo un errore del dispositivo (scollegato, formato cambiato): va riaperto.
    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }
}

impl Drop for MicBridge {
    fn drop(&mut self) {
        self.stop.set();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

struct Bridge {
    mic: DeviceId,
    cable: DeviceId,
    denoise: Option<Denoise>,
    tx: Producer<f32>,
    out: VoiceOut,
    stop: Arc<Event>,
    alive: Arc<AtomicBool>,
}

/// I due stream; i client si chiudono prima degli eventi che usano (ordine dei campi).
struct Streams {
    capture_client: IAudioClient,
    capture: IAudioCaptureClient,
    render_client: IAudioClient,
    render: IAudioRenderClient,
    render_frames: u32,
    capture_event: Event,
    render_event: Event,
}

impl Bridge {
    fn run(mut self, ready: SyncSender<Result<(), String>>) {
        // SAFETY: inizializzazione COM del thread, bilanciata da CoUninitialize in fondo.
        let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let mut task_index = 0u32;
        // SAFETY: nome di task MMCSS statico; il handle viene revocato in fondo.
        let mmcss = unsafe { AvSetMmThreadCharacteristicsW(w!("Pro Audio"), &mut task_index) }.ok();
        crate::enable_ftz_daz();

        // SAFETY: COM inizializzato su questo thread.
        match unsafe { self.open() } {
            Ok(s) => {
                let _ = ready.send(Ok(()));
                // SAFETY: stream aperti su questo thread.
                let _ = unsafe { self.pump(&s) };
                // SAFETY: client inizializzati.
                unsafe {
                    let _ = s.capture_client.Stop();
                    let _ = s.render_client.Stop();
                }
            }
            Err(e) => {
                let _ = ready.send(Err(e));
            }
        }
        self.alive.store(false, Ordering::Release);
        if let Some(h) = mmcss {
            // SAFETY: handle restituito da AvSetMmThreadCharacteristicsW.
            let _ = unsafe { AvRevertMmThreadCharacteristics(h) };
        }
        if com.is_ok() {
            // SAFETY: bilancia CoInitializeEx riuscito.
            unsafe { CoUninitialize() };
        }
    }

    unsafe fn open(&self) -> Result<Streams, String> {
        let capture_event = Event::new().map_err(|e| e.to_string())?;
        let render_event = Event::new().map_err(|e| e.to_string())?;
        // SAFETY: COM inizializzato; gli eventi vivono quanto gli Streams.
        unsafe {
            let (render_client, _) = open_client(&self.cable, eRender, CHANNELS as u16, render_event.handle())
                .map_err(|e| format!("uscita sul cavo: {e}"))?;
            let (capture_client, _) = open_client(&self.mic, eCapture, 1, capture_event.handle())
                .map_err(|e| format!("cattura del microfono: {e}"))?;
            start(capture_client, render_client, capture_event, render_event)
                .map_err(|e| format!("avvio del bridge: {e}"))
        }
    }

    /// Ciclo del thread: un evento per stream più quello di stop. Esce allo stop o al primo errore.
    unsafe fn pump(&mut self, s: &Streams) -> windows::core::Result<()> {
        let handles = [s.capture_event.handle(), s.render_event.handle(), self.stop.handle()];
        loop {
            match wait_any(&handles, None) {
                // SAFETY: stream aperti su questo thread.
                Wait::Signaled(0) => unsafe { self.capture(s)? },
                // SAFETY: come sopra.
                Wait::Signaled(1) => unsafe { render(s, &mut self.out)? },
                _ => return Ok(()),
            }
        }
    }

    /// Legge tutti i pacchetti pronti e li passa (filtrati, se serve) al ring della voce.
    unsafe fn capture(&mut self, s: &Streams) -> windows::core::Result<()> {
        let Self { denoise, tx, .. } = self;
        let mut emit = |f: &[f32]| voice::push(tx, f);
        // SAFETY: interfacce valide; il buffer di GetBuffer è leggibile per `frames` campioni mono
        // fino a ReleaseBuffer.
        unsafe {
            while s.capture.GetNextPacketSize()? > 0 {
                let mut data = std::ptr::null_mut();
                let mut frames = 0u32;
                let mut flags = 0u32;
                s.capture.GetBuffer(&mut data, &mut frames, &mut flags, None, None)?;
                let n = frames as usize;
                if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || data.is_null() {
                    for k in (0..n).step_by(ZEROS.len()) {
                        feed(denoise, &ZEROS[..(n - k).min(ZEROS.len())], &mut emit);
                    }
                } else {
                    feed(denoise, std::slice::from_raw_parts(data as *const f32, n), &mut emit);
                }
                s.capture.ReleaseBuffer(frames)?;
            }
        }
        Ok(())
    }
}

/// Prepara e avvia i due stream; il buffer d'uscita parte pieno di silenzio, come nei bus dei suoni.
unsafe fn start(
    capture_client: IAudioClient,
    render_client: IAudioClient,
    capture_event: Event,
    render_event: Event,
) -> windows::core::Result<Streams> {
    // SAFETY: client inizializzati su questo thread.
    unsafe {
        let render_frames = render_client.GetBufferSize()?;
        let render: IAudioRenderClient = render_client.GetService()?;
        let capture: IAudioCaptureClient = capture_client.GetService()?;
        render.GetBuffer(render_frames)?;
        render.ReleaseBuffer(render_frames, AUDCLNT_BUFFERFLAGS_SILENT.0 as u32)?;
        render_client.Start()?;
        capture_client.Start()?;
        Ok(Streams { capture_client, capture, render_client, render, render_frames, capture_event, render_event })
    }
}

#[inline]
fn feed(denoise: &mut Option<Denoise>, samples: &[f32], emit: &mut impl FnMut(&[f32])) {
    match denoise {
        Some(d) => d.process(samples, emit),
        None => emit(samples),
    }
}

/// Riempie lo spazio libero del buffer del cavo con la voce.
unsafe fn render(s: &Streams, out: &mut VoiceOut) -> windows::core::Result<()> {
    // SAFETY: interfacce valide; il buffer di GetBuffer è scrivibile per `free` frame.
    unsafe {
        let padding = s.render_client.GetCurrentPadding()?;
        let free = s.render_frames.saturating_sub(padding);
        if free == 0 {
            return Ok(());
        }
        let ptr = s.render.GetBuffer(free)?;
        out.render(std::slice::from_raw_parts_mut(ptr as *mut f32, free as usize * CHANNELS));
        s.render.ReleaseBuffer(free, 0)
    }
}
