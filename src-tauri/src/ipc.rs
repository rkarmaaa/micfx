//! Comandi `#[tauri::command]` verso `mfx-core` (SPEC §12). Ogni comando ritorna
//! `Result<T, ApiError>` con codice stabile. Le chiamate al core bloccano finché l'engine risponde:
//! girano su un thread del pool, mai sul thread principale.

use mfx_core::{
    ApiError, ApiResult, BusKind, CoreHandle, Diagnostics, ErrorCode, Folder, Hotkey, HotkeyAction, OutputPatch,
    Settings, Snapshot, Sound, SoundPatch, StatusCode,
};
use tauri::{AppHandle, State, WebviewWindow};

async fn blocking<T: Send + 'static>(
    core: &State<'_, CoreHandle>,
    f: impl FnOnce(CoreHandle) -> ApiResult<T> + Send + 'static,
) -> ApiResult<T> {
    let core = core.inner().clone();
    tauri::async_runtime::spawn_blocking(move || f(core))
        .await
        .map_err(|e| ApiError::new(ErrorCode::Internal, e.to_string()))?
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_snapshot(core: State<'_, CoreHandle>) -> ApiResult<Snapshot> {
    blocking(&core, |c| c.get_snapshot()).await
}

/// La finestra è tornata in primo piano: il core ricontrolla Discord (risponde con eventi).
#[tauri::command(rename_all = "snake_case")]
pub async fn recheck_setup(core: State<'_, CoreHandle>) -> ApiResult<()> {
    blocking(&core, |c| c.recheck_setup()).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn play_sound(core: State<'_, CoreHandle>, sound_id: String) -> ApiResult<Option<u64>> {
    blocking(&core, move |c| c.play_sound(sound_id)).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn preview_sound(core: State<'_, CoreHandle>, sound_id: String) -> ApiResult<Option<u64>> {
    blocking(&core, move |c| c.preview_sound(sound_id)).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn stop_sound(core: State<'_, CoreHandle>, sound_id: String) -> ApiResult<()> {
    blocking(&core, move |c| c.stop_sound(sound_id)).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn stop_all(core: State<'_, CoreHandle>) -> ApiResult<()> {
    blocking(&core, |c| c.stop_all()).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn import_files(
    core: State<'_, CoreHandle>,
    paths: Vec<String>,
    folder_id: Option<String>,
) -> ApiResult<u64> {
    blocking(&core, move |c| c.import_files(paths, folder_id)).await
}

/// Selettore file nativo aperto dal core: la WebView non ha accesso al file system.
#[tauri::command(rename_all = "snake_case")]
pub async fn pick_and_import(
    app: AppHandle,
    window: WebviewWindow,
    core: State<'_, CoreHandle>,
    folder_id: Option<String>,
) -> ApiResult<Option<u64>> {
    blocking(&core, move |c| {
        use tauri_plugin_dialog::DialogExt;
        let picked = app
            .dialog()
            .file()
            .set_parent(&window)
            .set_title("Aggiungi suoni")
            .add_filter("File audio", &["mp3", "wav", "flac", "ogg", "oga", "m4a", "aac", "mp4"])
            .add_filter("Tutti i file", &["*"])
            .blocking_pick_files();
        let Some(files) = picked else { return Ok(None) };
        let paths: Vec<String> =
            files.into_iter().filter_map(|f| f.into_path().ok()).map(|p| p.display().to_string()).collect();
        if paths.is_empty() {
            return Ok(None);
        }
        c.import_files(paths, folder_id).map(Some)
    })
    .await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn update_sound(core: State<'_, CoreHandle>, sound_id: String, patch: SoundPatch) -> ApiResult<Sound> {
    blocking(&core, move |c| c.update_sound(sound_id, patch)).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn delete_sounds(core: State<'_, CoreHandle>, sound_ids: Vec<String>) -> ApiResult<()> {
    blocking(&core, move |c| c.delete_sounds(sound_ids)).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn restore_sounds(core: State<'_, CoreHandle>, sound_ids: Vec<String>) -> ApiResult<Vec<Sound>> {
    blocking(&core, move |c| c.restore_sounds(sound_ids)).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn upsert_folder(
    core: State<'_, CoreHandle>,
    id: Option<String>,
    name: String,
    order: Option<i32>,
) -> ApiResult<Folder> {
    blocking(&core, move |c| c.upsert_folder(id, name, order)).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn delete_folder(core: State<'_, CoreHandle>, folder_id: String) -> ApiResult<()> {
    blocking(&core, move |c| c.delete_folder(folder_id)).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn set_hotkey(
    core: State<'_, CoreHandle>,
    action: HotkeyAction,
    accelerator: Option<String>,
) -> ApiResult<Option<Hotkey>> {
    blocking(&core, move |c| c.set_hotkey(action, accelerator)).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn pause_hotkeys(core: State<'_, CoreHandle>, paused: bool) -> ApiResult<()> {
    blocking(&core, move |c| c.pause_hotkeys(paused)).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn set_output(core: State<'_, CoreHandle>, bus: BusKind, patch: OutputPatch) -> ApiResult<Vec<StatusCode>> {
    blocking(&core, move |c| c.set_output(bus, patch.device_id, patch.volume_db, patch.enabled)).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn update_settings(
    app: AppHandle,
    core: State<'_, CoreHandle>,
    patch: serde_json::Value,
) -> ApiResult<Settings> {
    let s = blocking(&core, move |c| c.update_settings(patch)).await?;
    sync_autostart(&app, s.startup.autostart);
    Ok(s)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_diagnostics(core: State<'_, CoreHandle>) -> ApiResult<Diagnostics> {
    blocking(&core, |c| c.get_diagnostics()).await
}

#[tauri::command(rename_all = "snake_case")]
pub fn open_sound_settings(tab: u8) -> ApiResult<()> {
    let tab = if matches!(tab, 0 | 1 | 3) { tab } else { 0 };
    mfx_core::open_sound_control_panel(tab).map_err(|e| ApiError::new(ErrorCode::IoError, e.to_string()))
}

#[tauri::command(rename_all = "snake_case")]
pub fn open_data_folder() -> ApiResult<()> {
    let dir = mfx_core::Paths::from_env().local;
    std::process::Command::new("explorer.exe")
        .arg(dir)
        .spawn()
        .map(|_| ())
        .map_err(|e| ApiError::new(ErrorCode::IoError, e.to_string()))
}

/// Solo indirizzi noti: la UI non può aprire URL arbitrari.
#[tauri::command(rename_all = "snake_case")]
pub fn open_link(which: String) -> ApiResult<()> {
    let url = match which.as_str() {
        "vb-cable" => "https://vb-audio.com/Cable/",
        "discord-voice" => "https://support.discord.com/hc/it/articles/360045138471",
        // Protocollo registrato dal client: apre Discord (sulle impostazioni voce, se le riconosce).
        "discord-app" => "discord://-/settings/voice",
        _ => return Err(ApiError::new(ErrorCode::InvalidArgument, "Collegamento sconosciuto.")),
    };
    std::process::Command::new("rundll32.exe")
        .args(["url.dll,FileProtocolHandler", url])
        .spawn()
        .map(|_| ())
        .map_err(|e| ApiError::new(ErrorCode::IoError, e.to_string()))
}

/// Allinea la chiave Run di Windows all'impostazione (anche dopo una reinstallazione in un altro percorso).
pub fn sync_autostart(app: &AppHandle, enabled: bool) {
    use tauri_plugin_autostart::ManagerExt;
    let al = app.autolaunch();
    let res = if enabled {
        al.enable()
    } else if al.is_enabled().unwrap_or(false) {
        al.disable()
    } else {
        Ok(())
    };
    if let Err(e) = res {
        tracing::warn!("autostart: {e}");
    }
}
