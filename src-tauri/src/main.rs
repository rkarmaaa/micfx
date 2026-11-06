//! Guscio Tauri di MicFX (docs/SPEC.md §10): tray, finestra effimera, IPC.
//! Tutta la logica sta in `mfx-core`; qui solo il collegamento con Windows e la WebView.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ipc;
mod tray;
mod window;

use std::sync::Arc;

use mfx_core::{CoreEvent, CoreHandle, EventSink, Paths};
use tauri::{AppHandle, Emitter, Manager, RunEvent};

/// Inoltra gli eventi del core: alla finestra se esiste, al guscio quelli che lo riguardano.
struct Sink(AppHandle);

impl EventSink for Sink {
    fn emit(&self, ev: CoreEvent) {
        match &ev {
            CoreEvent::UiExpired => {
                let app = self.0.clone();
                let _ = self.0.run_on_main_thread(move || window::destroy(&app));
                return;
            }
            CoreEvent::OpenWindow => {
                let app = self.0.clone();
                let _ = self.0.run_on_main_thread(move || window::show(&app));
                return;
            }
            CoreEvent::HotkeysChanged { paused, .. } => tray::set_paused(&self.0, *paused),
            _ => {}
        }
        if let Some(name) = ev.ui_name()
            && self.0.get_webview_window(window::MAIN).is_some()
        {
            let _ = self.0.emit_to(window::MAIN, name, ev.ui_payload());
        }
    }
}

fn init_logging(paths: &Paths) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    use tracing_appender::rolling::{Builder, Rotation};
    use tracing_subscriber::EnvFilter;
    let default = if cfg!(debug_assertions) { "info,mfx_core=debug" } else { "info" };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default));
    let appender = Builder::new()
        .rotation(Rotation::DAILY)
        .filename_prefix("micfx")
        .filename_suffix("log")
        .max_log_files(5)
        .build(paths.logs_dir())
        .ok()?;
    let (writer, guard) = tracing_appender::non_blocking(appender);
    tracing_subscriber::fmt().with_env_filter(filter).with_writer(writer).with_ansi(false).init();
    Some(guard)
}

fn main() {
    let paths = Paths::from_env();
    let _ = paths.ensure();
    let _log_guard = init_logging(&paths);

    // Il PTT non deve mai restare premuto, nemmeno se qualcosa va storto (SPEC §8.2, §10.3).
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        mfx_core::emergency_release_ptt();
        tracing::error!("panic: {info}");
        default_hook(info);
    }));

    let start_in_tray = std::env::args().any(|a| a == "--tray");
    tracing::info!("MicFX {} avvio{}", env!("CARGO_PKG_VERSION"), if start_in_tray { " in tray" } else { "" });

    let app = tauri::Builder::default()
        // Istanza singola: un secondo avvio apre la finestra di quella già attiva (F14).
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| window::show(app)))
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--tray"])))
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            let sink = Arc::new(Sink(app.handle().clone()));
            let core = CoreHandle::start(paths.clone(), sink)?;
            app.manage(core.clone());
            tray::create(app)?;
            if let Ok(s) = core.get_settings() {
                ipc::sync_autostart(app.handle(), s.startup.autostart);
            }
            if !start_in_tray {
                window::show(app.handle());
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ipc::get_snapshot,
            ipc::recheck_setup,
            ipc::play_sound,
            ipc::preview_sound,
            ipc::stop_sound,
            ipc::stop_all,
            ipc::import_files,
            ipc::pick_and_import,
            ipc::update_sound,
            ipc::delete_sounds,
            ipc::restore_sounds,
            ipc::upsert_folder,
            ipc::delete_folder,
            ipc::set_hotkey,
            ipc::pause_hotkeys,
            ipc::set_output,
            ipc::update_settings,
            ipc::get_diagnostics,
            ipc::open_sound_settings,
            ipc::open_data_folder,
            ipc::open_link,
        ])
        .build(tauri::generate_context!())
        .expect("avvio di MicFX fallito");

    app.run(|app, event| match event {
        // Chiudere l'ultima finestra non chiude l'app: resta nella tray (F12). Solo "Esci" passa un codice.
        RunEvent::ExitRequested { api, code, .. } if code.is_none() => api.prevent_exit(),
        RunEvent::Exit => {
            if let Some(core) = app.try_state::<CoreHandle>() {
                core.shutdown();
            }
        }
        _ => {}
    });
}
