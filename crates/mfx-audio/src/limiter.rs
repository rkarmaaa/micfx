//! Limiter di picco sul bus, senza lookahead (SPEC F09, §6.4).
//! Attack 1 ms, release 100 ms, ceiling −1 dBFS, più clip di sicurezza.

use crate::SAMPLE_RATE;

pub struct Limiter {
    gain: f32,
    attack: f32,
    release: f32,
    ceiling: f32,
}

impl Limiter {
    pub fn new(ceiling_db: f32) -> Self {
        let coeff = |ms: f32| 1.0 - (-1.0 / (ms * 0.001 * SAMPLE_RATE as f32)).exp();
        Self { gain: 1.0, attack: coeff(1.0), release: coeff(100.0), ceiling: crate::db_to_lin(ceiling_db) }
    }

    pub fn reset(&mut self) {
        self.gain = 1.0;
    }

    /// Elabora un buffer stereo interleaved in place.
    #[inline]
    pub fn process(&mut self, buf: &mut [f32]) {
        let c = self.ceiling;
        let mut g = self.gain;
        for frame in buf.as_chunks_mut::<2>().0 {
            let peak = frame[0].abs().max(frame[1].abs());
            let target = if peak * g > c { c / peak } else { 1.0 };
            let coeff = if target < g { self.attack } else { self.release };
            g += (target - g) * coeff;
            frame[0] = (frame[0] * g).clamp(-c, c);
            frame[1] = (frame[1] * g).clamp(-c, c);
        }
        self.gain = g;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_exceeds_ceiling() {
        let mut l = Limiter::new(-1.0);
        let mut buf: Vec<f32> = (0..9600).map(|i| if i % 2 == 0 { 3.0 } else { -2.5 }).collect();
        l.process(&mut buf);
        let c = crate::db_to_lin(-1.0);
        assert!(buf.iter().all(|s| s.abs() <= c + 1e-6));
    }

    #[test]
    fn transparent_below_ceiling() {
        let mut l = Limiter::new(-1.0);
        let mut buf = vec![0.5f32; 960];
        l.process(&mut buf);
        assert!(buf.iter().all(|&s| (s - 0.5).abs() < 1e-6));
    }

    #[test]
    fn releases_after_peak() {
        let mut l = Limiter::new(-1.0);
        let mut loud = vec![2.0f32; 960];
        l.process(&mut loud);
        assert!(l.gain < 0.5);
        let mut quiet = vec![0.1f32; 2 * 48_000];
        l.process(&mut quiet);
        assert!(l.gain > 0.99, "gain {}", l.gain);
    }
}
