//! Astrazione delle risorse di sistema usate dall'engine: dispositivi, hotkey, tasti simulati, bus audio.
//! In produzione [`WindowsPlatform`] o [`MacPlatform`], nei test una piattaforma finta con bus offline.

use std::sync::Arc;

use mfx_audio::{DeviceId, OutputBus, Wake};
use mfx_platform::Accelerator;
use mfx_platform::discord::DiscordVoice;

use crate::api::{BusKind, Device};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(any(windows, target_os = "macos"))]
mod voices;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "macos")]
pub use macos::MacPlatform;
#[cfg(windows)]
pub use windows::WindowsPlatform;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MicEndpoint {
    pub id: String,
    pub name: String,
    pub is_default: bool,
    pub is_cable: bool,
    pub listen_enabled: bool,
    pub listen_target: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegisterError {
    Conflict,
    Failed(String),
}

/// Lettura delle impostazioni di Discord: gira su un thread a parte (I/O su disco).
pub type DiscordReader = Arc<dyn Fn() -> Option<DiscordVoice> + Send + Sync>;

#[derive(Debug, Clone, Copy)]
pub struct BusParams {
    pub gain: f32,
    pub ceiling_db: f32,
    pub idle_stop_ms: u32,
    pub warm_hold_ms: u32,
}

/// Bridge del microfono: quale microfono mandare in quale uscita, e se filtrarne il rumore.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MicRoute {
    /// `None` = microfono predefinito del sistema.
    pub mic_id: Option<String>,
    /// Il cavo, o le cuffie per la prova del microfono; `None` = uscita predefinita.
    pub output_id: Option<String>,
    pub denoise: bool,
}

pub trait Platform {
    fn render_endpoints(&mut self) -> Vec<Device>;
    fn capture_endpoints(&mut self) -> Vec<MicEndpoint>;
    fn register_hotkey(&mut self, id: i32, accel: Accelerator) -> Result<(), RegisterError>;
    fn unregister_hotkey(&mut self, id: i32);
    /// Tasto simulato per il relay PTT. `false` se il sistema lo rifiuta.
    fn send_key(&mut self, scancode: u16, down: bool) -> bool;
    fn create_bus(
        &mut self,
        kind: BusKind,
        device: DeviceId,
        params: BusParams,
        waker: Arc<dyn Wake>,
    ) -> Result<Box<dyn OutputBus>, String>;
    fn discord_reader(&self) -> DiscordReader;
    /// Accende, sposta o spegne (`None`) il bridge del microfono. Le piattaforme senza bridge ignorano.
    fn set_mic_bridge(&mut self, _route: Option<MicRoute>) {}
    /// Azzera (con una rampa breve) o ridà la voce nel bridge. Le piattaforme senza bridge ignorano.
    fn set_bridge_muted(&mut self, _muted: bool) {}
    /// Prova del microfono: la voce, con la stessa catena del bridge, nelle cuffie. `None` la spegne.
    fn set_mic_test(&mut self, _route: Option<MicRoute>) {}
    /// Intensità della riduzione del rumore (limite in dB): vale subito, senza riaprire i bridge.
    fn set_denoise_limit(&mut self, _db: f32) {}
    /// Silenzia o riattiva un microfono di sistema. Restituisce lo stato di prima, `None` se non riesce.
    fn set_mic_mute(&mut self, _id: &str, _mute: bool) -> Option<bool> {
        None
    }
}
