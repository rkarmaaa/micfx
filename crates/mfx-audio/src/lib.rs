//! Motore audio di MicFX (docs/SPEC.md §6).
//!
//! - [`mixer`]: voci, gain, fade, furto di voci, limiter. Nessuna allocazione in `render`.
//! - [`bus`]: thread di render WASAPI shared/event-driven con ciclo di vita Closed → Running → WarmIdle.
//! - [`sink`]: trait [`OutputBus`] usato dall'engine, con un bus offline per i test.
//!
//! Regole real-time (SPEC §6.5): nel thread audio niente allocazioni, lock, log, I/O o drop che liberano
//! memoria. Comunicazione solo via `rtrb` e atomici.

pub mod mixer;
pub mod sink;

#[cfg(windows)]
pub mod bus;
#[cfg(windows)]
pub mod event;

mod limiter;

pub use mixer::{Mixer, MixerCmd, SampleSource, SoundRef};
pub use sink::{BusCmd, BusEvent, BusStats, DeviceId, OfflineBus, OutputBus, Wake};

/// Formato di tutti gli stream: float 32 bit, 48 kHz, stereo (SPEC §6.1).
pub const SAMPLE_RATE: u32 = 48_000;
pub const CHANNELS: usize = 2;
/// Voci per bus (SPEC §6.4). Oltre si ruba la più vecchia con fade.
pub const MAX_VOICES: usize = 32;
/// Slot extra riservati alle voci rubate mentre sfumano.
pub const STEAL_SLOTS: usize = 8;

pub const FADE_IN_FRAMES: u32 = SAMPLE_RATE * 2 / 1000; // 2 ms
pub const FADE_OUT_FRAMES: u32 = SAMPLE_RATE * 8 / 1000; // 8 ms

#[inline]
pub fn db_to_lin(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// Attiva flush-to-zero e denormals-are-zero sul thread corrente (SPEC §6.5.4).
pub fn enable_ftz_daz() {
    #[cfg(target_arch = "x86_64")]
    // SAFETY: legge e riscrive MXCSR del thread corrente impostando solo i bit FTZ (15) e DAZ (6).
    unsafe {
        let mut csr: u32 = 0;
        core::arch::asm!("stmxcsr [{}]", in(reg) &mut csr, options(nostack));
        csr |= (1 << 15) | (1 << 6);
        core::arch::asm!("ldmxcsr [{}]", in(reg) &csr, options(nostack, readonly));
    }
}

#[cfg(test)]
#[global_allocator]
static TEST_ALLOC: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;
