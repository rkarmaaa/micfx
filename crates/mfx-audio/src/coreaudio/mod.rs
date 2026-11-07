//! CoreAudio (macOS): binding minime scritte a mano e funzioni d'aiuto sul HAL.
//!
//! Solo le poche funzioni C che servono a MicFX, senza bindgen: niente dipendenza da libclang in
//! build e nessun crate in più. Le costanti sono i FourCC degli header di Apple.

pub mod sys;

use std::ffi::c_void;
use std::ptr;

use sys::*;

/// Proprietà del HAL a dimensione fissa.
pub fn get_prop<T: Copy>(obj: AudioObjectID, selector: u32, scope: u32) -> Option<T> {
    let addr = AudioObjectPropertyAddress { selector, scope, element: ELEMENT_MAIN };
    let mut size = size_of::<T>() as u32;
    let mut out = std::mem::MaybeUninit::<T>::uninit();
    // SAFETY: buffer della dimensione dichiarata; il HAL scrive al più `size` byte.
    let r = unsafe { AudioObjectGetPropertyData(obj, &addr, 0, ptr::null(), &mut size, out.as_mut_ptr().cast()) };
    // SAFETY: il HAL ha riempito il valore (r == 0 e dimensione intera).
    (r == 0 && size as usize == size_of::<T>()).then(|| unsafe { out.assume_init() })
}

/// Proprietà del HAL a lunghezza variabile (array di `T`).
pub fn get_vec<T: Copy + Default>(obj: AudioObjectID, selector: u32, scope: u32) -> Vec<T> {
    let addr = AudioObjectPropertyAddress { selector, scope, element: ELEMENT_MAIN };
    let mut size = 0u32;
    // SAFETY: interroga solo la dimensione.
    if unsafe { AudioObjectGetPropertyDataSize(obj, &addr, 0, ptr::null(), &mut size) } != 0 || size == 0 {
        return Vec::new();
    }
    let mut v = vec![T::default(); size as usize / size_of::<T>()];
    // SAFETY: buffer di `size` byte.
    let r = unsafe { AudioObjectGetPropertyData(obj, &addr, 0, ptr::null(), &mut size, v.as_mut_ptr().cast()) };
    if r != 0 {
        return Vec::new();
    }
    v.truncate(size as usize / size_of::<T>());
    v
}

/// Proprietà di tipo `CFStringRef` (il chiamante del HAL la possiede e va rilasciata).
pub fn get_string(obj: AudioObjectID, selector: u32, scope: u32) -> Option<String> {
    let s: CFStringRef = get_prop(obj, selector, scope)?;
    if s.is_null() {
        return None;
    }
    let out = cfstring_to_string(s);
    // SAFETY: riferimento restituito dal HAL con regola "copy": va rilasciato una volta.
    unsafe { CFRelease(s) };
    out
}

fn cfstring_to_string(s: CFStringRef) -> Option<String> {
    // SAFETY: `s` è una CFString valida; il buffer è dimensionato per la codifica UTF-8.
    unsafe {
        let len = CFStringGetLength(s);
        let cap = CFStringGetMaximumSizeForEncoding(len, CF_STRING_ENCODING_UTF8) + 1;
        let mut buf = vec![0u8; cap.max(1) as usize];
        if CFStringGetCString(s, buf.as_mut_ptr().cast(), cap, CF_STRING_ENCODING_UTF8) == 0 {
            return None;
        }
        let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        buf.truncate(end);
        String::from_utf8(buf).ok()
    }
}

pub fn device_ids() -> Vec<AudioObjectID> {
    get_vec(SYSTEM_OBJECT, HARDWARE_DEVICES, SCOPE_GLOBAL)
}

pub fn default_device(output: bool) -> Option<AudioObjectID> {
    let sel = if output { HARDWARE_DEFAULT_OUTPUT } else { HARDWARE_DEFAULT_INPUT };
    get_prop::<AudioObjectID>(SYSTEM_OBJECT, sel, SCOPE_GLOBAL).filter(|&id| id != UNKNOWN_OBJECT)
}

pub fn device_uid(id: AudioObjectID) -> Option<String> {
    get_string(id, DEVICE_UID, SCOPE_GLOBAL)
}

pub fn device_name(id: AudioObjectID) -> Option<String> {
    get_string(id, OBJECT_NAME, SCOPE_GLOBAL)
}

pub fn nominal_rate(id: AudioObjectID) -> Option<f64> {
    get_prop::<f64>(id, DEVICE_NOMINAL_SAMPLE_RATE, SCOPE_GLOBAL)
}

/// Canali del dispositivo nella direzione indicata (0 = nessuno stream in quella direzione).
pub fn channels(id: AudioObjectID, output: bool) -> u32 {
    let scope = if output { SCOPE_OUTPUT } else { SCOPE_INPUT };
    let addr = AudioObjectPropertyAddress { selector: DEVICE_STREAM_CONFIGURATION, scope, element: ELEMENT_MAIN };
    let mut size = 0u32;
    // SAFETY: interroga solo la dimensione.
    if unsafe { AudioObjectGetPropertyDataSize(id, &addr, 0, ptr::null(), &mut size) } != 0 || size == 0 {
        return 0;
    }
    // AudioBufferList di lunghezza variabile: buffer allineato a 8 byte.
    let mut raw = vec![0u64; (size as usize).div_ceil(8)];
    // SAFETY: buffer di almeno `size` byte.
    let r = unsafe { AudioObjectGetPropertyData(id, &addr, 0, ptr::null(), &mut size, raw.as_mut_ptr().cast()) };
    if r != 0 {
        return 0;
    }
    let list = raw.as_ptr() as *const AudioBufferList;
    // SAFETY: il HAL ha scritto un AudioBufferList con `count` elementi dentro `raw`.
    unsafe {
        let n = (*list).count as usize;
        let first = (*list).buffers.as_ptr();
        (0..n).map(|i| (*first.add(i)).channels).sum()
    }
}

/// Dispositivo con questo UID (gli ID numerici cambiano a ogni riavvio, gli UID no).
pub fn device_by_uid(uid: &str) -> Option<AudioObjectID> {
    device_ids().into_iter().find(|&id| device_uid(id).as_deref() == Some(uid))
}

/// Il dispositivo è tenuto in "hog mode" (uso esclusivo) da un altro processo?
pub fn hogged_by_other(id: AudioObjectID) -> bool {
    match get_prop::<i32>(id, DEVICE_HOG_MODE, SCOPE_GLOBAL) {
        Some(pid) => pid != -1 && pid != std::process::id() as i32,
        None => false,
    }
}

/// Crea un'istanza AUHAL (`kAudioUnitSubType_HALOutput`).
pub fn new_hal_unit() -> Result<AudioUnit, OSStatus> {
    let desc = AudioComponentDescription {
        component_type: AU_TYPE_OUTPUT,
        component_sub_type: AU_SUBTYPE_HAL_OUTPUT,
        component_manufacturer: AU_MANUFACTURER_APPLE,
        component_flags: 0,
        component_flags_mask: 0,
    };
    // SAFETY: descrizione valida; l'istanza va distrutta con AudioComponentInstanceDispose.
    unsafe {
        let comp = AudioComponentFindNext(ptr::null_mut(), &desc);
        if comp.is_null() {
            return Err(-1);
        }
        let mut unit: AudioUnit = ptr::null_mut();
        let r = AudioComponentInstanceNew(comp, &mut unit);
        if r != 0 || unit.is_null() {
            return Err(if r != 0 { r } else { -1 });
        }
        Ok(unit)
    }
}

pub(crate) fn set_unit_prop<T>(
    unit: AudioUnit,
    prop: u32,
    scope: u32,
    element: u32,
    value: &T,
) -> Result<(), OSStatus> {
    // SAFETY: `value` vive per tutta la chiamata; AudioUnit copia il valore.
    let r = unsafe {
        AudioUnitSetProperty(unit, prop, scope, element, (value as *const T).cast::<c_void>(), size_of::<T>() as u32)
    };
    if r == 0 { Ok(()) } else { Err(r) }
}

/// Formato di MicFX: float 32 bit interleaved, `channels` canali, `rate` Hz.
pub fn float_format(rate: f64, channels: u32) -> AudioStreamBasicDescription {
    AudioStreamBasicDescription {
        sample_rate: rate,
        format_id: FORMAT_LINEAR_PCM,
        format_flags: FORMAT_FLAG_IS_FLOAT | FORMAT_FLAG_IS_PACKED,
        bytes_per_packet: 4 * channels,
        frames_per_packet: 1,
        bytes_per_frame: 4 * channels,
        channels_per_frame: channels,
        bits_per_channel: 32,
        reserved: 0,
    }
}

/// Distrugge un'AudioUnit: ferma, de-inizializza, rilascia.
///
/// # Safety
/// `unit` valida e non più usata dopo la chiamata.
pub unsafe fn dispose_unit(unit: AudioUnit) {
    // SAFETY: vedi contratto della funzione.
    unsafe {
        AudioOutputUnitStop(unit);
        AudioUnitUninitialize(unit);
        AudioComponentInstanceDispose(unit);
    }
}
