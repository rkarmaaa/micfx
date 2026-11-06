//! Interfaccia tra engine e bus (SPEC §13 `sink.rs`): bus WASAPI reale e bus offline per i test.

use std::sync::atomic::{AtomicU8, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::mixer::{Mixer, MixerCmd, SoundRef};

/// Risveglio dell'engine (evento Win32 nel codice reale). Chiamabile dal thread audio: niente alloc.
pub trait Wake: Send + Sync {
    fn wake(&self);
}

/// ID di endpoint a dimensione fissa: si può mandare al thread audio senza allocare.
/// Lunghezza 0 = dispositivo di riproduzione predefinito.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct DeviceId {
    buf: [u16; Self::CAP + 1],
    len: u16,
}

impl DeviceId {
    /// Gli ID degli endpoint (`{0.0.0.00000000}.{guid}`) sono lunghi 55 caratteri.
    pub const CAP: usize = 95;
    pub const DEFAULT: Self = Self { buf: [0; Self::CAP + 1], len: 0 };

    pub fn new(id: &str) -> Option<Self> {
        let mut buf = [0u16; Self::CAP + 1];
        let mut len = 0usize;
        for u in id.encode_utf16() {
            if len == Self::CAP {
                return None;
            }
            buf[len] = u;
            len += 1;
        }
        Some(Self { buf, len: len as u16 })
    }

    pub fn from_option(id: Option<&str>) -> Self {
        id.and_then(Self::new).unwrap_or(Self::DEFAULT)
    }

    pub fn is_default(&self) -> bool {
        self.len == 0
    }

    /// UTF-16 terminato da NUL, pronto per `PCWSTR`.
    pub fn as_wide_nul(&self) -> &[u16] {
        &self.buf[..self.len as usize + 1]
    }
}

impl std::fmt::Debug for DeviceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_default() {
            write!(f, "DeviceId(default)")
        } else {
            write!(f, "DeviceId({})", String::from_utf16_lossy(&self.buf[..self.len as usize]))
        }
    }
}

pub enum BusCmd {
    Mixer(MixerCmd),
    /// Cambia endpoint conservando le voci (cuffie scollegate, nuovo predefinito, scelta utente).
    SetDevice(DeviceId),
    /// Riprova ad aprire il dispositivo dopo un `Lost` (backoff deciso dall'engine).
    Retry,
    /// Chiude tutte le voci subito (dispositivo irrecuperabile).
    Abort,
}

pub enum BusEvent {
    /// Voce finita: l'Arc torna all'engine, che lo rilascia fuori dal thread audio.
    Ended { play_id: u64, sound: SoundRef },
    /// Stream aperto. `mix_rate` è il mix format dell'endpoint (per `rate_mismatch`).
    Opened { mix_rate: u32 },
    /// Dispositivo invalidato o non apribile. Le voci restano in attesa di `Retry` o `Abort`.
    Lost { hresult: i32 },
    /// `AUDCLNT_E_DEVICE_IN_USE`: un'altra app tiene l'endpoint in exclusive mode.
    Busy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum BusState {
    #[default]
    Closed = 0,
    Running = 1,
    WarmIdle = 2,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct BusStats {
    pub state: BusState,
    pub voices: u32,
    pub underruns: u64,
    pub errors: u64,
    pub blocks: u64,
    pub mix_rate: u32,
    pub buffer_frames: u32,
}

/// Contatori atomici scritti dal thread audio, letti dall'engine solo su richiesta (SPEC §6.5.5).
#[derive(Default)]
pub struct BusShared {
    pub gain: Arc<AtomicU32>,
    pub state: AtomicU8,
    pub voices: AtomicU32,
    pub underruns: AtomicU64,
    pub errors: AtomicU64,
    pub blocks: AtomicU64,
    pub mix_rate: AtomicU32,
    pub buffer_frames: AtomicU32,
}

impl BusShared {
    pub fn new(gain: f32) -> Arc<Self> {
        let s = Self::default();
        s.gain.store(gain.to_bits(), Ordering::Relaxed);
        Arc::new(s)
    }

    pub fn stats(&self) -> BusStats {
        BusStats {
            state: match self.state.load(Ordering::Relaxed) {
                1 => BusState::Running,
                2 => BusState::WarmIdle,
                _ => BusState::Closed,
            },
            voices: self.voices.load(Ordering::Relaxed),
            underruns: self.underruns.load(Ordering::Relaxed),
            errors: self.errors.load(Ordering::Relaxed),
            blocks: self.blocks.load(Ordering::Relaxed),
            mix_rate: self.mix_rate.load(Ordering::Relaxed),
            buffer_frames: self.buffer_frames.load(Ordering::Relaxed),
        }
    }
}

/// Un bus d'uscita visto dall'engine.
pub trait OutputBus: Send {
    /// Accoda un comando e sveglia il thread del bus. `Err` = ring pieno: il comando torna indietro
    /// intero, così i suoi `Arc` non vengono mai rilasciati per sbaglio.
    #[allow(clippy::result_large_err)]
    fn send(&mut self, cmd: BusCmd) -> Result<(), BusCmd>;
    /// Consuma gli eventi pubblicati dal bus.
    fn drain(&mut self, f: &mut dyn FnMut(BusEvent));
    /// Volume lineare del bus, applicato con rampa al blocco successivo.
    fn set_gain(&self, lin: f32);
    fn stats(&self) -> BusStats;
    /// Ferma il thread e rilascia le voci rimaste sul thread chiamante.
    fn shutdown(self: Box<Self>);
}

/// Bus senza dispositivo: renderizza su richiesta. Usato nei test dell'engine e del mixer.
#[derive(Clone)]
pub struct OfflineBus {
    inner: Arc<Mutex<OfflineInner>>,
}

struct OfflineInner {
    mixer: Mixer,
    shared: Arc<BusShared>,
    events: Vec<BusEvent>,
    device: DeviceId,
    captured: Vec<f32>,
    capture: bool,
}

impl OfflineBus {
    pub fn new(gain: f32) -> Self {
        let shared = BusShared::new(gain);
        let mixer = Mixer::new(shared.gain.clone(), -1.0);
        let inner = OfflineInner {
            mixer,
            shared,
            events: Vec::new(),
            device: DeviceId::DEFAULT,
            captured: Vec::new(),
            capture: false,
        };
        Self { inner: Arc::new(Mutex::new(inner)) }
    }

    /// Registra l'uscita renderizzata (per confronti nei test).
    pub fn set_capture(&self, on: bool) {
        self.inner.lock().unwrap().capture = on;
    }

    pub fn take_captured(&self) -> Vec<f32> {
        std::mem::take(&mut self.inner.lock().unwrap().captured)
    }

    /// Avanza il tempo di `frames` campioni.
    pub fn advance(&self, frames: usize) {
        let mut i = self.inner.lock().unwrap();
        let mut out = vec![0.0f32; frames * 2];
        let OfflineInner { mixer, events, .. } = &mut *i;
        mixer.render(&mut out, &mut |play_id, sound| {
            events.push(BusEvent::Ended { play_id, sound });
            Ok(())
        });
        let voices = i.mixer.active_voices();
        i.shared.voices.store(voices, Ordering::Relaxed);
        i.shared.blocks.fetch_add(1, Ordering::Relaxed);
        if i.capture {
            i.captured.extend_from_slice(&out);
        }
    }

    pub fn device(&self) -> DeviceId {
        self.inner.lock().unwrap().device
    }

    pub fn active_voices(&self) -> u32 {
        self.inner.lock().unwrap().mixer.active_voices()
    }

    /// Simula un dispositivo perso.
    pub fn inject(&self, ev: BusEvent) {
        self.inner.lock().unwrap().events.push(ev);
    }
}

impl OutputBus for OfflineBus {
    #[allow(clippy::result_large_err)]
    fn send(&mut self, cmd: BusCmd) -> Result<(), BusCmd> {
        let mut i = self.inner.lock().unwrap();
        let OfflineInner { mixer, events, device, .. } = &mut *i;
        let mut ended = |play_id, sound| {
            events.push(BusEvent::Ended { play_id, sound });
            Ok(())
        };
        match cmd {
            BusCmd::Mixer(c) => mixer.apply(c, &mut ended),
            BusCmd::SetDevice(d) => *device = d,
            BusCmd::Retry => {}
            BusCmd::Abort => mixer.kill_all(&mut ended),
        }
        Ok(())
    }

    fn drain(&mut self, f: &mut dyn FnMut(BusEvent)) {
        let events = std::mem::take(&mut self.inner.lock().unwrap().events);
        for e in events {
            f(e);
        }
    }

    fn set_gain(&self, lin: f32) {
        self.inner.lock().unwrap().shared.gain.store(lin.to_bits(), Ordering::Relaxed);
    }

    fn stats(&self) -> BusStats {
        self.inner.lock().unwrap().shared.stats()
    }

    fn shutdown(self: Box<Self>) {
        let mut i = self.inner.lock().unwrap();
        let OfflineInner { mixer, events, .. } = &mut *i;
        mixer.kill_all(&mut |play_id, sound| {
            events.push(BusEvent::Ended { play_id, sound });
            Ok(())
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_id_round_trip() {
        let id = DeviceId::new("{0.0.0.00000000}.{abc}").unwrap();
        assert!(!id.is_default());
        assert_eq!(*id.as_wide_nul().last().unwrap(), 0);
        assert!(DeviceId::DEFAULT.is_default());
        assert!(DeviceId::new(&"x".repeat(96)).is_none());
        assert_eq!(DeviceId::from_option(None), DeviceId::DEFAULT);
    }
}
