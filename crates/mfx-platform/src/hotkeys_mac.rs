//! Hotkey globali su macOS (SPEC §8.1): `RegisterEventHotKey` di Carbon.
//!
//! È l'API che non chiede permessi all'utente (a differenza di un event tap, che richiede
//! "Monitoraggio input"). Registrazioni ed evento arrivano sul thread principale, dove gira il run
//! loop di Cocoa: le richieste dell'engine ci vengono accodate in modo asincrono (vedi
//! [`crate::main_queue`]). Un rifiuto di Carbon torna all'engine con `on_failed`.
//! Il callback dell'evento solo inoltra e ritorna subito: nessun polling, nessun thread in più.

use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr;
use std::sync::{Arc, Mutex};

use crate::accel::Accelerator;
use crate::main_queue::run_on_main;

type OSStatus = i32;
type EventRef = *mut c_void;
type EventTargetRef = *mut c_void;
type EventHotKeyRef = *mut c_void;
type EventHandlerCallRef = *mut c_void;
type EventHandlerUPP = unsafe extern "C" fn(EventHandlerCallRef, EventRef, *mut c_void) -> OSStatus;

#[repr(C)]
struct EventTypeSpec {
    event_class: u32,
    event_kind: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct EventHotKeyID {
    signature: u32,
    id: u32,
}

const fn fourcc(b: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*b)
}

const EVENT_CLASS_KEYBOARD: u32 = fourcc(b"keyb");
const EVENT_HOTKEY_PRESSED: u32 = 5;
const EVENT_PARAM_DIRECT_OBJECT: u32 = fourcc(b"----");
const TYPE_EVENT_HOTKEY_ID: u32 = fourcc(b"hkid");
const EVENT_NOT_HANDLED_ERR: OSStatus = -9874;
const EVENT_HOTKEY_EXISTS_ERR: OSStatus = -9878;
/// Firma delle hotkey di MicFX: distingue le nostre da quelle di altri componenti del processo.
const SIGNATURE: u32 = fourcc(b"MiFX");

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn GetApplicationEventTarget() -> EventTargetRef;
    fn InstallEventHandler(
        target: EventTargetRef,
        handler: EventHandlerUPP,
        num_types: usize,
        list: *const EventTypeSpec,
        user_data: *mut c_void,
        out_ref: *mut *mut c_void,
    ) -> OSStatus;
    fn GetEventParameter(
        event: EventRef,
        name: u32,
        desired_type: u32,
        actual_type: *mut u32,
        buffer_size: usize,
        actual_size: *mut usize,
        data: *mut c_void,
    ) -> OSStatus;
    fn RegisterEventHotKey(
        key_code: u32,
        modifiers: u32,
        id: EventHotKeyID,
        target: EventTargetRef,
        options: u32,
        out_ref: *mut EventHotKeyRef,
    ) -> OSStatus;
    fn UnregisterEventHotKey(hotkey: EventHotKeyRef) -> OSStatus;
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HotkeyError {
    #[error("combinazione già usata da un'altra applicazione")]
    Conflict,
    #[error("registrazione rifiutata da macOS: {0}")]
    Failed(String),
    #[error("servizio hotkey non attivo")]
    Stopped,
}

struct Shared {
    on_hotkey: Box<dyn Fn(i32) + Send + Sync>,
    /// `(id, conflitto)`: Carbon ha rifiutato una registrazione già confermata all'engine.
    on_failed: Box<dyn Fn(i32, bool) + Send + Sync>,
    /// Usata solo dal thread principale.
    refs: Mutex<HashMap<i32, usize>>,
}

pub struct HotkeyService {
    shared: Arc<Shared>,
}

impl HotkeyService {
    /// Prepara il servizio. Le callback girano sul thread principale: devono solo inoltrare.
    pub fn spawn(
        on_hotkey: impl Fn(i32) + Send + Sync + 'static,
        on_failed: impl Fn(i32, bool) + Send + Sync + 'static,
    ) -> std::io::Result<Self> {
        let shared =
            Arc::new(Shared { on_hotkey: Box::new(on_hotkey), on_failed: Box::new(on_failed), refs: Mutex::default() });
        // Il gestore dell'evento resta installato per tutta la vita del processo, con un suo Arc.
        let user = Arc::into_raw(shared.clone()) as usize;
        run_on_main(move || {
            let spec = EventTypeSpec { event_class: EVENT_CLASS_KEYBOARD, event_kind: EVENT_HOTKEY_PRESSED };
            // SAFETY: thread principale; `user` è un Arc<Shared> mai rilasciato.
            let r = unsafe {
                InstallEventHandler(
                    GetApplicationEventTarget(),
                    handler,
                    1,
                    &spec,
                    user as *mut c_void,
                    ptr::null_mut(),
                )
            };
            if r != 0 {
                tracing::error!("gestore hotkey Carbon non installato: {r}");
            }
        });
        Ok(Self { shared })
    }

    /// Registra (o ri-registra) `id`. La conferma definitiva arriva da Carbon sul thread principale:
    /// qui si rifiuta subito solo un tasto che sul Mac non esiste.
    pub fn register(&self, id: i32, accel: Accelerator) -> Result<(), HotkeyError> {
        let code =
            accel.key.mac_keycode().ok_or_else(|| HotkeyError::Failed("tasto non disponibile sul Mac".into()))?;
        let mods = accel.mods.to_carbon();
        let sh = self.shared.clone();
        run_on_main(move || sh.register_now(id, code as u32, mods));
        Ok(())
    }

    pub fn unregister(&self, id: i32) {
        let sh = self.shared.clone();
        run_on_main(move || sh.unregister_now(id));
    }
}

impl Drop for HotkeyService {
    fn drop(&mut self) {
        let sh = self.shared.clone();
        run_on_main(move || {
            let ids: Vec<i32> = sh.refs.lock().unwrap().keys().copied().collect();
            for id in ids {
                sh.unregister_now(id);
            }
        });
    }
}

impl Shared {
    fn register_now(&self, id: i32, code: u32, mods: u32) {
        self.unregister_now(id);
        let mut out: EventHotKeyRef = ptr::null_mut();
        let hk = EventHotKeyID { signature: SIGNATURE, id: id as u32 };
        // SAFETY: thread principale (chiamato da run_on_main).
        let r = unsafe { RegisterEventHotKey(code, mods, hk, GetApplicationEventTarget(), 0, &mut out) };
        if r == 0 && !out.is_null() {
            self.refs.lock().unwrap().insert(id, out as usize);
        } else {
            tracing::warn!("hotkey {id} rifiutata da Carbon: {r}");
            (self.on_failed)(id, r == EVENT_HOTKEY_EXISTS_ERR);
        }
    }

    fn unregister_now(&self, id: i32) {
        if let Some(r) = self.refs.lock().unwrap().remove(&id) {
            // SAFETY: riferimento restituito da RegisterEventHotKey e non ancora rilasciato.
            unsafe { UnregisterEventHotKey(r as EventHotKeyRef) };
        }
    }
}

unsafe extern "C" fn handler(_call: EventHandlerCallRef, event: EventRef, user: *mut c_void) -> OSStatus {
    // SAFETY: `user` è l'Arc<Shared> installato in spawn(), mai rilasciato.
    let sh = unsafe { &*(user as *const Shared) };
    let mut hk = EventHotKeyID { signature: 0, id: 0 };
    // SAFETY: legge un EventHotKeyID dentro un buffer della sua dimensione.
    let r = unsafe {
        GetEventParameter(
            event,
            EVENT_PARAM_DIRECT_OBJECT,
            TYPE_EVENT_HOTKEY_ID,
            ptr::null_mut(),
            size_of::<EventHotKeyID>(),
            ptr::null_mut(),
            (&raw mut hk).cast(),
        )
    };
    if r == 0 && hk.signature == SIGNATURE {
        (sh.on_hotkey)(hk.id as i32);
        0
    } else {
        EVENT_NOT_HANDLED_ERR
    }
}
