//! Finestra effimera (SPEC §10.2): creata a richiesta, nascosta e sospesa alla chiusura,
//! distrutta allo scadere del keepalive. Senza finestra non esiste nessun processo WebView.

use mfx_core::{CoreHandle, WindowRect};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};

pub const MAIN: &str = "main";

pub fn show(app: &AppHandle) {
    let core = app.state::<CoreHandle>();
    if let Some(w) = app.get_webview_window(MAIN) {
        resume(&w);
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        core.ui_opened();
        return;
    }
    if let Err(e) = create(app) {
        tracing::error!("finestra non creata: {e}");
    }
}

fn create(app: &AppHandle) -> tauri::Result<()> {
    let core = app.state::<CoreHandle>().inner().clone();
    let saved = core.get_settings().ok().and_then(|s| s.ui.window);
    let mut b = WebviewWindowBuilder::new(app, MAIN, WebviewUrl::App("index.html".into()))
        .title("MicFX")
        .inner_size(980.0, 680.0)
        .min_inner_size(560.0, 420.0)
        .focused(true);
    match saved.filter(|r| on_screen(app, r)) {
        Some(r) => b = b.position(r.x as f64, r.y as f64).inner_size(r.w as f64, r.h as f64),
        None => b = b.center(),
    }
    let w = b.build()?;
    if saved.is_some_and(|r| r.maximized) {
        let _ = w.maximize();
    }
    let _ = w.set_focus();
    core.ui_opened();
    let handle = app.clone();
    w.on_window_event(move |ev| match ev {
        WindowEvent::CloseRequested { api, .. } => {
            // La X nasconde: la finestra resta pronta per `keepalive_s`, poi viene distrutta.
            api.prevent_close();
            hide(&handle);
        }
        WindowEvent::Moved(_) | WindowEvent::Resized(_) => save_rect(&handle),
        WindowEvent::Destroyed => handle.state::<CoreHandle>().ui_closed(),
        _ => {}
    });
    Ok(())
}

pub fn hide(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(MAIN) {
        let _ = w.hide();
        suspend(&w);
        app.state::<CoreHandle>().ui_hidden();
    }
}

pub fn destroy(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(MAIN) {
        // Se nel frattempo è stata riaperta, resta.
        if w.is_visible().unwrap_or(false) {
            return;
        }
        let _ = w.destroy();
        tracing::info!("finestra distrutta dopo il keepalive");
    }
}

fn save_rect(app: &AppHandle) {
    let Some(w) = app.get_webview_window(MAIN) else { return };
    if w.is_minimized().unwrap_or(false) || !w.is_visible().unwrap_or(false) {
        return;
    }
    let core = app.state::<CoreHandle>();
    let maximized = w.is_maximized().unwrap_or(false);
    let prev = core.get_settings().ok().and_then(|s| s.ui.window);
    let rect = if maximized {
        // Da massimizzata si ricorda solo il flag: la posizione resta quella "normale".
        match prev {
            Some(p) => WindowRect { maximized: true, ..p },
            None => return,
        }
    } else {
        let scale = w.scale_factor().unwrap_or(1.0);
        let (Ok(pos), Ok(size)) = (w.outer_position(), w.inner_size()) else { return };
        WindowRect {
            x: (pos.x as f64 / scale).round() as i32,
            y: (pos.y as f64 / scale).round() as i32,
            w: (size.width as f64 / scale).round() as u32,
            h: (size.height as f64 / scale).round() as u32,
            maximized: false,
        }
    };
    core.save_window_rect(rect);
}

/// La posizione salvata è ancora su uno schermo collegato? (monitor scollegato, risoluzione cambiata)
fn on_screen(app: &AppHandle, r: &WindowRect) -> bool {
    let Ok(monitors) = app.available_monitors() else { return false };
    monitors.iter().any(|m| {
        let s = m.scale_factor();
        let (mx, my) = (m.position().x as f64, m.position().y as f64);
        let (mw, mh) = (m.size().width as f64, m.size().height as f64);
        let (x, y, w, h) = (r.x as f64 * s, r.y as f64 * s, r.w as f64 * s, r.h as f64 * s);
        let ow = (x + w).min(mx + mw) - x.max(mx);
        let oh = (y + h).min(my + mh) - y.max(my);
        ow >= 120.0 && oh >= 60.0
    })
}

/// WebView nascosta: invisibile e sospesa, memoria al livello basso (SPEC §10.2).
fn suspend(w: &WebviewWindow) {
    let _ = w.with_webview(|wv| {
        use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_3;
        use webview2_com::TrySuspendCompletedHandler;
        use windows::core::Interface;
        let controller = wv.controller();
        // SAFETY: chiamate COM sul thread principale, dove vive il controller.
        unsafe {
            let _ = controller.SetIsVisible(false);
            if let Ok(core) = controller.CoreWebView2()
                && let Ok(core3) = core.cast::<ICoreWebView2_3>()
            {
                let handler = TrySuspendCompletedHandler::create(Box::new(|_, _| Ok(())));
                let _ = core3.TrySuspend(&handler);
            }
        }
    });
}

fn resume(w: &WebviewWindow) {
    let _ = w.with_webview(|wv| {
        use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_3;
        use windows::core::Interface;
        let controller = wv.controller();
        // SAFETY: vedi suspend().
        unsafe {
            if let Ok(core) = controller.CoreWebView2()
                && let Ok(core3) = core.cast::<ICoreWebView2_3>()
            {
                let _ = core3.Resume();
            }
            let _ = controller.SetIsVisible(true);
        }
    });
}
