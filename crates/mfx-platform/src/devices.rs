//! Dispositivi audio (SPEC §9): enumerazione, riconoscimento di VB-Cable, notifiche e verifica di
//! "Ascolta questo dispositivo" sul microfono (modo Sistema, SPEC §6.6).
//!
//! Le callback di Windows arrivano su thread di sistema: inoltrano e ritornano subito.

use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Media::Audio::{
    DEVICE_STATE, DEVICE_STATE_ACTIVE, EDataFlow, ERole, IAudioClient, IMMDevice, IMMDeviceEnumerator,
    IMMNotificationClient, IMMNotificationClient_Impl, MMDeviceEnumerator, eCapture, eConsole, eRender,
};
use windows::Win32::System::Com::StructuredStorage::{PROPVARIANT, PropVariantClear, PropVariantToBSTR};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, CoTaskMemFree, STGM_READ};
use windows::Win32::System::Variant::{VT_BOOL, VT_EMPTY};
use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
use windows::core::{GUID, PCWSTR, implement};

/// Proprietà di "Ascolta questo dispositivo" sugli endpoint di cattura.
/// pid 1 = attivo (VT_BOOL), pid 0 = ID dell'endpoint di riproduzione (VT_LPWSTR, vuoto = predefinito).
const LISTEN_FMTID: GUID = GUID::from_u128(0x24dbb0fc_9311_4b3d_9cf0_18ff155639d4);
const PKEY_LISTEN_ENABLED: PROPERTYKEY = PROPERTYKEY { fmtid: LISTEN_FMTID, pid: 1 };
const PKEY_LISTEN_TARGET: PROPERTYKEY = PROPERTYKEY { fmtid: LISTEN_FMTID, pid: 0 };

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
    /// È CABLE Output (non è un microfono vero).
    pub is_cable: bool,
    pub listen_enabled: bool,
    /// `None` = "Dispositivo di riproduzione predefinito".
    pub listen_target: Option<String>,
}

/// VB-Cable si riconosce dal nome (SPEC §9).
pub fn is_cable_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("cable input") && n.contains("vb-audio")
}

fn is_cable_output_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("cable output") && n.contains("vb-audio")
}

/// Accesso agli endpoint. Va creato e usato su un thread con COM inizializzato.
pub struct Devices {
    enumerator: IMMDeviceEnumerator,
}

impl Devices {
    pub fn new() -> windows::core::Result<Self> {
        // SAFETY: COM inizializzato dal chiamante.
        let enumerator = unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? };
        Ok(Self { enumerator })
    }

    fn default_id(&self, flow: EDataFlow) -> Option<String> {
        // SAFETY: interfacce valide.
        unsafe {
            let d = self.enumerator.GetDefaultAudioEndpoint(flow, eConsole).ok()?;
            device_id(&d)
        }
    }

    pub fn render_endpoints(&self) -> windows::core::Result<Vec<RenderEndpoint>> {
        let default = self.default_id(eRender);
        let mut out = Vec::new();
        // SAFETY: interfacce valide, stringhe COM liberate in device_id/prop_string.
        unsafe {
            let coll = self.enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)?;
            for i in 0..coll.GetCount()? {
                let Ok(dev) = coll.Item(i) else { continue };
                let Some(id) = device_id(&dev) else { continue };
                let store = dev.OpenPropertyStore(STGM_READ).ok();
                let name = store
                    .as_ref()
                    .and_then(|s| prop_string(s, &PKEY_Device_FriendlyName))
                    .unwrap_or_else(|| id.clone());
                let mix_rate = mix_rate(&dev).unwrap_or(0);
                out.push(RenderEndpoint {
                    is_default: default.as_deref() == Some(id.as_str()),
                    is_cable: is_cable_name(&name),
                    id,
                    name,
                    mix_rate,
                });
            }
        }
        Ok(out)
    }

    pub fn capture_endpoints(&self) -> windows::core::Result<Vec<CaptureEndpoint>> {
        let default = self.default_id(eCapture);
        let mut out = Vec::new();
        // SAFETY: vedi render_endpoints.
        unsafe {
            let coll = self.enumerator.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE)?;
            for i in 0..coll.GetCount()? {
                let Ok(dev) = coll.Item(i) else { continue };
                let Some(id) = device_id(&dev) else { continue };
                let store = dev.OpenPropertyStore(STGM_READ).ok();
                let name = store
                    .as_ref()
                    .and_then(|s| prop_string(s, &PKEY_Device_FriendlyName))
                    .unwrap_or_else(|| id.clone());
                let listen_enabled = store.as_ref().and_then(|s| prop_bool(s, &PKEY_LISTEN_ENABLED)).unwrap_or(false);
                let listen_target =
                    store.as_ref().and_then(|s| prop_string(s, &PKEY_LISTEN_TARGET)).filter(|s| !s.is_empty());
                out.push(CaptureEndpoint {
                    is_default: default.as_deref() == Some(id.as_str()),
                    is_cable: is_cable_output_name(&name),
                    id,
                    name,
                    listen_enabled,
                    listen_target,
                });
            }
        }
        Ok(out)
    }

    /// Registra le notifiche di Windows. Restano attive finché il `Watcher` vive.
    pub fn watch(&self, on_event: impl Fn(DeviceEvent) + Send + Sync + 'static) -> windows::core::Result<Watcher> {
        let client: IMMNotificationClient = Notifier { f: Box::new(on_event) }.into();
        // SAFETY: client valido; deregistrato in Drop del Watcher.
        unsafe { self.enumerator.RegisterEndpointNotificationCallback(&client)? };
        Ok(Watcher { enumerator: self.enumerator.clone(), client })
    }
}

/// Esito di un controllo sul modo Sistema: il microfono arriva a CABLE Input?
pub fn mic_routed_to(captures: &[CaptureEndpoint], cable_id: &str, default_render_id: Option<&str>) -> bool {
    captures.iter().filter(|c| !c.is_cable).any(|c| {
        c.listen_enabled
            && match &c.listen_target {
                Some(t) => t.eq_ignore_ascii_case(cable_id),
                None => default_render_id.is_some_and(|d| d.eq_ignore_ascii_case(cable_id)),
            }
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceEvent {
    /// Aggiunto, rimosso o cambio di stato: va rifatta l'enumerazione.
    Changed,
    /// Nuovo dispositivo predefinito (riproduzione o cattura).
    DefaultChanged { render: bool },
    /// Cambiata una proprietà rilevante (nome, "Ascolta questo dispositivo").
    PropertyChanged,
}

pub struct Watcher {
    enumerator: IMMDeviceEnumerator,
    client: IMMNotificationClient,
}

impl Drop for Watcher {
    fn drop(&mut self) {
        // SAFETY: client registrato in watch().
        let _ = unsafe { self.enumerator.UnregisterEndpointNotificationCallback(&self.client) };
    }
}

#[implement(IMMNotificationClient)]
struct Notifier {
    f: Box<dyn Fn(DeviceEvent) + Send + Sync>,
}

impl IMMNotificationClient_Impl for Notifier_Impl {
    fn OnDeviceStateChanged(&self, _id: &PCWSTR, _state: DEVICE_STATE) -> windows::core::Result<()> {
        (self.f)(DeviceEvent::Changed);
        Ok(())
    }

    fn OnDeviceAdded(&self, _id: &PCWSTR) -> windows::core::Result<()> {
        (self.f)(DeviceEvent::Changed);
        Ok(())
    }

    fn OnDeviceRemoved(&self, _id: &PCWSTR) -> windows::core::Result<()> {
        (self.f)(DeviceEvent::Changed);
        Ok(())
    }

    fn OnDefaultDeviceChanged(&self, flow: EDataFlow, role: ERole, _id: &PCWSTR) -> windows::core::Result<()> {
        if role == eConsole {
            (self.f)(DeviceEvent::DefaultChanged { render: flow == eRender });
        }
        Ok(())
    }

    fn OnPropertyValueChanged(&self, _id: &PCWSTR, key: &PROPERTYKEY) -> windows::core::Result<()> {
        if key.fmtid == LISTEN_FMTID || *key == PKEY_Device_FriendlyName {
            (self.f)(DeviceEvent::PropertyChanged);
        }
        Ok(())
    }
}

unsafe fn device_id(dev: &IMMDevice) -> Option<String> {
    unsafe {
        let p = dev.GetId().ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as *const _));
        s
    }
}

unsafe fn mix_rate(dev: &IMMDevice) -> Option<u32> {
    unsafe {
        let client: IAudioClient = dev.Activate(CLSCTX_ALL, None).ok()?;
        let mix = client.GetMixFormat().ok()?;
        let rate = (*mix).nSamplesPerSec;
        CoTaskMemFree(Some(mix as *const _));
        Some(rate)
    }
}

unsafe fn prop_string(store: &IPropertyStore, key: &PROPERTYKEY) -> Option<String> {
    unsafe {
        let mut v: PROPVARIANT = store.GetValue(key).ok()?;
        let out =
            if v.Anonymous.Anonymous.vt == VT_EMPTY { None } else { PropVariantToBSTR(&v).ok().map(|b| b.to_string()) };
        let _ = PropVariantClear(&mut v);
        out
    }
}

unsafe fn prop_bool(store: &IPropertyStore, key: &PROPERTYKEY) -> Option<bool> {
    unsafe {
        let mut v: PROPVARIANT = store.GetValue(key).ok()?;
        let out = (v.Anonymous.Anonymous.vt == VT_BOOL).then(|| v.Anonymous.Anonymous.Anonymous.boolVal.0 != 0);
        let _ = PropVariantClear(&mut v);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_vb_cable() {
        assert!(is_cable_name("CABLE Input (VB-Audio Virtual Cable)"));
        assert!(!is_cable_name("Altoparlanti (Realtek(R) Audio)"));
        assert!(is_cable_output_name("CABLE Output (VB-Audio Virtual Cable)"));
    }

    #[test]
    fn mic_routing_check() {
        let mic = |enabled, target: Option<&str>| CaptureEndpoint {
            id: "mic".into(),
            name: "Microfono".into(),
            is_default: true,
            is_cable: false,
            listen_enabled: enabled,
            listen_target: target.map(Into::into),
        };
        assert!(mic_routed_to(&[mic(true, Some("CABLE"))], "cable", None));
        assert!(!mic_routed_to(&[mic(false, Some("cable"))], "cable", None));
        assert!(!mic_routed_to(&[mic(true, Some("cuffie"))], "cable", None));
        assert!(mic_routed_to(&[mic(true, None)], "cable", Some("cable")));
    }

    /// Prova sul PC reale: elenca gli endpoint. `cargo test -p mfx-platform -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn list_real_devices() {
        let _com = crate::com::ComGuard::mta();
        let d = Devices::new().unwrap();
        for e in d.render_endpoints().unwrap() {
            println!("R {:?}", e);
        }
        for e in d.capture_endpoints().unwrap() {
            println!("C {:?}", e);
        }
    }
}
