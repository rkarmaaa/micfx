//! Integrazione con il sistema (SPEC §8, §9): hotkey globali, relay PTT, dispositivi audio.
//!
//! Tutto è a eventi, nessun polling. Windows: `RegisterHotKey` + `GetMessageW`, `IMMNotificationClient`,
//! `SendInput`. macOS: hotkey Carbon (`RegisterEventHotKey`), listener del HAL di CoreAudio, `CGEventPost`.

pub mod accel;
pub mod discord;

#[cfg(windows)]
pub mod com;
#[cfg(windows)]
#[path = "devices_win.rs"]
pub mod devices;
#[cfg(windows)]
#[path = "hotkeys_win.rs"]
pub mod hotkeys;
#[cfg(windows)]
#[path = "input_win.rs"]
pub mod input;

#[cfg(target_os = "macos")]
#[path = "devices_mac.rs"]
pub mod devices;
#[cfg(target_os = "macos")]
#[path = "hotkeys_mac.rs"]
pub mod hotkeys;
#[cfg(target_os = "macos")]
#[path = "input_mac.rs"]
pub mod input;
#[cfg(target_os = "macos")]
mod main_queue;

pub use accel::{AccelError, Accelerator, Key, Mods};

/// Apre le impostazioni audio del sistema.
/// Windows: pannello classico (`mmsys.cpl`) sulla scheda indicata (0 = Riproduzione, 1 = Registrazione,
/// 3 = Comunicazioni). macOS: Impostazioni di Sistema → Suono (1 = scheda Ingresso), 2 = Configurazione MIDI Audio.
pub fn open_sound_control_panel(tab: u8) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        std::process::Command::new("control.exe").arg(format!("mmsys.cpl,,{tab}")).spawn().map(|_| ())
    }
    #[cfg(target_os = "macos")]
    {
        let mut cmd = std::process::Command::new("open");
        match tab {
            2 => cmd.args(["-b", "com.apple.audio.AudioMIDISetup"]),
            1 => cmd.arg("x-apple.systempreferences:com.apple.Sound-Settings.extension?input"),
            _ => cmd.arg("x-apple.systempreferences:com.apple.Sound-Settings.extension?output"),
        };
        cmd.spawn().map(|_| ())
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = tab;
        Err(std::io::Error::other("non supportato"))
    }
}
