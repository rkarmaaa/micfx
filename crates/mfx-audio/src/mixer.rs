//! Mixer di un bus (SPEC §6.4): pool fisso di voci, gain con rampa, fade, furto di voci, limiter.
//!
//! `render` non alloca, non blocca e non libera memoria: gli `Arc` delle voci finite tornano
//! all'engine attraverso la callback `ended`, che li spinge su un ring `rtrb`.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::limiter::Limiter;
use crate::{FADE_IN_FRAMES, FADE_OUT_FRAMES, MAX_VOICES, STEAL_SLOTS};

/// Sorgente di campioni i16 interleaved a 48 kHz (es. file `.sbpcm` mappato).
pub trait SampleSource: Send + Sync + 'static {
    fn samples(&self) -> &[i16];
    fn channels(&self) -> u16;
}

impl SampleSource for (Vec<i16>, u16) {
    fn samples(&self) -> &[i16] {
        &self.0
    }
    fn channels(&self) -> u16 {
        self.1
    }
}

/// Riferimento a un suono pronto per il mixer: puntatore e lunghezza più l'`Arc` che tiene viva la memoria.
pub struct SoundRef {
    src: Arc<dyn SampleSource>,
    ptr: *const i16,
    len: usize,
    frames: usize,
    channels: u16,
}

// SAFETY: `ptr` punta nella memoria posseduta da `src` (immutabile, Send + Sync) e vive quanto l'Arc.
unsafe impl Send for SoundRef {}

impl SoundRef {
    pub fn new(src: Arc<dyn SampleSource>) -> Self {
        let s = src.samples();
        let channels = src.channels().clamp(1, 2);
        let (ptr, len) = (s.as_ptr(), s.len());
        Self { src, ptr, len, frames: len / channels as usize, channels }
    }

    #[inline]
    fn samples(&self) -> &[i16] {
        // SAFETY: vedi `unsafe impl Send`; `len` viene dalla slice originale.
        unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
    }

    pub fn frames(&self) -> usize {
        self.frames
    }

    pub fn channels(&self) -> u16 {
        self.channels
    }

    pub fn source(&self) -> &Arc<dyn SampleSource> {
        &self.src
    }
}

impl Clone for SoundRef {
    fn clone(&self) -> Self {
        Self { src: self.src.clone(), ptr: self.ptr, len: self.len, frames: self.frames, channels: self.channels }
    }
}

/// Comandi verso il mixer, già risolti dall'engine (modalità, gain).
pub enum MixerCmd {
    /// `gain` lineare = loudness × gain utente del suono. Il volume del bus è a parte.
    Play { play_id: u64, sound: SoundRef, gain: f32 },
    /// Fade-out di 8 ms di tutte le voci con quel `play_id`.
    Stop { play_id: u64 },
    /// Fade-out di 8 ms di tutto.
    StopAll,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum VState {
    #[default]
    Free,
    FadeIn,
    Play,
    FadeOut,
    /// Finita, in attesa che il ring verso l'engine abbia posto per restituire l'Arc.
    Done,
}

#[derive(Default)]
struct Voice {
    sound: Option<SoundRef>,
    play_id: u64,
    pos: usize,
    gain: f32,
    env: f32,
    state: VState,
    seq: u64,
}

/// Callback di fine voce: restituisce l'Arc all'engine. `Err` = ring pieno, si riprova al blocco dopo.
pub type EndedFn<'a> = dyn FnMut(u64, SoundRef) -> Result<(), SoundRef> + 'a;

pub struct Mixer {
    voices: Box<[Voice]>,
    seq: u64,
    bus_gain: Arc<AtomicU32>,
    bus_gain_cur: f32,
    limiter: Limiter,
    idle_frames: u64,
    active: u32,
}

impl Mixer {
    /// `bus_gain`: volume lineare del bus (bit di un f32), scritto dall'engine, letto a ogni blocco.
    pub fn new(bus_gain: Arc<AtomicU32>, ceiling_db: f32) -> Self {
        let mut voices = Vec::with_capacity(MAX_VOICES + STEAL_SLOTS);
        voices.resize_with(MAX_VOICES + STEAL_SLOTS, Voice::default);
        let g = f32::from_bits(bus_gain.load(Ordering::Relaxed));
        Self {
            voices: voices.into_boxed_slice(),
            seq: 0,
            bus_gain,
            bus_gain_cur: g,
            limiter: Limiter::new(ceiling_db),
            idle_frames: 0,
            active: 0,
        }
    }

    pub fn has_voices(&self) -> bool {
        self.voices.iter().any(|v| v.state != VState::Free)
    }

    /// Voci che producono suono (escluse quelle già finite in attesa di restituzione).
    pub fn active_voices(&self) -> u32 {
        self.active
    }

    /// Frame consecutivi renderizzati senza alcuna voce.
    pub fn idle_frames(&self) -> u64 {
        self.idle_frames
    }

    pub fn apply(&mut self, cmd: MixerCmd, ended: &mut EndedFn<'_>) {
        match cmd {
            MixerCmd::Play { play_id, sound, gain } => self.play(play_id, sound, gain, ended),
            MixerCmd::Stop { play_id } => {
                for v in self.voices.iter_mut().filter(|v| v.play_id == play_id) {
                    if matches!(v.state, VState::FadeIn | VState::Play) {
                        v.state = VState::FadeOut;
                    }
                }
            }
            MixerCmd::StopAll => {
                for v in self.voices.iter_mut() {
                    if matches!(v.state, VState::FadeIn | VState::Play) {
                        v.state = VState::FadeOut;
                    }
                }
            }
        }
        self.recount();
    }

    /// Termina subito tutte le voci, senza fade (dispositivo perso e nessun recupero).
    pub fn kill_all(&mut self, ended: &mut EndedFn<'_>) {
        for v in self.voices.iter_mut() {
            if v.state != VState::Free {
                v.state = VState::Done;
            }
        }
        self.flush_done(ended);
        self.recount();
    }

    fn play(&mut self, play_id: u64, sound: SoundRef, gain: f32, ended: &mut EndedFn<'_>) {
        self.seq += 1;
        let sounding = self.voices.iter().filter(|v| matches!(v.state, VState::FadeIn | VState::Play)).count();
        if sounding >= MAX_VOICES {
            // Si ruba la voce più vecchia, con fade di 8 ms.
            if let Some(old) = self
                .voices
                .iter_mut()
                .filter(|v| matches!(v.state, VState::FadeIn | VState::Play))
                .min_by_key(|v| v.seq)
            {
                old.state = VState::FadeOut;
            }
        }
        let slot = match self.voices.iter().position(|v| v.state == VState::Free) {
            Some(i) => i,
            None => {
                // Caso limite: tutti gli slot occupati da voci in fade. Si chiude la più vecchia di colpo.
                let i = self
                    .voices
                    .iter()
                    .enumerate()
                    .filter(|(_, v)| v.state != VState::Done)
                    .min_by_key(|(_, v)| v.seq)
                    .map(|(i, _)| i);
                match i {
                    Some(i) => {
                        let v = &mut self.voices[i];
                        if let Some(s) = v.sound.take() {
                            match ended(v.play_id, s) {
                                Ok(()) => v.state = VState::Free,
                                Err(s) => {
                                    v.sound = Some(s);
                                    v.state = VState::Done;
                                    let _ = ended(play_id, sound);
                                    return;
                                }
                            }
                        }
                        i
                    }
                    None => {
                        // Nessuno slot liberabile: il play viene restituito subito come finito.
                        let _ = ended(play_id, sound);
                        return;
                    }
                }
            }
        };
        let v = &mut self.voices[slot];
        v.sound = Some(sound);
        v.play_id = play_id;
        v.pos = 0;
        v.gain = gain;
        v.env = 0.0;
        v.state = VState::FadeIn;
        v.seq = self.seq;
    }

    fn recount(&mut self) {
        self.active =
            self.voices.iter().filter(|v| matches!(v.state, VState::FadeIn | VState::Play | VState::FadeOut)).count()
                as u32;
    }

    fn flush_done(&mut self, ended: &mut EndedFn<'_>) {
        for v in self.voices.iter_mut().filter(|v| v.state == VState::Done) {
            if let Some(s) = v.sound.take() {
                match ended(v.play_id, s) {
                    Ok(()) => v.state = VState::Free,
                    Err(s) => {
                        v.sound = Some(s);
                        return; // ring pieno: si riprova al prossimo blocco
                    }
                }
            } else {
                v.state = VState::Free;
            }
        }
    }

    /// Riempie `out` (stereo interleaved, f32) con la somma delle voci, volume del bus e limiter.
    pub fn render(&mut self, out: &mut [f32], ended: &mut EndedFn<'_>) {
        out.fill(0.0);
        let frames = out.len() / 2;
        let mut any = false;
        for v in self.voices.iter_mut() {
            match v.state {
                VState::Free | VState::Done => continue,
                _ => {}
            }
            any = true;
            if render_voice(v, out, frames) {
                v.state = VState::Done;
            }
        }
        self.flush_done(ended);
        self.recount();

        let target = f32::from_bits(self.bus_gain.load(Ordering::Relaxed));
        if !any {
            self.bus_gain_cur = target;
            self.limiter.reset();
            self.idle_frames = self.idle_frames.saturating_add(frames as u64);
            return;
        }
        self.idle_frames = 0;

        // Volume del bus con rampa lineare sul blocco: nessun gradino udibile quando muovi lo slider.
        let start = self.bus_gain_cur;
        if (target - start).abs() > 1e-6 {
            let step = (target - start) / frames.max(1) as f32;
            let mut g = start;
            for f in out.as_chunks_mut::<2>().0 {
                g += step;
                f[0] *= g;
                f[1] *= g;
            }
        } else if (target - 1.0).abs() > 1e-6 {
            for s in out.iter_mut() {
                *s *= target;
            }
        }
        self.bus_gain_cur = target;
        self.limiter.process(out);
    }
}

/// Somma una voce in `out`. Ritorna `true` se la voce è finita.
#[inline]
fn render_voice(v: &mut Voice, out: &mut [f32], frames: usize) -> bool {
    let Some(sound) = v.sound.as_ref() else { return true };
    let src = sound.samples();
    let total = sound.frames;
    let stereo = sound.channels == 2;
    let k = v.gain * (1.0 / 32768.0);
    let mut i = 0usize;
    while i < frames {
        if v.pos >= total {
            return true;
        }
        let n = (frames - i).min(total - v.pos);
        match v.state {
            VState::Play => {
                let o = &mut out[i * 2..(i + n) * 2];
                if stereo {
                    let s = &src[v.pos * 2..(v.pos + n) * 2];
                    for (o, s) in o.iter_mut().zip(s) {
                        *o += *s as f32 * k;
                    }
                } else {
                    let s = &src[v.pos..v.pos + n];
                    for (o, s) in o.as_chunks_mut::<2>().0.iter_mut().zip(s) {
                        let x = *s as f32 * k;
                        o[0] += x;
                        o[1] += x;
                    }
                }
                v.pos += n;
                i += n;
            }
            VState::FadeIn | VState::FadeOut => {
                let fade_in = v.state == VState::FadeIn;
                let step = if fade_in { 1.0 / FADE_IN_FRAMES as f32 } else { -1.0 / FADE_OUT_FRAMES as f32 };
                let mut j = 0usize;
                while j < n {
                    v.env += step;
                    if fade_in && v.env >= 1.0 {
                        v.env = 1.0;
                        v.state = VState::Play;
                    } else if !fade_in && v.env <= 0.0 {
                        v.env = 0.0;
                        v.pos += j;
                        return true;
                    }
                    let g = k * v.env;
                    let (l, r) = if stereo {
                        let p = (v.pos + j) * 2;
                        (src[p] as f32 * g, src[p + 1] as f32 * g)
                    } else {
                        let x = src[v.pos + j] as f32 * g;
                        (x, x)
                    };
                    out[(i + j) * 2] += l;
                    out[(i + j) * 2 + 1] += r;
                    j += 1;
                    if v.state == VState::Play {
                        break;
                    }
                }
                v.pos += j;
                i += j;
            }
            VState::Free | VState::Done => return true,
        }
    }
    v.pos >= total
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db_to_lin;

    fn gain_atomic(g: f32) -> Arc<AtomicU32> {
        Arc::new(AtomicU32::new(g.to_bits()))
    }

    fn sound(samples: Vec<i16>, ch: u16) -> SoundRef {
        SoundRef::new(Arc::new((samples, ch)))
    }

    struct Ended(Vec<u64>);
    impl Ended {
        fn cb(&mut self) -> impl FnMut(u64, SoundRef) -> Result<(), SoundRef> + '_ {
            move |id, _s| {
                self.0.push(id);
                Ok(())
            }
        }
    }

    fn render(m: &mut Mixer, frames: usize, ended: &mut Ended) -> Vec<f32> {
        let mut out = vec![0.0; frames * 2];
        m.render(&mut out, &mut ended.cb());
        out
    }

    #[test]
    fn sums_and_fades_in() {
        let mut m = Mixer::new(gain_atomic(1.0), 0.0);
        let mut e = Ended(vec![]);
        m.apply(MixerCmd::Play { play_id: 1, sound: sound(vec![8192; 4800], 1), gain: 1.0 }, &mut e.cb());
        let out = render(&mut m, 480, &mut e);
        // primo campione quasi zero (fade-in), dopo 2 ms pieno: 8192/32768 = 0,25
        assert!(out[0] < 0.01);
        assert!((out[2 * 200] - 0.25).abs() < 1e-4);
        assert_eq!(out[2 * 200], out[2 * 200 + 1], "mono duplicato su L e R");
    }

    #[test]
    fn voice_ends_and_returns_arc() {
        let mut m = Mixer::new(gain_atomic(1.0), 0.0);
        let mut e = Ended(vec![]);
        let s = sound(vec![1000; 1000], 1);
        let weak = Arc::downgrade(s.source());
        m.apply(MixerCmd::Play { play_id: 7, sound: s, gain: 1.0 }, &mut e.cb());
        render(&mut m, 2000, &mut e);
        assert_eq!(e.0, vec![7]);
        assert!(!m.has_voices());
        assert!(weak.upgrade().is_none(), "l'Arc è stato restituito e rilasciato fuori dal mixer");
    }

    #[test]
    fn stop_fades_out_in_8ms() {
        let mut m = Mixer::new(gain_atomic(1.0), 0.0);
        let mut e = Ended(vec![]);
        m.apply(MixerCmd::Play { play_id: 1, sound: sound(vec![16000; 48000], 1), gain: 1.0 }, &mut e.cb());
        render(&mut m, 480, &mut e);
        m.apply(MixerCmd::Stop { play_id: 1 }, &mut e.cb());
        let out = render(&mut m, 480, &mut e);
        assert_eq!(e.0, vec![1]);
        // decrescente e a zero dopo 384 frame
        assert!(out[2 * 100] < out[0]);
        assert_eq!(out[2 * 400], 0.0);
    }

    #[test]
    fn steals_oldest_voice() {
        let mut m = Mixer::new(gain_atomic(1.0), 0.0);
        let mut e = Ended(vec![]);
        for id in 0..(MAX_VOICES as u64 + 1) {
            m.apply(MixerCmd::Play { play_id: id, sound: sound(vec![10; 48000], 1), gain: 1.0 }, &mut e.cb());
        }
        render(&mut m, 480, &mut e);
        assert_eq!(e.0, vec![0], "la voce più vecchia è stata rubata");
        assert_eq!(m.active_voices(), MAX_VOICES as u32);
    }

    #[test]
    fn many_plays_never_lose_arcs() {
        let mut m = Mixer::new(gain_atomic(1.0), 0.0);
        let mut e = Ended(vec![]);
        for id in 0..200u64 {
            m.apply(MixerCmd::Play { play_id: id, sound: sound(vec![10; 48000], 1), gain: 1.0 }, &mut e.cb());
        }
        m.apply(MixerCmd::StopAll, &mut e.cb());
        render(&mut m, 4800, &mut e);
        let mut got = e.0.clone();
        got.sort();
        got.dedup();
        assert_eq!(got.len(), 200);
    }

    #[test]
    fn limiter_keeps_sum_under_ceiling() {
        let mut m = Mixer::new(gain_atomic(1.0), -1.0);
        let mut e = Ended(vec![]);
        for id in 0..8 {
            m.apply(MixerCmd::Play { play_id: id, sound: sound(vec![30000; 48000], 1), gain: 1.0 }, &mut e.cb());
        }
        let out = render(&mut m, 4800, &mut e);
        let c = db_to_lin(-1.0);
        assert!(out.iter().all(|s| s.abs() <= c + 1e-6));
    }

    #[test]
    fn bus_gain_ramps_without_steps() {
        let g = gain_atomic(1.0);
        let mut m = Mixer::new(g.clone(), 0.0);
        let mut e = Ended(vec![]);
        m.apply(MixerCmd::Play { play_id: 1, sound: sound(vec![8000; 48000], 1), gain: 1.0 }, &mut e.cb());
        render(&mut m, 480, &mut e);
        g.store(0.0f32.to_bits(), Ordering::Relaxed);
        let out = render(&mut m, 480, &mut e);
        let max_jump = out
            .as_chunks::<2>()
            .0
            .iter()
            .map(|f| f[0])
            .collect::<Vec<_>>()
            .windows(2)
            .map(|w| (w[0] - w[1]).abs())
            .fold(0.0, f32::max);
        assert!(max_jump < 0.01, "salto {max_jump}");
        let out = render(&mut m, 480, &mut e);
        assert!(out.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn idle_counter_tracks_silence() {
        let mut m = Mixer::new(gain_atomic(1.0), 0.0);
        let mut e = Ended(vec![]);
        render(&mut m, 480, &mut e);
        render(&mut m, 480, &mut e);
        assert_eq!(m.idle_frames(), 960);
        m.apply(MixerCmd::Play { play_id: 1, sound: sound(vec![1; 100], 1), gain: 1.0 }, &mut e.cb());
        render(&mut m, 480, &mut e);
        assert_eq!(m.idle_frames(), 0);
    }

    #[test]
    fn full_ring_retries_next_block() {
        let mut m = Mixer::new(gain_atomic(1.0), 0.0);
        m.apply(MixerCmd::Play { play_id: 3, sound: sound(vec![1; 10], 1), gain: 1.0 }, &mut |_, s| Err(s));
        let mut out = vec![0.0; 960];
        m.render(&mut out, &mut |_, s| Err(s));
        assert!(m.has_voices(), "la voce resta finché il ring non ha posto");
        let mut got = vec![];
        m.render(&mut out, &mut |id, _| {
            got.push(id);
            Ok(())
        });
        assert_eq!(got, vec![3]);
        assert!(!m.has_voices());
    }

    #[test]
    fn render_does_not_allocate() {
        let mut m = Mixer::new(gain_atomic(0.5), -1.0);
        let (mut tx, mut rx) = rtrb::RingBuffer::<(u64, SoundRef)>::new(64);
        for id in 0..40u64 {
            m.apply(
                MixerCmd::Play { play_id: id, sound: sound(vec![20000; 9600], (id % 2 + 1) as u16), gain: 0.8 },
                &mut |id, s| tx.push((id, s)).map_err(|rtrb::PushError::Full((_, s))| s),
            );
        }
        let mut out = vec![0.0f32; 960];
        for _ in 0..20 {
            assert_no_alloc::assert_no_alloc(|| {
                m.render(&mut out, &mut |id, s| tx.push((id, s)).map_err(|rtrb::PushError::Full((_, s))| s));
            });
        }
        let mut n = 0;
        while rx.pop().is_ok() {
            n += 1;
        }
        assert_eq!(n, 40);
    }

    /// Golden: sequenza di comandi renderizzata offline, confronto bit a bit tramite hash FNV-1a.
    #[test]
    fn golden_sequence() {
        let render_seq = || {
            let mut m = Mixer::new(gain_atomic(db_to_lin(-3.0)), -1.0);
            let mut e = Ended(vec![]);
            let a: Vec<i16> = (0..24000).map(|i| ((i as f32 * 0.05).sin() * 20000.0) as i16).collect();
            let b: Vec<i16> = (0..9600).map(|i| (((i * 37) % 2000) as i16 - 1000) * 12).collect();
            let mut all = Vec::new();
            m.apply(MixerCmd::Play { play_id: 1, sound: sound(a.clone(), 1), gain: 1.0 }, &mut e.cb());
            all.extend(render(&mut m, 480, &mut e));
            m.apply(MixerCmd::Play { play_id: 2, sound: sound(b, 2), gain: 0.7 }, &mut e.cb());
            all.extend(render(&mut m, 480, &mut e));
            m.apply(MixerCmd::Play { play_id: 3, sound: sound(a, 1), gain: 1.5 }, &mut e.cb());
            all.extend(render(&mut m, 480, &mut e));
            m.apply(MixerCmd::Stop { play_id: 1 }, &mut e.cb());
            for _ in 0..60 {
                all.extend(render(&mut m, 480, &mut e));
            }
            (all, e.0)
        };
        let (a, ended_a) = render_seq();
        let (b, ended_b) = render_seq();
        assert_eq!(ended_a, ended_b);
        assert_eq!(ended_a, vec![1, 2, 3]);
        let hash = |v: &[f32]| {
            v.iter().fold(0xcbf29ce484222325u64, |h, s| (h ^ s.to_bits() as u64).wrapping_mul(0x100000001b3))
        };
        assert_eq!(hash(&a), hash(&b));
        let c = db_to_lin(-1.0);
        assert!(a.iter().all(|s| s.abs() <= c + 1e-6));
    }
}
