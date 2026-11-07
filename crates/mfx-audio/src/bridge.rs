//! Bridge del microfono (macOS, SPEC §6.6 "Bridge"): la voce va dal microfono al microfono virtuale.
//!
//! macOS non ha "Ascolta questo dispositivo": senza bridge Discord, che ascolta il cavo virtuale,
//! sentirebbe solo i suoni. Due AUHAL indipendenti:
//!
//! - cattura sul microfono, alla sua frequenza; la callback converte in stereo 48 kHz (interpolazione
//!   lineare, sufficiente per la voce) e scrive in un ring `rtrb`;
//! - riproduzione sul cavo; la callback legge dal ring. Il HAL somma questa uscita a quella del bus
//!   dei suoni, che resta indipendente (pausa calda compresa).
//!
//! I due dispositivi hanno clock diversi: la latenza nel ring si tiene intorno a [`TARGET_FRAMES`]
//! scartando l'eccesso, e dopo un buco si riparte solo a ring di nuovo pieno quanto basta.
//! Nelle callback: niente allocazioni, lock, log (SPEC §6.5).

use std::ffi::c_void;
use std::ptr;

use rtrb::{Consumer, Producer, RingBuffer};

use crate::coreaudio::sys::*;
use crate::coreaudio::{self, dispose_unit, float_format, new_hal_unit, set_unit_prop};
use crate::{CHANNELS, SAMPLE_RATE};

/// Latenza di lavoro nel ring: 20 ms.
const TARGET_FRAMES: usize = SAMPLE_RATE as usize / 50;
/// Oltre 4× la latenza di lavoro si scarta l'eccesso (deriva dei clock, callback in ritardo).
const MAX_FRAMES: usize = TARGET_FRAMES * 4;
/// Frame massimi per callback di cattura (buffer preallocato).
const CAPTURE_MAX_FRAMES: usize = 8192;

pub struct MicBridge {
    input: AudioUnit,
    output: AudioUnit,
    capture: *mut CaptureState,
    play: *mut PlayState,
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
}

struct PlayState {
    rx: Consumer<f32>,
    priming: bool,
}

/// Interpolazione lineare a passo fisso: `step` = frequenza d'ingresso / 48000.
struct Resampler {
    step: f64,
    t: f64,
    prev: [f32; 2],
}

impl Resampler {
    fn new(in_rate: f64) -> Self {
        Self { step: in_rate / SAMPLE_RATE as f64, t: 0.0, prev: [0.0; 2] }
    }

    #[inline]
    fn feed(&mut self, cur: [f32; 2], out: &mut impl FnMut([f32; 2])) {
        while self.t < 1.0 {
            let t = self.t as f32;
            out([self.prev[0] + (cur[0] - self.prev[0]) * t, self.prev[1] + (cur[1] - self.prev[1]) * t]);
            self.t += self.step;
        }
        self.t -= 1.0;
        self.prev = cur;
    }
}

impl MicBridge {
    /// Avvia il bridge. `mic_uid = None` = microfono predefinito di macOS.
    pub fn start(mic_uid: Option<&str>, cable_uid: &str) -> Result<Self, String> {
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
        let (tx, rx) = RingBuffer::<f32>::new(SAMPLE_RATE as usize * CHANNELS / 2);
        let capture = Box::into_raw(Box::new(CaptureState {
            unit: ptr::null_mut(),
            buf: vec![0.0f32; CAPTURE_MAX_FRAMES * in_ch as usize].into_boxed_slice(),
            channels: in_ch,
            rs: Resampler::new(in_rate),
            tx,
        }));
        let play = Box::into_raw(Box::new(PlayState { rx, priming: true }));
        let mut b = Self {
            input: ptr::null_mut(),
            output: ptr::null_mut(),
            capture,
            play,
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
            drop(Box::from_raw(self.capture));
            drop(Box::from_raw(self.play));
        }
    }
}

fn open_output(dev: AudioObjectID, play: *mut PlayState) -> Result<AudioUnit, OSStatus> {
    let unit = new_hal_unit()?;
    let setup = || -> Result<(), OSStatus> {
        set_unit_prop(unit, AU_PROP_CURRENT_DEVICE, AU_SCOPE_GLOBAL, 0, &dev)?;
        let fmt = float_format(SAMPLE_RATE as f64, CHANNELS as u32);
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
    let CaptureState { buf, rs, tx, .. } = st;
    for f in buf[..n * ch].chunks_exact(ch) {
        let cur = if ch == 1 { [f[0], f[0]] } else { [f[0], f[1]] };
        rs.feed(cur, &mut |s| {
            if tx.slots() >= 2 {
                let _ = tx.push(s[0]);
                let _ = tx.push(s[1]);
            }
        });
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
    // SAFETY: `ref_con` è il PlayState registrato in open_output.
    let st = unsafe { &mut *(ref_con as *mut PlayState) };
    // SAFETY: AudioBufferList valido, un buffer interleaved (formato impostato).
    let buf = unsafe { &mut (*data).buffers[0] };
    let want = (frames as usize * CHANNELS).min(buf.data_size as usize / 4);
    // SAFETY: buffer di CoreAudio scrivibile per `data_size` byte.
    let out = unsafe { std::slice::from_raw_parts_mut(buf.data as *mut f32, want) };

    let mut avail = st.rx.slots();
    if st.priming {
        if avail < TARGET_FRAMES * CHANNELS {
            out.fill(0.0);
            return 0;
        }
        st.priming = false;
    }
    if avail > MAX_FRAMES * CHANNELS {
        // Troppa latenza accumulata: si torna a quella di lavoro (a coppie, per non scambiare i canali).
        let drop_n = (avail - TARGET_FRAMES * CHANNELS) & !1;
        if let Ok(c) = st.rx.read_chunk(drop_n) {
            c.commit_all();
        }
        avail -= drop_n;
    }
    let take = want.min(avail) & !1;
    if let Ok(c) = st.rx.read_chunk(take) {
        let (a, b) = c.as_slices();
        out[..a.len()].copy_from_slice(a);
        out[a.len()..a.len() + b.len()].copy_from_slice(b);
        c.commit_all();
    }
    if take < want {
        // Buco: silenzio e si riparte con il ring di nuovo pieno quanto basta.
        out[take..].fill(0.0);
        st.priming = true;
    }
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
                rs.feed([0.5, -0.5], &mut |s| {
                    assert!((s[0] - 0.5).abs() < 1e-6 || n < 4);
                    n += 1;
                });
            }
            assert!(n.abs_diff(expect) <= 2, "{rate}: {n}");
        }
    }
}
