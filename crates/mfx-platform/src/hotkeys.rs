//! Hotkey globali (SPEC §8.1): un thread che dorme in `GetMessageW` e si sveglia solo per
//! `WM_HOTKEY` o per una richiesta di registrazione.
//!
//! `RegisterHotKey` va chiamato dal thread che riceverà i messaggi: le richieste arrivano su un
//! canale e il thread viene svegliato con `PostThreadMessageW(WM_APP)`.

use std::thread::JoinHandle;

use crossbeam_channel::{Receiver, Sender, bounded, unbounded};
use windows::Win32::Foundation::{ERROR_HOTKEY_ALREADY_REGISTERED, LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{HOT_KEY_MODIFIERS, MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey};
use windows::Win32::UI::WindowsAndMessaging::{
    GetMessageW, MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW, WM_APP, WM_HOTKEY, WM_QUIT, WM_USER,
};

use crate::accel::Accelerator;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HotkeyError {
    #[error("combinazione già usata da un'altra applicazione")]
    Conflict,
    #[error("registrazione rifiutata da Windows: {0}")]
    Failed(String),
    #[error("servizio hotkey non attivo")]
    Stopped,
}

enum Request {
    Register { id: i32, accel: Accelerator, reply: Sender<Result<(), HotkeyError>> },
    Unregister { id: i32, reply: Sender<()> },
}

pub struct HotkeyService {
    thread_id: u32,
    tx: Sender<Request>,
    join: Option<JoinHandle<()>>,
}

impl HotkeyService {
    /// Avvia il thread. `on_hotkey` gira sul thread hotkey: deve solo inoltrare e tornare subito.
    pub fn spawn(on_hotkey: impl Fn(i32) + Send + 'static) -> std::io::Result<Self> {
        let (tx, rx) = unbounded::<Request>();
        let (ready_tx, ready_rx) = bounded::<u32>(1);
        let join =
            std::thread::Builder::new().name("mfx-hotkeys".into()).spawn(move || run(rx, ready_tx, on_hotkey))?;
        let thread_id = ready_rx.recv().map_err(|_| std::io::Error::other("thread hotkey non partito"))?;
        Ok(Self { thread_id, tx, join: Some(join) })
    }

    fn wake(&self) {
        // SAFETY: thread_id del thread hotkey, che ha già una coda messaggi.
        let _ = unsafe { PostThreadMessageW(self.thread_id, WM_APP, WPARAM(0), LPARAM(0)) };
    }

    /// Registra (o ri-registra) `id`. Sincrono: risponde il thread hotkey.
    pub fn register(&self, id: i32, accel: Accelerator) -> Result<(), HotkeyError> {
        let (reply, rx) = bounded(1);
        self.tx.send(Request::Register { id, accel, reply }).map_err(|_| HotkeyError::Stopped)?;
        self.wake();
        rx.recv().unwrap_or(Err(HotkeyError::Stopped))
    }

    pub fn unregister(&self, id: i32) {
        let (reply, rx) = bounded(1);
        if self.tx.send(Request::Unregister { id, reply }).is_ok() {
            self.wake();
            let _ = rx.recv();
        }
    }
}

impl Drop for HotkeyService {
    fn drop(&mut self) {
        // SAFETY: vedi wake().
        let _ = unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

fn run(rx: Receiver<Request>, ready: Sender<u32>, on_hotkey: impl Fn(i32)) {
    let mut msg = MSG::default();
    // SAFETY: crea la coda messaggi del thread prima di annunciarne l'ID.
    unsafe {
        let _ = PeekMessageW(&mut msg, None, WM_USER, WM_USER, PM_NOREMOVE);
        let _ = ready.send(GetCurrentThreadId());
    }
    let mut registered: Vec<i32> = Vec::new();
    loop {
        // SAFETY: msg valido; hwnd None = messaggi del thread.
        let r = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        if r.0 == 0 || r.0 == -1 {
            break;
        }
        match msg.message {
            WM_HOTKEY => on_hotkey(msg.wParam.0 as i32),
            WM_APP => {
                while let Ok(req) = rx.try_recv() {
                    match req {
                        Request::Register { id, accel, reply } => {
                            if registered.contains(&id) {
                                // SAFETY: id registrato da questo thread.
                                let _ = unsafe { UnregisterHotKey(None, id) };
                                registered.retain(|x| *x != id);
                            }
                            let mods = HOT_KEY_MODIFIERS(accel.mods.to_win32()) | MOD_NOREPEAT;
                            // SAFETY: registrazione sul thread corrente (hwnd None).
                            let res = unsafe { RegisterHotKey(None, id, mods, accel.virtual_key()) };
                            let out = match res {
                                Ok(()) => {
                                    registered.push(id);
                                    Ok(())
                                }
                                Err(e) if e.code() == ERROR_HOTKEY_ALREADY_REGISTERED.to_hresult() => {
                                    Err(HotkeyError::Conflict)
                                }
                                Err(e) => Err(HotkeyError::Failed(e.message())),
                            };
                            let _ = reply.send(out);
                        }
                        Request::Unregister { id, reply } => {
                            if registered.contains(&id) {
                                // SAFETY: id registrato da questo thread.
                                let _ = unsafe { UnregisterHotKey(None, id) };
                                registered.retain(|x| *x != id);
                            }
                            let _ = reply.send(());
                        }
                    }
                }
            }
            _ => {}
        }
    }
    for id in registered {
        // SAFETY: id registrati da questo thread.
        let _ = unsafe { UnregisterHotKey(None, id) };
    }
}
