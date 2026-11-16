//! Voce del microfono verso il cavo (bridge, SPEC §6.6): la parte comune a Windows e macOS.
//!
//! Il percorso è sempre lo stesso: cattura → mono 48 kHz → [`VoiceChain`] (riduzione del rumore
//! facoltativa, livellamento) → ring `rtrb` → [`VoiceOut`], che la scrive stereo sul cavo virtuale. Il sistema somma questa uscita a quella del
//! bus dei suoni: i suoni non passano mai dal filtro, che li toglierebbe come fa Krisp su Discord.
//!
//! La riduzione del rumore è DeepFilterNet3 (`dfn3_rs`): rete neurale per la voce a 48 kHz, frame di
//! 10 ms, 30 ms di ritardo, nessuna allocazione dopo la costruzione.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

use dfn3_rs::{Denoiser, FRAME_SIZE};
use rtrb::{Consumer, Producer, RingBuffer};

use crate::limiter::Limiter;
use crate::{CHANNELS, SAMPLE_RATE};

/// Latenza di lavoro nel ring: 20 ms.
pub const TARGET_FRAMES: usize = SAMPLE_RATE as usize / 50;
/// Oltre 4× la latenza di lavoro si scarta l'eccesso (deriva dei clock, callback in ritardo).
pub const MAX_FRAMES: usize = TARGET_FRAMES * 4;
/// Rampa del mute della voce: 10 ms, niente click.
const MUTE_STEP: f32 = 1.0 / (SAMPLE_RATE as f32 / 100.0);

/// Ring della voce (mono): mezzo secondo, molto più di [`MAX_FRAMES`].
pub fn ring() -> (Producer<f32>, Consumer<f32>) {
    RingBuffer::new(SAMPLE_RATE as usize / 2)
}

/// Spinge i campioni che entrano; se il ring è pieno (uscita ferma) il resto si perde.
#[inline]
pub fn push(tx: &mut Producer<f32>, samples: &[f32]) {
    let n = samples.len().min(tx.slots());
    if let Ok(mut c) = tx.write_chunk(n) {
        let (a, b) = c.as_mut_slices();
        a.copy_from_slice(&samples[..a.len()]);
        b.copy_from_slice(&samples[a.len()..n]);
        c.commit_all();
    }
}

/// Un `f32` condiviso senza lock (i bit in un `AtomicU32`): parametri regolabili mentre l'audio gira.
#[derive(Debug, Default)]
pub struct AtomicF32(AtomicU32);

impl AtomicF32 {
    pub fn new(v: f32) -> Self {
        Self(AtomicU32::new(v.to_bits()))
    }

    #[inline]
    pub fn load(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }

    #[inline]
    pub fn store(&self, v: f32) {
        self.0.store(v.to_bits(), Ordering::Relaxed);
    }
}

/// Riduzione del rumore della catena: la rete e quanto può togliere al massimo (dB, `atten-lim` di
/// DeepFilterNet), letto a ogni frame così lo slider dell'intensità agisce dal vivo.
struct Denoise {
    net: Box<Denoiser>,
    limit: Arc<AtomicF32>,
    current: f32,
}

/// Catena della voce a frame di 10 ms: riduzione del rumore (facoltativa) e livellamento ([`Agc`]).
/// Accumula i campioni e restituisce la voce pronta per il cavo a frame interi.
pub struct VoiceChain {
    denoise: Option<Denoise>,
    frame: [f32; FRAME_SIZE],
    out: [f32; FRAME_SIZE],
    fill: usize,
    agc: Agc,
}

impl VoiceChain {
    /// Con la riduzione del rumore (`denoise` = limite in dB, condiviso) alloca e scalda la rete: va
    /// chiamata fuori dal thread audio.
    pub fn new(denoise: Option<Arc<AtomicF32>>) -> Self {
        let denoise = denoise.map(|limit| {
            let mut net = Box::new(Denoiser::new());
            // Riscaldamento consigliato da dfn3_rs: tutte le fasi della rete girano una volta (pesi in
            // memoria, niente picco sul primo frame vero), poi si riparte puliti.
            let mut buf = [0.0f32; FRAME_SIZE];
            for (i, s) in buf.iter_mut().enumerate() {
                *s = if i % 2 == 0 { 1e-3 } else { -1e-3 };
            }
            net.set_thresholds(-100.0, 100.0, 100.0);
            for _ in 0..4 {
                net.process_frame(&mut buf);
            }
            net.reset();
            net.set_thresholds(-10.0, 30.0, 20.0);
            let current = limit.load();
            net.set_attenuation_limit(current);
            Denoise { net, limit, current }
        });
        Self { denoise, frame: [0.0; FRAME_SIZE], out: [0.0; FRAME_SIZE], fill: 0, agc: Agc::new() }
    }

    /// Elabora `input` (mono 48 kHz); `emit` riceve la voce, un frame alla volta. Nessuna allocazione.
    #[inline]
    pub fn process(&mut self, mut input: &[f32], emit: &mut impl FnMut(&[f32])) {
        while !input.is_empty() {
            let n = (FRAME_SIZE - self.fill).min(input.len());
            self.frame[self.fill..self.fill + n].copy_from_slice(&input[..n]);
            self.fill += n;
            input = &input[n..];
            if self.fill == FRAME_SIZE {
                self.fill = 0;
                match &mut self.denoise {
                    Some(d) => {
                        let want = d.limit.load();
                        if want != d.current {
                            // Cambio d'intensità dal vivo: dissolvenza di 100 ms, nessun salto.
                            d.current = want;
                            d.net.set_attenuation_limit_smooth(want, Duration::from_millis(100));
                        }
                        d.net.process(&self.frame, &mut self.out);
                    }
                    None => self.out = self.frame,
                }
                self.agc.process(&mut self.out);
                emit(&self.out);
            }
        }
    }
}

/// Livello del parlato a cui l'AGC porta la voce (RMS medio dei frame di voce): qualche dB sotto i
/// suoni, che l'import livella a −18 LUFS, con margine per i picchi. Senza, un microfono poco
/// sensibile arriva 20–30 dB sotto i suoni.
const AGC_TARGET_DB: f32 = -23.0;
/// Guadagno massimo e minimo: oltre +30 dB si amplificherebbe solo rumore.
const AGC_MAX_DB: f32 = 30.0;
const AGC_MIN_DB: f32 = -12.0;
/// Frame di voce (10 ms) del primo adattamento, veloce: il primo secondo in cui parli.
const AGC_LEARN_FRAMES: u32 = 100;
/// Media del livello per frame di voce, in potenza (come la loudness: contano le parti forti del
/// parlato, non le code): 0,1 mentre impara, poi 0,005 (~2 s). Segue la voce, non le sillabe: è un
/// fader che si sposta piano, non un compressore.
const AGC_LEARN_ALPHA: f32 = 0.1;
const AGC_ALPHA: f32 = 0.005;
/// Velocità del guadagno per frame: mentre impara 50 dB/s, poi sale di 6 dB/s e scende di 20 dB/s.
const AGC_LEARN_STEP_DB: f32 = 0.5;
const AGC_UP_DB: f32 = 0.06;
const AGC_DOWN_DB: f32 = 0.2;
/// Un frame è voce se supera di tanto il rumore di fondo e una soglia assoluta, e se non è molto sotto
/// il livello della voce (soglia relativa, come il gate di EBU R128): il residuo tra le parole non
/// trascina giù la stima.
const AGC_SPEECH_OVER_FLOOR_DB: f32 = 10.0;
const AGC_SPEECH_MIN_DB: f32 = -62.0;
const AGC_SPEECH_BELOW_LEVEL_DB: f32 = 20.0;
/// Tetto dei picchi dopo il guadagno (limitatore a campione, come quello dei suoni).
const AGC_CEILING_DB: f32 = -1.0;

/// Livellamento automatico della voce, come il "controllo automatico del guadagno" di Discord (che la
/// guida fa spegnere perché abbassa i suoni). Il guadagno si muove piano e solo mentre parli: nelle
/// pause resta fermo e non alza il rumore. I picchi li prende il limitatore.
struct Agc {
    /// Rumore di fondo stimato (minimo che risale piano), dB.
    floor_db: f32,
    /// Potenza media del parlato; `None` finché non si è sentita voce.
    level: Option<f32>,
    speech_frames: u32,
    gain_db: f32,
    /// Guadagno lineare applicato alla fine del frame precedente (la rampa parte da lì).
    applied: f32,
    limiter: Limiter,
}

impl Agc {
    fn new() -> Self {
        Self {
            floor_db: -90.0,
            level: None,
            speech_frames: 0,
            gain_db: 0.0,
            applied: 1.0,
            limiter: Limiter::new(AGC_CEILING_DB),
        }
    }

    fn process(&mut self, f: &mut [f32; FRAME_SIZE]) {
        let pow = f.iter().map(|s| s * s).sum::<f32>() / FRAME_SIZE as f32 + 1e-12;
        let db = 10.0 * pow.log10();
        let level_db = self.level.map(|l| 10.0 * l.log10());
        // Fondo: segue subito i cali, risale di 3 dB/s (una voce lunga non diventa "fondo").
        self.floor_db = if db < self.floor_db { db } else { self.floor_db + 0.03 };
        let speech = db > AGC_SPEECH_MIN_DB
            && db > self.floor_db + AGC_SPEECH_OVER_FLOOR_DB
            && level_db.is_none_or(|l| db > l - AGC_SPEECH_BELOW_LEVEL_DB);
        if speech {
            let learning = self.speech_frames < AGC_LEARN_FRAMES;
            self.speech_frames += 1;
            let level = match self.level {
                Some(l) => l + (pow - l) * if learning { AGC_LEARN_ALPHA } else { AGC_ALPHA },
                None => pow,
            };
            self.level = Some(level);
            let want = (AGC_TARGET_DB - 10.0 * level.log10()).clamp(AGC_MIN_DB, AGC_MAX_DB);
            let (down, up) = if learning { (AGC_LEARN_STEP_DB, AGC_LEARN_STEP_DB) } else { (AGC_DOWN_DB, AGC_UP_DB) };
            self.gain_db += (want - self.gain_db).clamp(-down, up);
        }
        // Rampa dal guadagno del frame precedente: nessun gradino udibile.
        let g = 10f32.powf(self.gain_db / 20.0);
        let step = (g - self.applied) / FRAME_SIZE as f32;
        let mut cur = self.applied;
        for s in f.iter_mut() {
            cur += step;
            *s *= cur;
        }
        self.applied = g;
        self.limiter.process_mono(f);
    }
}

/// Lato uscita: legge la voce dal ring e la scrive stereo interleaved, tenendo la latenza intorno a
/// [`TARGET_FRAMES`]. I due dispositivi hanno clock diversi: l'eccesso si scarta, e dopo un buco si
/// riparte solo a ring di nuovo pieno quanto basta. Nessuna allocazione, lock o log.
pub struct VoiceOut {
    rx: Consumer<f32>,
    priming: bool,
    /// "Disattiva la mia voce": la voce scende a zero mentre suona l'effetto.
    muted: Arc<AtomicBool>,
    gain: f32,
}

impl VoiceOut {
    pub fn new(rx: Consumer<f32>, muted: Arc<AtomicBool>) -> Self {
        let gain = if muted.load(Ordering::Relaxed) { 0.0 } else { 1.0 };
        Self { rx, priming: true, muted, gain }
    }

    /// Riempie tutto `out` (stereo interleaved): voce dove c'è, silenzio altrove.
    pub fn render(&mut self, out: &mut [f32]) {
        let frames = out.len() / CHANNELS;
        let mut avail = self.rx.slots();
        if self.priming {
            if avail < TARGET_FRAMES {
                out.fill(0.0);
                return;
            }
            self.priming = false;
        }
        if avail > MAX_FRAMES {
            // Troppa latenza accumulata: si torna a quella di lavoro.
            let drop_n = avail - TARGET_FRAMES;
            if let Ok(c) = self.rx.read_chunk(drop_n) {
                c.commit_all();
            }
            avail -= drop_n;
        }
        let take = frames.min(avail);
        if let Ok(c) = self.rx.read_chunk(take) {
            let (a, b) = c.as_slices();
            let target = if self.muted.load(Ordering::Relaxed) { 0.0 } else { 1.0 };
            let mut gain = self.gain;
            for (o, &s) in out.as_chunks_mut::<CHANNELS>().0.iter_mut().zip(a.iter().chain(b)) {
                if gain != target {
                    gain = if target == 0.0 { (gain - MUTE_STEP).max(0.0) } else { (gain + MUTE_STEP).min(1.0) };
                }
                o.fill(s * gain);
            }
            self.gain = gain;
            c.commit_all();
        }
        if take < frames {
            // Buco: silenzio e si riparte con il ring di nuovo pieno quanto basta.
            out[take * CHANNELS..].fill(0.0);
            self.priming = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn out_frames(n: usize) -> Vec<f32> {
        vec![9.0; n * CHANNELS]
    }

    #[test]
    fn primes_then_plays_and_trims_latency() {
        let (mut tx, rx) = ring();
        let mut v = VoiceOut::new(rx, Arc::new(AtomicBool::new(false)));
        let mut out = out_frames(480);
        push(&mut tx, &[0.5; TARGET_FRAMES - 1]);
        v.render(&mut out);
        assert!(out.iter().all(|s| *s == 0.0), "priming: silenzio");
        push(&mut tx, &[0.5; 1 + MAX_FRAMES * 2]);
        assert_no_alloc::assert_no_alloc(|| v.render(&mut out));
        assert!(out.iter().all(|s| *s == 0.5));
        assert_eq!(tx.buffer().capacity() - tx.slots(), TARGET_FRAMES - 480, "eccesso scartato");
    }

    #[test]
    fn hole_restarts_priming() {
        let (mut tx, rx) = ring();
        let mut v = VoiceOut::new(rx, Arc::new(AtomicBool::new(false)));
        push(&mut tx, &[0.25; TARGET_FRAMES + 100]);
        let mut out = out_frames(TARGET_FRAMES);
        v.render(&mut out);
        let mut out = out_frames(480);
        v.render(&mut out);
        assert!(out[..200].iter().all(|s| *s == 0.25) && out[200..].iter().all(|s| *s == 0.0));
        push(&mut tx, &[0.25; 10]);
        v.render(&mut out);
        assert!(out.iter().all(|s| *s == 0.0), "dopo un buco aspetta il ring pieno");
    }

    #[test]
    fn mute_ramp() {
        let (mut tx, rx) = ring();
        let muted = Arc::new(AtomicBool::new(false));
        let mut v = VoiceOut::new(rx, muted.clone());
        let mut out = out_frames(TARGET_FRAMES);
        let mut render = |v: &mut VoiceOut, out: &mut [f32]| {
            push(&mut tx, &[1.0; TARGET_FRAMES]);
            v.render(out);
        };
        muted.store(true, Ordering::Relaxed);
        render(&mut v, &mut out);
        assert!(out[0] > 0.9 && *out.last().unwrap() == 0.0, "rampa di 10 ms");
        render(&mut v, &mut out);
        assert!(out.iter().all(|s| *s == 0.0));
        muted.store(false, Ordering::Relaxed);
        render(&mut v, &mut out);
        assert!(out[0] < 0.1 && *out.last().unwrap() == 1.0);
    }

    /// Esegue la catena a pezzi di dimensione qualsiasi (come arrivano dai driver), senza allocare.
    fn run(chain: &mut VoiceChain, input: &[f32]) -> Vec<f32> {
        let mut out = Vec::with_capacity(input.len());
        for chunk in input.chunks(441) {
            assert_no_alloc::assert_no_alloc(|| chain.process(chunk, &mut |f| out.extend_from_slice(f)));
        }
        assert_eq!(out.len(), input.len() / FRAME_SIZE * FRAME_SIZE);
        out
    }

    fn rms_db(s: &[f32]) -> f32 {
        10.0 * (s.iter().map(|x| x * x).sum::<f32>() / s.len() as f32 + 1e-12).log10()
    }

    /// "Parlato" sintetico: vocale con armoniche, 300 ms di voce e 200 ms di pausa (silenzio).
    fn talk(secs: f32, rms_db_target: f32) -> Vec<f32> {
        let n = (SAMPLE_RATE as f32 * secs) as usize;
        let mut v: Vec<f32> = (0..n)
            .map(|i| {
                let t = i as f32 / SAMPLE_RATE as f32;
                if (t * 2.0).fract() >= 0.6 {
                    return 0.0;
                }
                (1..6).map(|h| (std::f32::consts::TAU * 180.0 * h as f32 * t).sin() / h as f32).sum::<f32>()
            })
            .collect();
        let on: Vec<f32> = v.iter().copied().filter(|x| *x != 0.0).collect();
        let g = 10f32.powf((rms_db_target - rms_db(&on)) / 20.0);
        v.iter_mut().for_each(|x| *x *= g);
        v
    }

    /// Livello dei soli tratti di voce dell'ultimo secondo.
    fn voiced_db(out: &[f32], input: &[f32]) -> f32 {
        let from = out.len() - SAMPLE_RATE as usize;
        let on: Vec<f32> =
            out[from..].iter().zip(&input[from..]).filter(|(_, i)| **i != 0.0).map(|(o, _)| *o).collect();
        rms_db(&on)
    }

    #[test]
    fn denoise_removes_noise_without_allocating() {
        let mut chain = VoiceChain::new(Some(Arc::new(AtomicF32::new(100.0))));
        let mut seed = 0x1234_5678u32;
        // 2 s di rumore bianco a −45 dBFS circa: nessuna voce, il livellamento non deve alzarlo.
        let input: Vec<f32> = (0..SAMPLE_RATE as usize * 2)
            .map(|_| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                ((seed >> 9) as f32 / (1u32 << 23) as f32 - 0.5) * 0.02
            })
            .collect();
        let out = run(&mut chain, &input);
        let tail = out.len() / 2;
        let drop = rms_db(&out[tail..]) - rms_db(&input[tail..]);
        assert!(drop < -20.0, "rumore attenuato di almeno 20 dB: {drop}");
    }

    #[test]
    fn agc_raises_a_quiet_voice() {
        let mut chain = VoiceChain::new(None);
        let input = talk(3.0, -45.0);
        let out = run(&mut chain, &input);
        let level = voiced_db(&out, &input);
        assert!((level - AGC_TARGET_DB).abs() < 3.0, "voce a −45 dBFS portata a {level:.1} dBFS");
        assert!(out.iter().all(|s| s.abs() <= 1.0));
        // Nelle pause il silenzio resta silenzio.
        assert!(out.iter().zip(&input).filter(|(_, i)| **i == 0.0).all(|(o, _)| *o == 0.0));
    }

    #[test]
    fn agc_tames_a_loud_voice_without_clipping() {
        let mut chain = VoiceChain::new(None);
        let input = talk(3.0, -4.0);
        let out = run(&mut chain, &input);
        let level = voiced_db(&out, &input);
        assert!(level < -12.0, "voce forte abbassata: {level:.1} dBFS");
        let peak = out[SAMPLE_RATE as usize..].iter().fold(0f32, |m, s| m.max(s.abs()));
        let ceiling = crate::db_to_lin(AGC_CEILING_DB);
        assert!(out.iter().all(|s| s.abs() <= ceiling + 1e-6), "picco {peak} oltre −1 dBFS");
    }

    #[test]
    fn agc_follows_the_voice_not_the_syllables() {
        // Sillabe alternate forti e deboli (±6 dB): dopo il primo secondo il guadagno resta quasi fermo.
        let mut chain = VoiceChain::new(None);
        let mut input = talk(6.0, -40.0);
        for (i, s) in input.iter_mut().enumerate() {
            if (i / (SAMPLE_RATE as usize / 2)) % 2 == 1 {
                *s *= 4.0;
            }
        }
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for (k, chunk) in input.chunks(FRAME_SIZE).enumerate() {
            chain.process(chunk, &mut |_| {});
            if k >= 200 {
                lo = lo.min(chain.agc.gain_db);
                hi = hi.max(chain.agc.gain_db);
            }
        }
        assert!(hi - lo < 2.0, "il guadagno oscilla di {:.1} dB", hi - lo);
    }

    #[test]
    fn agc_holds_gain_on_background_noise() {
        // Rumore costante e debole dopo la voce: il fondo lo riconosce, il guadagno non sale ancora.
        let mut chain = VoiceChain::new(None);
        let mut input = talk(2.0, -30.0);
        let gain_after_voice = {
            run(&mut chain, &input);
            chain.agc.gain_db
        };
        input = (0..SAMPLE_RATE as usize * 2).map(|i| if i % 2 == 0 { 3e-4 } else { -3e-4 }).collect();
        run(&mut chain, &input);
        assert!((chain.agc.gain_db - gain_after_voice).abs() < 1.0, "{} → {}", gain_after_voice, chain.agc.gain_db);
    }
}
