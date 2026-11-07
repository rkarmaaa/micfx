//! Piattaforma macOS: CoreAudio (AUHAL e listener del HAL), hotkey Carbon, `CGEventPost`, bridge
//! del microfono.

use std::sync::Arc;

use crossbeam_channel::Sender;
use mfx_audio::bridge::MicBridge;
use mfx_audio::bus::{BusTiming, CoreAudioBus};
use mfx_audio::event::Event;
use mfx_audio::{DeviceId, OutputBus, Wake};
use mfx_platform::Accelerator;
use mfx_platform::devices::{DeviceEvent, Devices, Watcher};
use mfx_platform::hotkeys::{HotkeyError, HotkeyService};

use super::{BusParams, DiscordReader, MicEndpoint, MicRoute, Platform, RegisterError};
use crate::api::{BusKind, Device};
use crate::engine::{Command, Engine};

pub struct MacPlatform {
    devices: Devices,
    _watcher: Option<Watcher>,
    hotkeys: Option<HotkeyService>,
    bridge: Option<(MicRoute, MicBridge)>,
}

impl MacPlatform {
    pub fn new(tx: Sender<Command>, waker: Arc<Event>) -> Self {
        let devices = Devices;
        let watcher = {
            let (tx, waker) = (tx.clone(), waker.clone());
            devices
                .watch(move |ev: DeviceEvent| {
                    let _ = tx.send(Command::Device(ev));
                    waker.set();
                })
                .map_err(|e| tracing::error!("notifiche dispositivi non disponibili: {e}"))
                .ok()
        };
        let hotkeys = {
            let (tx, waker) = (tx.clone(), waker.clone());
            let (ftx, fwaker) = (tx.clone(), waker.clone());
            HotkeyService::spawn(
                move |id| {
                    let _ = tx.send(Command::Hotkey(id));
                    waker.set();
                },
                move |id, conflict| {
                    let _ = ftx.send(Command::Call(Box::new(move |e: &mut Engine| e.hotkey_rejected(id, conflict))));
                    fwaker.set();
                },
            )
            .map_err(|e| tracing::error!("servizio hotkey non avviato: {e}"))
            .ok()
        };
        Self { devices, _watcher: watcher, hotkeys, bridge: None }
    }
}

impl Platform for MacPlatform {
    fn render_endpoints(&mut self) -> Vec<Device> {
        match self.devices.render_endpoints() {
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
        match self.devices.capture_endpoints() {
            Ok(list) => list
                .into_iter()
                .map(|c| MicEndpoint {
                    id: c.id,
                    name: c.name,
                    is_default: c.is_default,
                    is_cable: c.is_cable,
                    listen_enabled: false,
                    listen_target: None,
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
        CoreAudioBus::spawn(name, device, params.gain, params.ceiling_db, timing, waker)
            .map(|b| Box::new(b) as Box<dyn OutputBus>)
            .map_err(|e| e.to_string())
    }

    fn discord_reader(&self) -> DiscordReader {
        Arc::new(mfx_platform::discord::read_voice_settings)
    }

    fn set_mic_bridge(&mut self, route: Option<MicRoute>) {
        if self.bridge.as_ref().map(|(r, _)| r) == route.as_ref() {
            return;
        }
        // Prima si chiude il vecchio: un solo flusso di voce nel cavo.
        self.bridge = None;
        let Some(route) = route else {
            tracing::info!("bridge microfono spento");
            return;
        };
        match MicBridge::start(route.mic_id.as_deref(), &route.cable_id) {
            Ok(b) => {
                tracing::info!("bridge microfono: «{}» → cavo", b.mic_name);
                self.bridge = Some((route, b));
            }
            Err(e) => tracing::warn!("bridge microfono non avviato: {e}"),
        }
    }
}
