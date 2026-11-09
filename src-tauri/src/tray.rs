//! Icona nella tray (F12): apri, ferma tutto, pausa scorciatoie, verifica aggiornamenti, esci.

use mfx_core::CoreHandle;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Manager, Wry};

use crate::window;

pub struct TrayState {
    pause: CheckMenuItem<Wry>,
}

pub fn create(app: &App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Apri MicFX", true, None::<&str>)?;
    let stop = MenuItem::with_id(app, "stop_all", "Ferma tutti i suoni", true, None::<&str>)?;
    let pause = CheckMenuItem::with_id(app, "pause", "Pausa scorciatoie", true, false, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    // Il controllo lo fa la UI (il core non fa rete): la voce apre la finestra e glielo chiede.
    let update = MenuItem::with_id(app, "check_update", "Verifica aggiornamenti", true, None::<&str>)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Esci da MicFX", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &stop, &pause, &sep, &update, &sep2, &quit])?;

    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("MicFX")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, ev| match ev.id().as_ref() {
            "open" => window::show(app),
            "check_update" => window::request(app, "check-update"),
            "stop_all" => {
                let _ = app.state::<CoreHandle>().stop_all();
            }
            "pause" => {
                let core = app.state::<CoreHandle>();
                let paused = core.hotkeys_paused().unwrap_or(false);
                let _ = core.pause_hotkeys(!paused);
            }
            "quit" => {
                tracing::info!("uscita dal menu della tray");
                app.state::<CoreHandle>().shutdown();
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, ev| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                window::show(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    app.manage(TrayState { pause });
    Ok(())
}

pub fn set_paused(app: &AppHandle, paused: bool) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(t) = handle.try_state::<TrayState>() {
            let _ = t.pause.set_checked(paused);
        }
        if let Some(tray) = handle.tray_by_id("main") {
            let _ = tray.set_tooltip(Some(if paused { "MicFX · scorciatoie in pausa" } else { "MicFX" }));
        }
    });
}
