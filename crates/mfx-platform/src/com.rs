//! Inizializzazione COM per thread.

use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};

/// COM MTA per la durata della guardia.
pub struct ComGuard {
    owned: bool,
}

impl ComGuard {
    pub fn mta() -> Self {
        // SAFETY: bilanciata da CoUninitialize in Drop solo se riuscita.
        let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        Self { owned: hr.is_ok() }
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.owned {
            // SAFETY: CoInitializeEx riuscito su questo thread.
            unsafe { CoUninitialize() };
        }
    }
}
