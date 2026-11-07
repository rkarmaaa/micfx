//! Relay push-to-talk (SPEC §8.2): `SendInput` con `KEYEVENTF_SCANCODE`.

use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP,
    KEYEVENTF_SCANCODE, SendInput, VIRTUAL_KEY,
};

/// Preme (`down = true`) o rilascia un tasto per scancode. `false` se Windows rifiuta l'input
/// (es. finestra in primo piano con privilegi più alti, UIPI).
pub fn send_scancode(scancode: u16, down: bool) -> bool {
    let extended = scancode & 0xFF00 == 0xE000;
    let mut flags = KEYEVENTF_SCANCODE;
    if extended {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    if !down {
        flags |= KEYEVENTF_KEYUP;
    }
    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: scancode & 0x00FF,
                dwFlags: KEYBD_EVENT_FLAGS(flags.0),
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    // SAFETY: array di un INPUT valido, dimensione corretta.
    let sent = unsafe { SendInput(&[input], size_of::<INPUT>() as i32) };
    sent == 1
}
