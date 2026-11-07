//! Dispositivi audio su macOS (SPEC §9): enumerazione dal HAL di CoreAudio, riconoscimento del cavo
//! virtuale (BlackHole o VB-Cable per Mac) e notifiche con i listener del HAL.
//!
//! Su macOS lo stesso dispositivo ha uscita e ingresso con un solo UID: il cavo compare sia tra le
//! uscite (dove MicFX suona) sia tra gli ingressi (quello che Discord ascolta).
//! Le callback del HAL arrivano su un suo thread: inoltrano e ritornano subito.

use std::ffi::c_void;

use mfx_audio::coreaudio::sys::*;
use mfx_audio::coreaudio::{channels, default_device, device_ids, device_name, device_uid, nominal_rate};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderEndpoint {
    pub id: String,
    pub name: String,
    pub is_default: bool,
    pub is_cable: bool,
    pub mix_rate: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureEndpoint {
    pub id: String,
    pub name: String,
    pub is_default: bool,
    /// È il lato d'ingresso del cavo virtuale (non è un microfono vero).
    pub is_cable: bool,
}

/// Cavo virtuale riconosciuto dal nome: BlackHole (gratuito, open source) o VB-Cable per Mac.
pub fn is_cable_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("blackhole") || n.contains("vb-cable") || n.contains("vb-audio")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceEvent {
    /// Aggiunto o rimosso: va rifatta l'enumerazione.
    Changed,
    /// Nuovo dispositivo predefinito (riproduzione o cattura).
    DefaultChanged { render: bool },
    /// Cambiata una proprietà rilevante.
    PropertyChanged,
}

struct Info {
    uid: String,
    name: String,
    out_ch: u32,
    in_ch: u32,
    rate: u32,
}

fn info(id: AudioObjectID) -> Option<Info> {
    let uid = device_uid(id)?;
    let name = device_name(id).unwrap_or_else(|| uid.clone());
    Some(Info {
        uid,
        name,
        out_ch: channels(id, true),
        in_ch: channels(id, false),
        rate: nominal_rate(id).unwrap_or(0.0).round() as u32,
    })
}

/// Accesso ai dispositivi. Nessuno stato: ogni chiamata rilegge il HAL.
pub struct Devices;

impl Devices {
    pub fn new() -> Result<Self, String> {
        Ok(Self)
    }

    pub fn render_endpoints(&self) -> Result<Vec<RenderEndpoint>, String> {
        let default = default_device(true);
        Ok(device_ids()
            .into_iter()
            .filter_map(|id| info(id).filter(|i| i.out_ch > 0).map(|i| (id, i)))
            .map(|(id, i)| RenderEndpoint {
                is_default: Some(id) == default,
                is_cable: is_cable_name(&i.name),
                id: i.uid,
                name: i.name,
                mix_rate: i.rate,
            })
            .collect())
    }

    pub fn capture_endpoints(&self) -> Result<Vec<CaptureEndpoint>, String> {
        let default = default_device(false);
        Ok(device_ids()
            .into_iter()
            .filter_map(|id| info(id).filter(|i| i.in_ch > 0).map(|i| (id, i)))
            .map(|(id, i)| CaptureEndpoint {
                is_default: Some(id) == default,
                is_cable: is_cable_name(&i.name),
                id: i.uid,
                name: i.name,
            })
            .collect())
    }

    /// Registra i listener del HAL. Restano attivi finché il `Watcher` vive.
    pub fn watch(&self, on_event: impl Fn(DeviceEvent) + Send + Sync + 'static) -> Result<Watcher, String> {
        // Lasciato vivo per tutta la durata del processo: il HAL potrebbe chiamare il listener anche
        // mentre viene rimosso, e il Watcher vive comunque quanto l'app.
        let f: &'static Notifier = Box::leak(Box::new(Notifier { f: Box::new(on_event) }));
        let client = f as *const Notifier as *mut c_void;
        for sel in WATCHED {
            let addr = AudioObjectPropertyAddress { selector: sel, scope: SCOPE_GLOBAL, element: ELEMENT_MAIN };
            // SAFETY: `client` resta valido per sempre (leak sopra).
            let r = unsafe { AudioObjectAddPropertyListener(SYSTEM_OBJECT, &addr, listener, client) };
            if r != 0 {
                return Err(format!("AudioObjectAddPropertyListener: {r}"));
            }
        }
        Ok(Watcher { client })
    }
}

const WATCHED: [u32; 3] = [HARDWARE_DEVICES, HARDWARE_DEFAULT_OUTPUT, HARDWARE_DEFAULT_INPUT];

struct Notifier {
    f: Box<dyn Fn(DeviceEvent) + Send + Sync>,
}

pub struct Watcher {
    client: *mut c_void,
}

// SAFETY: `client` è un puntatore a dati Send + Sync che non vengono mai liberati.
unsafe impl Send for Watcher {}

impl Drop for Watcher {
    fn drop(&mut self) {
        for sel in WATCHED {
            let addr = AudioObjectPropertyAddress { selector: sel, scope: SCOPE_GLOBAL, element: ELEMENT_MAIN };
            // SAFETY: stessi parametri della registrazione.
            unsafe { AudioObjectRemovePropertyListener(SYSTEM_OBJECT, &addr, listener, self.client) };
        }
    }
}

unsafe extern "C" fn listener(
    _id: AudioObjectID,
    n: u32,
    addrs: *const AudioObjectPropertyAddress,
    client: *mut c_void,
) -> OSStatus {
    // SAFETY: `client` è il Notifier registrato in watch(), mai liberato.
    let notifier = unsafe { &*(client as *const Notifier) };
    // SAFETY: il HAL passa `n` indirizzi validi.
    let addrs = unsafe { std::slice::from_raw_parts(addrs, n as usize) };
    for a in addrs {
        let ev = match a.selector {
            HARDWARE_DEFAULT_OUTPUT => DeviceEvent::DefaultChanged { render: true },
            HARDWARE_DEFAULT_INPUT => DeviceEvent::DefaultChanged { render: false },
            _ => DeviceEvent::Changed,
        };
        (notifier.f)(ev);
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_virtual_cables() {
        assert!(is_cable_name("BlackHole 2ch"));
        assert!(is_cable_name("VB-Cable"));
        assert!(!is_cable_name("Altoparlanti MacBook Pro"));
        assert!(!is_cable_name("Microfono MacBook Pro"));
    }

    /// Prova sul Mac reale: `cargo test -p mfx-platform -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn list_real_devices() {
        let d = Devices::new().unwrap();
        for e in d.render_endpoints().unwrap() {
            println!("R {e:?}");
        }
        for e in d.capture_endpoints().unwrap() {
            println!("C {e:?}");
        }
    }
}
