//! Relay push-to-talk su macOS (SPEC §8.2): `CGEventPost` di un tasto per key code di posizione.
//!
//! macOS consente di simulare tasti solo alle app autorizzate in Impostazioni di Sistema →
//! Privacy e sicurezza → Accessibilità. Senza permesso `send_scancode` restituisce `false` e,
//! la prima volta, macOS mostra la richiesta.

use std::ffi::c_void;
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::accel::Key;

const HID_EVENT_TAP: u32 = 0;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventCreateKeyboardEvent(source: *const c_void, key: u16, key_down: bool) -> *mut c_void;
    fn CGEventPost(tap: u32, event: *mut c_void);
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> u8;
    fn AXIsProcessTrustedWithOptions(options: *const c_void) -> u8;
    static kAXTrustedCheckOptionPrompt: *const c_void;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFBooleanTrue: *const c_void;
    static kCFTypeDictionaryKeyCallBacks: u8;
    static kCFTypeDictionaryValueCallBacks: u8;
    fn CFDictionaryCreate(
        allocator: *const c_void,
        keys: *const *const c_void,
        values: *const *const c_void,
        count: isize,
        key_callbacks: *const c_void,
        value_callbacks: *const c_void,
    ) -> *const c_void;
    fn CFRelease(cf: *const c_void);
}

static PROMPTED: AtomicBool = AtomicBool::new(false);

/// MicFX può simulare tasti? Con `prompt`, se no, macOS chiede il permesso (una volta per avvio).
pub fn accessibility_trusted(prompt: bool) -> bool {
    // SAFETY: funzione senza parametri.
    if unsafe { AXIsProcessTrusted() } != 0 {
        return true;
    }
    if prompt && !PROMPTED.swap(true, Ordering::Relaxed) {
        // SAFETY: dizionario CF di una coppia chiave/valore costanti, rilasciato subito dopo.
        unsafe {
            let keys = [kAXTrustedCheckOptionPrompt];
            let values = [kCFBooleanTrue];
            let dict = CFDictionaryCreate(
                ptr::null(),
                keys.as_ptr(),
                values.as_ptr(),
                1,
                (&raw const kCFTypeDictionaryKeyCallBacks).cast(),
                (&raw const kCFTypeDictionaryValueCallBacks).cast(),
            );
            if !dict.is_null() {
                let ok = AXIsProcessTrustedWithOptions(dict) != 0;
                CFRelease(dict);
                return ok;
            }
        }
    }
    false
}

/// Preme (`down = true`) o rilascia il tasto con questo scancode (quello salvato in config).
/// `false` se il tasto non esiste sul Mac o se manca il permesso Accessibilità.
pub fn send_scancode(scancode: u16, down: bool) -> bool {
    let Some(code) = Key::by_scancode(scancode).and_then(|k| k.mac_keycode()) else { return false };
    // Il rilascio si manda sempre (non deve mai restare premuto); la pressione solo con permesso.
    if down && !accessibility_trusted(true) {
        return false;
    }
    // SAFETY: evento creato, inviato e rilasciato qui.
    unsafe {
        let ev = CGEventCreateKeyboardEvent(ptr::null(), code, down);
        if ev.is_null() {
            return false;
        }
        CGEventPost(HID_EVENT_TAP, ev);
        CFRelease(ev);
    }
    true
}
