//! Bridge del microfono su macOS (SPEC §6.6 "Bridge"): la voce va dal microfono al microfono virtuale.
//!
//! macOS non ha "Ascolta questo dispositivo": senza bridge Discord, che ascolta il cavo virtuale,
//! sentirebbe solo i suoni. Due AUHAL indipendenti:
//!
//! - cattura sul microfono, alla sua frequenza; la callback converte in mono 48 kHz (interpolazione
//!   lineare, sufficiente per la voce) e scrive in un ring `rtrb`;
//! - riproduzione sul cavo; la callback legge dal ring ([`VoiceOut`]). Il HAL somma questa uscita a
//!   quella del bus dei suoni, che resta indipendente (pausa calda compresa).
//!
//! Con la riduzione del rumore la rete neurale gira in un thread suo tra i due ring, non nelle
//! callback del HAL: un frame costa circa 1 ms e non deve far saltare l'audio del microfono.
//! Nelle callback: niente allocazioni, lock, log (SPEC §6.5).

use std::ffi::c_void;
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;

use rtrb::{Consumer, Producer};

use crate::SAMPLE_RATE;
use crate::coreaudio::sys::*;
use crate::coreaudio::{self, dispose_unit, float_format, new_hal_unit, set_unit_prop};
use crate::event::{Event, Wait};
use crate::voice::{self, Denoise, VoiceOut};

/// Frame massimi per callback di cattura (buffer preallocato).
const CAPTURE_MAX_FRAMES: usize = 8192;

pub struct MicBridge {
    input: AudioUnit,
    output: AudioUnit,
    capture: *mut CaptureState,
    play: *mut VoiceOut,
    denoise: Option<DenoiseThread>,
    pub mic_name: String,
}

// SAFETY: le unità e gli stati sono usati solo dalle callback di CoreAudio e da Drop.
unsafe impl Send for MicBridge {}

struct CaptureState {
    unit: AudioUnit,
    buf: Box<[f32]>,
    channels: u32,
    rs: Resampler,
    tx: Producer<f32>,
    /// Sveglia il thread della riduzione del rumore, se c'è.
    wake: Option<Arc<Event>>,
}

struct DenoiseThread {
    quit: Arc<AtomicBool>,
    wake: Arc<Event>,
    thread: Option<JoinHandle<()>>,
}

/// Interpolazione lineare a passo fisso: `step` = frequenza d'ingresso / 48000.
struct Resampler {
    step: f64,
    t: f64,
    prev: f32,
}

impl Resampler {
    fn new(in_rate: f64) -> Self {
        Self { step: in_rate / SAMPLE_RATE as f64, t: 0.0, prev: 0.0 }
    }

    #[inline]
    fn feed(&mut self, cur: f32, out: &mut impl FnMut(f32)) {
        while self.t < 1.0 {
            out(self.prev + (cur - self.prev) * self.t as f32);
            self.t += self.step;
        }
        self.t -= 1.0;
        self.prev = cur;
    }
}

impl MicBridge {
    /// Avvia il bridge. `mic_uid = None` = microfono predefinito di macOS. Con `muted` a `true` la
    /// voce non passa (rampa di 10 ms); con `denoise` passa dalla riduzione del rumore.
    pub fn start(
        mic_uid: Option<&str>,
        cable_uid: &str,
        denoise: bool,
        muted: Arc<AtomicBool>,
    ) -> Result<Self, String> {
        let cable = coreaudio::device_by_uid(cable_uid).ok_or("cavo virtuale non trovato")?;
        let mic = match mic_uid {
            Some(uid) => coreaudio::device_by_uid(uid),
            None => coreaudio::default_device(false),
        }
        .ok_or("nessun microfono")?;
        if mic == cable {
            return Err("il microfono scelto è il cavo virtuale".into());
        }
        let in_ch = coreaudio::channels(mic, false).clamp(1, 2);
        let in_rate = coreaudio::nominal_rate(mic).filter(|r| *r > 0.0).ok_or("frequenza del microfono sconosciuta")?;

        let (voice_tx, voice_rx) = voice::ring();
        // Con la riduzione del rumore la cattura scrive in un ring intermedio letto dal thread del filtro.
        let (tx, denoise) = if denoise {
            let (raw_tx, raw_rx) = voice::ring();
            (raw_tx, Some(DenoiseThread::spawn(raw_rx, voice_tx)?))
        } else {
            (voice_tx, None)
        };
        let capture = Box::into_raw(Box::new(CaptureState {
            unit: ptr::null_mut(),
            buf: vec![0.0f32; CAPTURE_MAX_FRAMES * in_ch as usize].into_boxed_slice(),
            channels: in_ch,
            rs: Resampler::new(in_rate),
            tx,
            wake: denoise.as_ref().map(|d| d.wake.clone()),
        }));
        let play = Box::into_raw(Box::new(VoiceOut::new(voice_rx, muted)));
        let mut b = Self {
            input: ptr::null_mut(),
            output: ptr::null_mut(),
            capture,
            play,
            denoise,
            mic_name: coreaudio::device_name(mic).unwrap_or_default(),
        };
        b.output = open_output(cable, play).map_err(|e| format!("uscita sul cavo: {e}"))?;
        b.input = open_input(mic, in_ch, in_rate, capture).map_err(|e| format!("cattura del microfono: {e}"))?;
        // SAFETY: unità inizializzate; prima l'uscita, così la prima voce trova già chi la legge.
        unsafe {
            let r = AudioOutputUnitStart(b.output);
            if r != 0 {
                return Err(format!("avvio uscita: {r}"));
            }
            let r = AudioOutputUnitStart(b.input);
            if r != 0 {
                return Err(format!("avvio cattura: {r}"));
            }
        }
        Ok(b)
    }
}

impl Drop for MicBridge {
    fn drop(&mut self) {
        // SAFETY: unità create in start(); dopo la distruzione nessuna callback usa più gli stati.
        unsafe {
            if !self.input.is_null() {
                dispose_unit(self.input);
            }
            if !self.output.is_null() {
                dispose_unit(self.output);
            }
        }
        // Il filtro si ferma dopo le callback: nessuno lo sveglia più.
        self.denoise = None;
        // SAFETY: stati creati con Box::into_raw in start() e non più usati.
        unsafe {
            drop(Box::from_raw(self.capture));
            drop(Box::from_raw(self.play));
        }
    }
}

impl DenoiseThread {
    fn spawn(mut rx: Consumer<f32>, mut tx: Producer<f32>) -> Result<Self, String> {
        let wake = Arc::new(Event::new().map_err(|e| e.to_string())?);
        let quit = Arc::new(AtomicBool::new(false));
        // La rete si costruisce qui (alloca), fuori da ogni thread audio.
        let mut dn = Denoise::new();
        let (w, q) = (wake.clone(), quit.clone());
        let thread = std::thread::Builder::new()
            .name("mfx-voice-denoise".into())
            .spawn(move || {
                // SAFETY: cambia solo la classe QoS del thread corrente.
                unsafe { pthread_set_qos_class_self_np(QOS_CLASS_USER_INTERACTIVE, 0) };
                crate::enable_ftz_daz();
                let mut emit = |f: &[f32]| voice::push(&mut tx, f);
                // Sveglia a ogni callback di cattura; il timeout serve solo a non restare appesi.
                while !q.load(Ordering::Acquire) {
                    if let Wait::Failed = w.wait(Some(100)) {
                        break;
                    }
                    if let Ok(c) = rx.read_chunk(rx.slots()) {
                        let (a, b) = c.as_slices();
                        dn.process(a, &mut emit);
                        dn.process(b, &mut emit);
                        c.commit_all();
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self { quit, wake, thread: Some(thread) })
    }
}

impl Drop for DenoiseThread {
    fn drop(&mut self) {
        self.quit.store(true, Ordering::Release);
        self.wake.set();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn open_output(dev: AudioObjectID, play: *mut VoiceOut) -> Result<AudioUnit, OSStatus> {
    let unit = new_hal_unit()?;
    let setup = || -> Result<(), OSStatus> {
        set_unit_prop(unit, AU_PROP_CURRENT_DEVICE, AU_SCOPE_GLOBAL, 0, &dev)?;
        let fmt = float_format(SAMPLE_RATE as f64, crate::CHANNELS as u32);
        set_unit_prop(unit, AU_PROP_STREAM_FORMAT, AU_SCOPE_INPUT, 0, &fmt)?;
        let cb = AURenderCallbackStruct { input_proc: play_cb, input_proc_ref_con: play.cast() };
        set_unit_prop(unit, AU_PROP_SET_RENDER_CALLBACK, AU_SCOPE_INPUT, 0, &cb)?;
        // SAFETY: unità configurata.
        match unsafe { AudioUnitInitialize(unit) } {
            0 => Ok(()),
            r => Err(r),
        }
    };
    setup().map(|()| unit).inspect_err(|_| {
        // SAFETY: unità creata qui e non registrata altrove.
        unsafe { dispose_unit(unit) }
    })
}

fn open_input(dev: AudioObjectID, ch: u32, rate: f64, capture: *mut CaptureState) -> Result<AudioUnit, OSStatus> {
    let unit = new_hal_unit()?;
    // SAFETY: lo stato è vivo e non ancora usato da nessuna callback.
    unsafe { (*capture).unit = unit };
    let setup = || -> Result<(), OSStatus> {
        // AUHAL in sola cattura: ingresso (elemento 1) acceso, uscita (elemento 0) spenta.
        set_unit_prop(unit, AU_PROP_ENABLE_IO, AU_SCOPE_INPUT, 1, &1u32)?;
        set_unit_prop(unit, AU_PROP_ENABLE_IO, AU_SCOPE_OUTPUT, 0, &0u32)?;
        set_unit_prop(unit, AU_PROP_CURRENT_DEVICE, AU_SCOPE_GLOBAL, 0, &dev)?;
        // In cattura l'AUHAL non cambia frequenza: si chiede quella del dispositivo e si converte qui.
        let fmt = float_format(rate, ch);
        set_unit_prop(unit, AU_PROP_STREAM_FORMAT, AU_SCOPE_OUTPUT, 1, &fmt)?;
        set_unit_prop(unit, AU_PROP_MAXIMUM_FRAMES_PER_SLICE, AU_SCOPE_GLOBAL, 0, &(CAPTURE_MAX_FRAMES as u32))?;
        let cb = AURenderCallbackStruct { input_proc: input_cb, input_proc_ref_con: capture.cast() };
        set_unit_prop(unit, AU_PROP_SET_INPUT_CALLBACK, AU_SCOPE_GLOBAL, 0, &cb)?;
        // SAFETY: unità configurata.
        match unsafe { AudioUnitInitialize(unit) } {
            0 => Ok(()),
            r => Err(r),
        }
    };
    setup().map(|()| unit).inspect_err(|_| {
        // SAFETY: unità creata qui e non registrata altrove.
        unsafe { dispose_unit(unit) }
    })
}

/// Callback di cattura: CoreAudio avvisa che ci sono `frames` campioni; li si legge con AudioUnitRender.
unsafe extern "C" fn input_cb(
    ref_con: *mut c_void,
    flags: *mut u32,
    ts: *const AudioTimeStamp,
    bus: u32,
    frames: u32,
    _data: *mut AudioBufferList,
) -> OSStatus {
    // SAFETY: `ref_con` è il CaptureState registrato in open_input, vivo finché l'unità esiste.
    let st = unsafe { &mut *(ref_con as *mut CaptureState) };
    let n = frames as usize;
    if n > CAPTURE_MAX_FRAMES {
        return 0;
    }
    let ch = st.channels as usize;
    let mut list = AudioBufferList {
        count: 1,
        buffers: [AudioBuffer {
            channels: st.channels,
            data_size: (n * ch * 4) as u32,
            data: st.buf.as_mut_ptr().cast(),
        }],
    };
    // SAFETY: buffer preallocato di almeno `n * ch` campioni.
    let r = unsafe { AudioUnitRender(st.unit, flags, ts, bus, frames, &mut list) };
    if r != 0 {
        return r;
    }
    crate::enable_ftz_daz();
    let CaptureState { buf, rs, tx, wake, .. } = st;
    for f in buf[..n * ch].chunks_exact(ch) {
        // La voce è mono: i microfoni stereo si sommano a metà.
        let m = if ch == 1 { f[0] } else { (f[0] + f[1]) * 0.5 };
        rs.feed(m, &mut |s| {
            let _ = tx.push(s);
        });
    }
    if let Some(w) = wake {
        w.set();
    }
    0
}

/// Callback di riproduzione sul cavo virtuale.
unsafe extern "C" fn play_cb(
    ref_con: *mut c_void,
    _flags: *mut u32,
    _ts: *const AudioTimeStamp,
    _bus: u32,
    frames: u32,
    data: *mut AudioBufferList,
) -> OSStatus {
    // SAFETY: `ref_con` è il VoiceOut registrato in open_output.
    let st = unsafe { &mut *(ref_con as *mut VoiceOut) };
    // SAFETY: AudioBufferList valido, un buffer interleaved (formato impostato).
    let buf = unsafe { &mut (*data).buffers[0] };
    let want = (frames as usize * crate::CHANNELS).min(buf.data_size as usize / 4);
    // SAFETY: buffer di CoreAudio scrivibile per `data_size` byte.
    let out = unsafe { std::slice::from_raw_parts_mut(buf.data as *mut f32, want) };
    st.render(out);
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resampler_counts() {
        for (rate, expect) in [(48_000.0, 48_000usize), (44_100.0, 48_000), (96_000.0, 48_000), (16_000.0, 48_000)] {
            let mut rs = Resampler::new(rate);
            let mut n = 0usize;
            for _ in 0..rate as usize {
                rs.feed(0.5, &mut |s| {
                    assert!((s - 0.5).abs() < 1e-6 || n < 4);
                    n += 1;
                });
            }
            assert!(n.abs_diff(expect) <= 2, "{rate}: {n}");
        }
    }
}
