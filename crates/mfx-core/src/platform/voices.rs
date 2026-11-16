//! Bridge della voce, uguali su Windows e macOS: quello verso il cavo (gli altri ti sentono) e quello
//! della prova del microfono (ti senti in cuffia, come il test di Discord). Stesso percorso, stessa
//! catena: nella prova senti la voce come la sentono gli altri.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use mfx_audio::bridge::MicBridge;
use mfx_audio::voice::AtomicF32;

use super::MicRoute;

pub(crate) struct Voices {
    main: Option<(MicRoute, MicBridge)>,
    test: Option<(MicRoute, MicBridge)>,
    /// "Disattiva la mia voce", condiviso con ogni bridge verso il cavo: uno riaperto durante un
    /// suono resta muto. La prova in cuffia non si zittisce.
    muted: Arc<AtomicBool>,
    /// Intensità della riduzione del rumore (limite in dB), letta dai bridge a ogni frame.
    limit: Arc<AtomicF32>,
}

impl Voices {
    pub fn new() -> Self {
        Self { main: None, test: None, muted: Arc::new(AtomicBool::new(false)), limit: Arc::new(AtomicF32::new(100.0)) }
    }

    pub fn set_main(&mut self, route: Option<MicRoute>) {
        let muted = self.muted.clone();
        set(&mut self.main, route, muted, &self.limit, "bridge microfono");
    }

    pub fn set_test(&mut self, route: Option<MicRoute>) {
        set(&mut self.test, route, Arc::new(AtomicBool::new(false)), &self.limit, "prova microfono");
    }

    pub fn set_muted(&self, muted: bool) {
        self.muted.store(muted, Ordering::Relaxed);
    }

    pub fn set_limit(&self, db: f32) {
        self.limit.store(db);
    }
}

/// Accende, sposta o spegne un bridge. Uno che si è fermato da solo (dispositivo cambiato o
/// scollegato) si riapre anche se la richiesta è la stessa.
fn set(
    slot: &mut Option<(MicRoute, MicBridge)>,
    route: Option<MicRoute>,
    muted: Arc<AtomicBool>,
    limit: &Arc<AtomicF32>,
    what: &str,
) {
    if let Some((r, b)) = slot.as_ref()
        && Some(r) == route.as_ref()
    {
        if b.is_alive() {
            return;
        }
        tracing::warn!("{what} interrotto (dispositivo cambiato o scollegato): lo riapro");
    }
    // Prima si chiude il vecchio: un solo flusso di voce per uscita.
    *slot = None;
    let Some(route) = route else {
        tracing::info!("{what} spento");
        return;
    };
    let denoise = route.denoise.then(|| limit.clone());
    match MicBridge::start(route.mic_id.as_deref(), route.output_id.as_deref(), denoise, muted) {
        Ok(b) => {
            tracing::info!(
                "{what}: {:?} → {:?} (riduzione del rumore: {})",
                route.mic_id,
                route.output_id,
                route.denoise
            );
            *slot = Some((route, b));
        }
        Err(e) => tracing::warn!("{what} non avviato: {e}"),
    }
}
