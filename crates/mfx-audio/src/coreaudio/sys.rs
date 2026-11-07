//! Dichiarazioni C di CoreAudio, AudioToolbox e CoreFoundation usate da MicFX.
#![allow(non_snake_case)]

use std::ffi::{c_char, c_void};

pub type OSStatus = i32;
pub type AudioObjectID = u32;
pub type AudioUnit = *mut c_void;
pub type AudioComponent = *mut c_void;
pub type CFStringRef = *const c_void;
pub type CFIndex = isize;

const fn fourcc(b: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*b)
}

// ------------------------------------------------------------------ HAL

pub const SYSTEM_OBJECT: AudioObjectID = 1;
pub const UNKNOWN_OBJECT: AudioObjectID = 0;
pub const ELEMENT_MAIN: u32 = 0;
pub const SCOPE_GLOBAL: u32 = fourcc(b"glob");
pub const SCOPE_INPUT: u32 = fourcc(b"inpt");
pub const SCOPE_OUTPUT: u32 = fourcc(b"outp");

pub const HARDWARE_DEVICES: u32 = fourcc(b"dev#");
pub const HARDWARE_DEFAULT_OUTPUT: u32 = fourcc(b"dOut");
pub const HARDWARE_DEFAULT_INPUT: u32 = fourcc(b"dIn ");
pub const OBJECT_NAME: u32 = fourcc(b"lnam");
pub const DEVICE_UID: u32 = fourcc(b"uid ");
pub const DEVICE_NOMINAL_SAMPLE_RATE: u32 = fourcc(b"nsrt");
pub const DEVICE_STREAM_CONFIGURATION: u32 = fourcc(b"slay");
pub const DEVICE_IS_ALIVE: u32 = fourcc(b"livn");
pub const DEVICE_HOG_MODE: u32 = fourcc(b"oink");
pub const DEVICE_PROCESSOR_OVERLOAD: u32 = fourcc(b"over");
/// `kAudioHardwareBadDeviceError`: dispositivo sparito.
pub const ERR_BAD_DEVICE: OSStatus = fourcc(b"!dev") as OSStatus;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct AudioObjectPropertyAddress {
    pub selector: u32,
    pub scope: u32,
    pub element: u32,
}

pub type AudioObjectPropertyListenerProc =
    unsafe extern "C" fn(AudioObjectID, u32, *const AudioObjectPropertyAddress, *mut c_void) -> OSStatus;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct AudioBuffer {
    pub channels: u32,
    pub data_size: u32,
    pub data: *mut c_void,
}

#[repr(C)]
pub struct AudioBufferList {
    pub count: u32,
    pub buffers: [AudioBuffer; 1],
}

/// Opaca: MicFX la passa soltanto da una callback ad `AudioUnitRender`.
#[repr(C)]
pub struct AudioTimeStamp {
    _private: [u8; 0],
}

#[link(name = "CoreAudio", kind = "framework")]
unsafe extern "C" {
    pub fn AudioObjectGetPropertyDataSize(
        id: AudioObjectID,
        address: *const AudioObjectPropertyAddress,
        qualifier_size: u32,
        qualifier: *const c_void,
        out_size: *mut u32,
    ) -> OSStatus;
    pub fn AudioObjectGetPropertyData(
        id: AudioObjectID,
        address: *const AudioObjectPropertyAddress,
        qualifier_size: u32,
        qualifier: *const c_void,
        io_size: *mut u32,
        out_data: *mut c_void,
    ) -> OSStatus;
    pub fn AudioObjectAddPropertyListener(
        id: AudioObjectID,
        address: *const AudioObjectPropertyAddress,
        listener: AudioObjectPropertyListenerProc,
        client_data: *mut c_void,
    ) -> OSStatus;
    pub fn AudioObjectRemovePropertyListener(
        id: AudioObjectID,
        address: *const AudioObjectPropertyAddress,
        listener: AudioObjectPropertyListenerProc,
        client_data: *mut c_void,
    ) -> OSStatus;
}

// ------------------------------------------------------------------ AudioUnit

pub const AU_TYPE_OUTPUT: u32 = fourcc(b"auou");
pub const AU_SUBTYPE_HAL_OUTPUT: u32 = fourcc(b"ahal");
pub const AU_MANUFACTURER_APPLE: u32 = fourcc(b"appl");

pub const AU_SCOPE_GLOBAL: u32 = 0;
pub const AU_SCOPE_INPUT: u32 = 1;
pub const AU_SCOPE_OUTPUT: u32 = 2;

pub const AU_PROP_STREAM_FORMAT: u32 = 8;
pub const AU_PROP_MAXIMUM_FRAMES_PER_SLICE: u32 = 14;
pub const AU_PROP_SET_RENDER_CALLBACK: u32 = 23;
pub const AU_PROP_CURRENT_DEVICE: u32 = 2000;
pub const AU_PROP_ENABLE_IO: u32 = 2003;
pub const AU_PROP_SET_INPUT_CALLBACK: u32 = 2005;

pub const FORMAT_LINEAR_PCM: u32 = fourcc(b"lpcm");
pub const FORMAT_FLAG_IS_FLOAT: u32 = 1 << 0;
pub const FORMAT_FLAG_IS_PACKED: u32 = 1 << 3;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct AudioComponentDescription {
    pub component_type: u32,
    pub component_sub_type: u32,
    pub component_manufacturer: u32,
    pub component_flags: u32,
    pub component_flags_mask: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct AudioStreamBasicDescription {
    pub sample_rate: f64,
    pub format_id: u32,
    pub format_flags: u32,
    pub bytes_per_packet: u32,
    pub frames_per_packet: u32,
    pub bytes_per_frame: u32,
    pub channels_per_frame: u32,
    pub bits_per_channel: u32,
    pub reserved: u32,
}

pub type AURenderCallback = unsafe extern "C" fn(
    ref_con: *mut c_void,
    action_flags: *mut u32,
    time_stamp: *const AudioTimeStamp,
    bus_number: u32,
    number_frames: u32,
    data: *mut AudioBufferList,
) -> OSStatus;

#[repr(C)]
pub struct AURenderCallbackStruct {
    pub input_proc: AURenderCallback,
    pub input_proc_ref_con: *mut c_void,
}

#[link(name = "AudioToolbox", kind = "framework")]
unsafe extern "C" {
    pub fn AudioComponentFindNext(component: AudioComponent, desc: *const AudioComponentDescription) -> AudioComponent;
    pub fn AudioComponentInstanceNew(component: AudioComponent, out: *mut AudioUnit) -> OSStatus;
    pub fn AudioComponentInstanceDispose(unit: AudioUnit) -> OSStatus;
    pub fn AudioUnitSetProperty(
        unit: AudioUnit,
        id: u32,
        scope: u32,
        element: u32,
        data: *const c_void,
        size: u32,
    ) -> OSStatus;
    pub fn AudioUnitInitialize(unit: AudioUnit) -> OSStatus;
    pub fn AudioUnitUninitialize(unit: AudioUnit) -> OSStatus;
    pub fn AudioOutputUnitStart(unit: AudioUnit) -> OSStatus;
    pub fn AudioOutputUnitStop(unit: AudioUnit) -> OSStatus;
    pub fn AudioUnitRender(
        unit: AudioUnit,
        action_flags: *mut u32,
        time_stamp: *const AudioTimeStamp,
        output_bus: u32,
        number_frames: u32,
        data: *mut AudioBufferList,
    ) -> OSStatus;
}

// ------------------------------------------------------------------ CoreFoundation

pub const CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    pub fn CFRelease(cf: *const c_void);
    pub fn CFStringGetLength(s: CFStringRef) -> CFIndex;
    pub fn CFStringGetMaximumSizeForEncoding(length: CFIndex, encoding: u32) -> CFIndex;
    pub fn CFStringGetCString(s: CFStringRef, buffer: *mut c_char, size: CFIndex, encoding: u32) -> u8;
}
