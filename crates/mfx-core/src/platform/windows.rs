//! Piattaforma Windows: WASAPI, `IMMNotificationClient`, `RegisterHotKey`, `SendInput`, bridge del
//! microfono (solo con la riduzione del rumore).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crossbeam_channel::Sender;
use mfx_audio::bridge::MicBridge;
use mfx_audio::bus::{BusTiming, WasapiBus};
use mfx_audio::event::Event;
use mfx_audio::{DeviceId, OutputBus, Wake};
use mfx_platform::Accelerator;
use mfx_platform::devices::{DeviceEvent, Devices, Watcher};
use mfx_platform::hotkeys::{HotkeyError, HotkeyService};

use super::{BusParams, DiscordReader, MicEndpoint, MicRoute, Platform, RegisterError};
use crate::api::{BusKind, Device};
use crate::engine::Command;

pub struct WindowsPlatform {
    devices: Option<Devices>,
    _watcher: Option<Watcher>,
    hotkeys: Option<HotkeyService>,
    bridge: Option<(MicRoute, MicBridge)>,
    /// Condiviso con ogni bridge: un bridge riaperto durante un suono resta muto.
    voice_muted: Arc<AtomicBool>,
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
        Self { devices, _watcher: watcher, hotkeys, bridge: None, voice_muted: Arc::new(AtomicBool::new(false)) }
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

    fn set_mic_bridge(&mut self, route: Option<MicRoute>) {
        match &self.bridge {
            Some((r, b)) if Some(r) == route.as_ref() => {
                if b.is_alive() {
                    return;
                }
                tracing::warn!("bridge microfono interrotto (dispositivo cambiato o scollegato): lo riapro");
            }
            _ => {}
        }
        // Prima si chiude il vecchio: un solo flusso di voce nel cavo.
        self.bridge = None;
        let Some(route) = route else {
            tracing::info!("bridge microfono spento");
            return;
        };
        match MicBridge::start(route.mic_id.as_deref(), &route.cable_id, route.denoise, self.voice_muted.clone()) {
            Ok(b) => {
                tracing::info!("bridge microfono: {:?} → cavo (riduzione del rumore: {})", route.mic_id, route.denoise);
                self.bridge = Some((route, b));
            }
            Err(e) => tracing::warn!("bridge microfono non avviato: {e}"),
        }
    }

    fn set_bridge_muted(&mut self, muted: bool) {
        self.voice_muted.store(muted, Ordering::Relaxed);
    }

    fn set_mic_mute(&mut self, id: &str, mute: bool) -> Option<bool> {
        let d = self.devices.as_ref()?;
        d.set_capture_mute(id, mute).map_err(|e| tracing::warn!("mute del microfono non riuscito: {e}")).ok()
    }
}
