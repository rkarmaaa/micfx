//! Evento Win32 auto-reset: il meccanismo di risveglio di tutti i thread di MicFX (SPEC §4).

use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_EVENT, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::System::Threading::{
    CreateEventW, INFINITE, SetEvent, WaitForMultipleObjects, WaitForSingleObject,
};
use windows::core::PCWSTR;

pub struct Event(HANDLE);

// SAFETY: un HANDLE di evento è usabile da qualsiasi thread.
unsafe impl Send for Event {}
unsafe impl Sync for Event {}

pub enum Wait {
    Signaled(usize),
    Timeout,
    Failed,
}

impl Event {
    pub fn new() -> std::io::Result<Self> {
        // SAFETY: parametri validi; il handle viene chiuso in Drop.
        let h = unsafe { CreateEventW(None, false, false, PCWSTR::null()) }.map_err(std::io::Error::other)?;
        Ok(Self(h))
    }

    #[inline]
    pub fn handle(&self) -> HANDLE {
        self.0
    }

    #[inline]
    pub fn set(&self) {
        // SAFETY: handle valido per tutta la vita di self.
        let _ = unsafe { SetEvent(self.0) };
    }

    /// Attende questo evento. `None` = senza limite.
    pub fn wait(&self, timeout_ms: Option<u32>) -> Wait {
        // SAFETY: handle valido.
        let r = unsafe { WaitForSingleObject(self.0, timeout_ms.unwrap_or(INFINITE)) };
        map_wait(r)
    }
}

impl Drop for Event {
    fn drop(&mut self) {
        // SAFETY: handle creato in new() e mai chiuso altrove.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

impl crate::sink::Wake for Event {
    fn wake(&self) {
        self.set();
    }
}

/// Attende il primo di più handle.
pub fn wait_any(handles: &[HANDLE], timeout_ms: Option<u32>) -> Wait {
    // SAFETY: handle validi forniti dal chiamante.
    let r = unsafe { WaitForMultipleObjects(handles, false, timeout_ms.unwrap_or(INFINITE)) };
    map_wait(r)
}

fn map_wait(r: WAIT_EVENT) -> Wait {
    if r == WAIT_TIMEOUT {
        Wait::Timeout
    } else if r.0 < WAIT_OBJECT_0.0 + 64 {
        Wait::Signaled((r.0 - WAIT_OBJECT_0.0) as usize)
    } else {
        Wait::Failed
    }
}
