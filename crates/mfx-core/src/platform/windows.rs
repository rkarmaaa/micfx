//! Piattaforma Windows: WASAPI, `IMMNotificationClient`, `RegisterHotKey`, `SendInput`.

use std::sync::Arc;

use crossbeam_channel::Sender;
use mfx_audio::bus::{BusTiming, WasapiBus};
use mfx_audio::event::Event;
use mfx_audio::{DeviceId, OutputBus, Wake};
use mfx_platform::Accelerator;
use mfx_platform::devices::{DeviceEvent, Devices, Watcher};
use mfx_platform::hotkeys::{HotkeyError, HotkeyService};

use super::{BusParams, DiscordReader, MicEndpoint, Platform, RegisterError};
use crate::api::{BusKind, Device};
use crate::engine::Command;

pub struct WindowsPlatform {
    devices: Option<Devices>,
    _watcher: Option<Watcher>,
    hotkeys: Option<HotkeyService>,
}

impl WindowsPlatform {
    /// Va creata sul thread engine, dopo l'inizializzazione COM MTA.
    pub fn new(tx: Sender<Command>, waker: Arc<Event>) -> Self {
        let devices = match Devices::new() {
            Ok(d) => Some(d),
            Err(e) => {
                tracing::error!("enumeratore dispositivi non disponibile: {e}");
                None
            }
        };
        let watcher = devices.as_ref().and_then(|d| {
            let (tx, waker) = (tx.clone(), waker.clone());
            d.watch(move |ev: DeviceEvent| {
                let _ = tx.send(Command::Device(ev));
                waker.set();
            })
            .map_err(|e| tracing::error!("notifiche dispositivi non disponibili: {e}"))
            .ok()
        });
        let hotkeys = {
            let (tx, waker) = (tx.clone(), waker.clone());
            HotkeyService::spawn(move |id| {
                let _ = tx.send(Command::Hotkey(id));
                waker.set();
            })
            .map_err(|e| tracing::error!("thread hotkey non avviato: {e}"))
            .ok()
        };
        Self { devices, _watcher: watcher, hotkeys }
    }
}

impl Platform for WindowsPlatform {
    fn render_endpoints(&mut self) -> Vec<Device> {
        let Some(d) = &self.devices else { return Vec::new() };
        match d.render_endpoints() {
            Ok(list) => list
                .into_iter()
                .map(|e| Device {
                    id: e.id,
                    name: e.name,
                    is_default: e.is_default,
                    is_cable: e.is_cable,
                    mix_rate: e.mix_rate,
                })
                .collect(),
            Err(e) => {
                tracing::warn!("enumerazione riproduzione fallita: {e}");
                Vec::new()
            }
        }
    }

    fn capture_endpoints(&mut self) -> Vec<MicEndpoint> {
        let Some(d) = &self.devices else { return Vec::new() };
        match d.capture_endpoints() {
            Ok(list) => list
                .into_iter()
                .map(|c| MicEndpoint {
                    id: c.id,
                    name: c.name,
                    is_default: c.is_default,
                    is_cable: c.is_cable,
                    listen_enabled: c.listen_enabled,
                    listen_target: c.listen_target,
                })
                .collect(),
            Err(e) => {
                tracing::warn!("enumerazione cattura fallita: {e}");
                Vec::new()
            }
        }
    }

    fn register_hotkey(&mut self, id: i32, accel: Accelerator) -> Result<(), RegisterError> {
        let Some(h) = &self.hotkeys else { return Err(RegisterError::Failed("servizio hotkey non attivo".into())) };
        h.register(id, accel).map_err(|e| match e {
            HotkeyError::Conflict => RegisterError::Conflict,
            other => RegisterError::Failed(other.to_string()),
        })
    }

    fn unregister_hotkey(&mut self, id: i32) {
        if let Some(h) = &self.hotkeys {
            h.unregister(id);
        }
    }

    fn send_key(&mut self, scancode: u16, down: bool) -> bool {
        mfx_platform::input::send_scancode(scancode, down)
    }

    fn create_bus(
        &mut self,
        kind: BusKind,
        device: DeviceId,
        params: BusParams,
        waker: Arc<dyn Wake>,
    ) -> Result<Box<dyn OutputBus>, String> {
        let name = match kind {
            BusKind::Cable => "cable",
            BusKind::Monitor => "monitor",
        };
        let timing = BusTiming { idle_stop_ms: params.idle_stop_ms, warm_hold_ms: params.warm_hold_ms };
        WasapiBus::spawn(name, device, params.gain, params.ceiling_db, timing, waker)
            .map(|b| Box::new(b) as Box<dyn OutputBus>)
            .map_err(|e| e.to_string())
    }

    fn discord_reader(&self) -> DiscordReader {
        Arc::new(mfx_platform::discord::read_voice_settings)
    }

    fn set_mic_mute(&mut self, id: &str, mute: bool) -> Option<bool> {
        let d = self.devices.as_ref()?;
        d.set_capture_mute(id, mute).map_err(|e| tracing::warn!("mute del microfono non riuscito: {e}")).ok()
    }
}
