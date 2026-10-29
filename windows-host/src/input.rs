//! Remote input injection on the Windows host, via the standard
//! `SendInput` API. This is the same mechanism every legitimate
//! remote-desktop tool uses (it is literally what Windows provides for
//! synthesizing input) — the security boundary for this is entirely
//! upstream of here, in transport.rs's trust-pinned authentication:
//! this function is only ever reached for an already-authenticated,
//! already-trusted connection.

use refresh_core::protocol::{InputEvent, MouseButton, TouchPhase};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP,
    MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN,
    MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
    MOUSEEVENTF_WHEEL, MOUSEINPUT,
};
use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

/// Last known remote-controller touch/mouse state, used to translate
/// absolute touch coordinates (0..1 normalized, from the Android client)
/// into the specific MOUSEEVENTF_ABSOLUTE coordinate space Windows wants
/// (0..65535 across the primary screen).
fn to_absolute(x_norm: f32, y_norm: f32) -> (i32, i32) {
    let max = 65535.0;
    (
        (x_norm.clamp(0.0, 1.0) * max) as i32,
        (y_norm.clamp(0.0, 1.0) * max) as i32,
    )
}

pub fn apply(event: &InputEvent) {
    unsafe {
        match event {
            InputEvent::MouseMove { x, y } => {
                let sw = GetSystemMetrics(SM_CXSCREEN).max(1) as f32;
                let sh = GetSystemMetrics(SM_CYSCREEN).max(1) as f32;
                let (ax, ay) = to_absolute(*x as f32 / sw, *y as f32 / sh);
                send_mouse(ax, ay, MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE, 0);
            }
            InputEvent::Touch { x, y, phase } => {
                let (ax, ay) = to_absolute(*x, *y);
                let flags = MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE;
                send_mouse(ax, ay, flags, 0);
                match phase {
                    TouchPhase::Start => {
                        send_mouse(ax, ay, MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_LEFTDOWN, 0)
                    }
                    TouchPhase::End => {
                        send_mouse(ax, ay, MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_LEFTUP, 0)
                    }
                    TouchPhase::Move => {}
                }
            }
            InputEvent::MouseButton { button, pressed } => {
                let flag = match (button, pressed) {
                    (MouseButton::Left, true) => MOUSEEVENTF_LEFTDOWN,
                    (MouseButton::Left, false) => MOUSEEVENTF_LEFTUP,
                    (MouseButton::Right, true) => MOUSEEVENTF_RIGHTDOWN,
                    (MouseButton::Right, false) => MOUSEEVENTF_RIGHTUP,
                    (MouseButton::Middle, true) => MOUSEEVENTF_MIDDLEDOWN,
                    (MouseButton::Middle, false) => MOUSEEVENTF_MIDDLEUP,
                };
                send_mouse(0, 0, flag, 0);
            }
            InputEvent::MouseScroll { delta_y, .. } => {
                send_mouse(0, 0, MOUSEEVENTF_WHEEL, *delta_y);
            }
            InputEvent::Key { code, pressed } => {
                send_key(*code as u16, *pressed);
            }
        }
    }
}

unsafe fn send_mouse(
    dx: i32,
    dy: i32,
    flags: windows::Win32::UI::Input::KeyboardAndMouse::MOUSE_EVENT_FLAGS,
    mouse_data: i32,
) {
    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: mouse_data as u32,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
}

unsafe fn send_key(vk: u16, pressed: bool) {
    let flags = if pressed {
        windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS(0)
    } else {
        KEYEVENTF_KEYUP
    };
    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY(vk),
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
}
