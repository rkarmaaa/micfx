//! Aggiornamento automatico (tauri-plugin-updater). Solo su richiesta della UI, quando la finestra è
//! aperta: a riposo nessuna rete. Il `latest.json` dell'ultima release pubblicata su GitHub indica la
//! versione e, per ogni sistema, il file firmato (installer NSIS su Windows, `.app.tar.gz` su Mac);
//! la firma (minisign, chiave pubblica in tauri.conf.json) si verifica prima di installare.
//!
//! Windows: l'installer parte in modalità passiva, chiude MicFX e lo riapre aggiornato.
//! Mac: il bundle in Applicazioni viene sostituito e MicFX si riavvia.

use std::sync::Mutex;

use mfx_core::{ApiError, ApiResult, CoreHandle, ErrorCode, UpdateInfo, UpdateProgress};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::window;

/// L'ultima versione trovata da `check_update`, pronta per `install_update`.
#[derive(Default)]
pub struct PendingUpdate(Mutex<Option<Update>>);

fn failed(e: impl std::fmt::Display) -> ApiError {
    tracing::warn!("aggiornamento: {e}");
    ApiError::new(ErrorCode::UpdateFailed, e.to_string())
}

async fn find(app: &AppHandle) -> ApiResult<Option<Update>> {
    let core = app.state::<CoreHandle>().inner().clone();
    let updater = app
        .updater_builder()
        // Windows: l'installer chiude il processo subito dopo l'avvio. Prima si ferma il core (PTT
        // rilasciato, configurazione salvata), come con "Esci".
        .on_before_exit(move || core.shutdown())
        .build()
        .map_err(failed)?;
    updater.check().await.map_err(failed)
}

/// Cerca una versione più recente di quella in uso. `None` = MicFX è aggiornato.
#[tauri::command(rename_all = "snake_case")]
pub async fn check_update(app: AppHandle, pending: State<'_, PendingUpdate>) -> ApiResult<Option<UpdateInfo>> {
    let update = find(&app).await?;
    let info = update.as_ref().map(|u| UpdateInfo { version: u.version.clone(), notes: u.body.clone() });
    *pending.0.lock().unwrap() = update;
    Ok(info)
}

/// Scarica, verifica e installa l'aggiornamento, poi riavvia MicFX. Avanzamento con `update:progress`.
#[tauri::command(rename_all = "snake_case")]
pub async fn install_update(app: AppHandle, pending: State<'_, PendingUpdate>) -> ApiResult<()> {
    let taken = pending.0.lock().unwrap().take();
    let update = match taken {
        Some(u) => u,
        // L'avviso può venire da un controllo di un'apertura precedente: si ricontrolla.
        None => find(&app).await?.ok_or_else(|| failed("nessun aggiornamento disponibile"))?,
    };
    tracing::info!("aggiornamento a {}", update.version);
    let mut downloaded = 0u64;
    let mut last_pct = u64::MAX;
    let progress_app = app.clone();
    update
        .download_and_install(
            move |chunk, total| {
                downloaded += chunk as u64;
                // Un evento per punto percentuale, non uno per blocco di rete.
                let pct = total.map_or(downloaded >> 20, |t| downloaded * 100 / t.max(1));
                if pct != last_pct {
                    last_pct = pct;
                    if progress_app.get_webview_window(window::MAIN).is_some() {
                        let _ =
                            progress_app.emit_to(window::MAIN, "update:progress", UpdateProgress { downloaded, total });
                    }
                }
            },
            || {},
        )
        .await
        .map_err(failed)?;
    // Mac: il nuovo bundle è al suo posto. Windows arriva qui solo se l'installer non ha chiuso l'app.
    if let Some(core) = app.try_state::<CoreHandle>() {
        core.shutdown();
    }
    app.restart();
}
