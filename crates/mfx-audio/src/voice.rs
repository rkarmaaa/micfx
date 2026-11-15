//! Voce del microfono verso il cavo (bridge, SPEC §6.6): la parte comune a Windows e macOS.
//!
//! Il percorso è sempre lo stesso: cattura → mono 48 kHz → (riduzione del rumore) → ring `rtrb` →
//! [`VoiceOut`], che la scrive stereo sul cavo virtuale. Il sistema somma questa uscita a quella del
//! bus dei suoni: i suoni non passano mai dal filtro, che li toglierebbe come fa Krisp su Discord.
//!
//! La riduzione del rumore è DeepFilterNet3 (`dfn3_rs`): rete neurale per la voce a 48 kHz, frame di
//! 10 ms, 30 ms di ritardo, nessuna allocazione dopo la costruzione.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use dfn3_rs::{Denoiser, FRAME_SIZE};
use rtrb::{Consumer, Producer, RingBuffer};

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

/// Riduzione del rumore a frame di 10 ms: accumula i campioni e restituisce la voce pulita a frame interi.
pub struct Denoise {
    net: Box<Denoiser>,
    frame: [f32; FRAME_SIZE],
    out: [f32; FRAME_SIZE],
    fill: usize,
}

impl Denoise {
    /// Alloca e scalda la rete: va chiamata fuori dal thread audio.
    pub fn new() -> Self {
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
        Self { net, frame: [0.0; FRAME_SIZE], out: [0.0; FRAME_SIZE], fill: 0 }
    }

    /// Filtra `input` (mono 48 kHz); `emit` riceve la voce pulita, un frame alla volta. Nessuna allocazione.
    #[inline]
    pub fn process(&mut self, mut input: &[f32], emit: &mut impl FnMut(&[f32])) {
        while !input.is_empty() {
            let n = (FRAME_SIZE - self.fill).min(input.len());
            self.frame[self.fill..self.fill + n].copy_from_slice(&input[..n]);
            self.fill += n;
            input = &input[n..];
            if self.fill == FRAME_SIZE {
                self.fill = 0;
                self.net.process(&self.frame, &mut self.out);
                emit(&self.out);
            }
        }
    }
}

impl Default for Denoise {
    fn default() -> Self {
        Self::new()
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

    #[test]
    fn denoise_removes_noise_without_allocating() {
        let mut d = Denoise::new();
        let mut seed = 0x1234_5678u32;
        let mut noise = || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            ((seed >> 9) as f32 / (1u32 << 23) as f32 - 0.5) * 0.2
        };
        // 2 s di rumore bianco a pezzi di dimensione qualsiasi (come arrivano dai driver).
        let input: Vec<f32> = (0..SAMPLE_RATE as usize * 2).map(|_| noise()).collect();
        let mut out = Vec::with_capacity(input.len());
        for chunk in input.chunks(441) {
            assert_no_alloc::assert_no_alloc(|| {
                d.process(chunk, &mut |f| out.extend_from_slice(f));
            });
        }
        assert_eq!(out.len(), input.len() / FRAME_SIZE * FRAME_SIZE);
        let energy = |s: &[f32]| s.iter().map(|x| x * x).sum::<f32>() / s.len() as f32;
        let tail = out.len() / 2;
        let ratio = energy(&out[tail..]) / energy(&input[tail..]);
        assert!(ratio < 0.01, "rumore attenuato di almeno 20 dB: {ratio}");
    }
}
