//! Integrazione con Windows (SPEC §8, §9): hotkey globali, relay PTT, dispositivi audio.
//!
//! Tutto è a eventi: `RegisterHotKey` + `GetMessageW`, `IMMNotificationClient`. Nessun polling.

pub mod accel;
pub mod discord;

#[cfg(windows)]
pub mod com;
#[cfg(windows)]
pub mod devices;
#[cfg(windows)]
pub mod hotkeys;
#[cfg(windows)]
pub mod input;

pub use accel::{AccelError, Accelerator, Key, Mods};

/// Apre il pannello audio classico di Windows (`mmsys.cpl`) sulla scheda indicata.
/// 0 = Riproduzione, 1 = Registrazione, 3 = Comunicazioni.
pub fn open_sound_control_panel(tab: u8) -> std::io::Result<()> {
    std::process::Command::new("control.exe").arg(format!("mmsys.cpl,,{tab}")).spawn().map(|_| ())
}
