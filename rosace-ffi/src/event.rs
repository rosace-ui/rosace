//! C-ABI input event (D106 Phase 24 Step 1).
//!
//! One flat `#[repr(C)]` struct covers every `rosace_platform::InputEvent`
//! variant via a `kind` tag — simpler and more stable across the FFI
//! boundary than a C union. Native touch callbacks (iOS `touchesBegan` /
//! `touchesMoved` / `touchesEnded`, Android `MotionEvent`) convert to
//! `MouseDown`/`MouseMove`/`MouseUp` on the host side, exactly like the
//! existing winit `Touch` handling in `rosace-platform/src/app.rs` — no new
//! `InputEvent` variants are needed for a native host.

use rosace_platform::{InputEvent, Key, MouseButton};

/// `kind` discriminants — keep in sync with `include/tzr_engine.h`.
pub const TZR_EVENT_MOUSE_MOVE: u32 = 0;
pub const TZR_EVENT_MOUSE_DOWN: u32 = 1;
pub const TZR_EVENT_MOUSE_UP: u32 = 2;
pub const TZR_EVENT_KEY_DOWN: u32 = 3;
pub const TZR_EVENT_KEY_UP: u32 = 4;
pub const TZR_EVENT_TEXT: u32 = 5;
pub const TZR_EVENT_WINDOW_RESIZED: u32 = 6;
pub const TZR_EVENT_SCROLL: u32 = 7;

/// `button` values (`MouseDown`/`MouseUp`).
// `TZR_BUTTON_LEFT` is the `match`'s fallback arm below (0 is also the
// natural "no button" default), so it's never named directly — kept `pub`
// to mirror `include/tzr_engine.h`'s constant for C/Swift/Kotlin callers.
#[allow(dead_code)]
pub const TZR_BUTTON_LEFT: u32 = 0;
pub const TZR_BUTTON_RIGHT: u32 = 1;
pub const TZR_BUTTON_MIDDLE: u32 = 2;

/// `key` values (`KeyDown`/`KeyUp`); `TZR_KEY_CHAR` reads `character`.
pub const TZR_KEY_ENTER: u32 = 0;
pub const TZR_KEY_ESCAPE: u32 = 1;
pub const TZR_KEY_SPACE: u32 = 2;
pub const TZR_KEY_BACKSPACE: u32 = 3;
pub const TZR_KEY_TAB: u32 = 4;
pub const TZR_KEY_ARROW_UP: u32 = 5;
pub const TZR_KEY_ARROW_DOWN: u32 = 6;
pub const TZR_KEY_ARROW_LEFT: u32 = 7;
pub const TZR_KEY_ARROW_RIGHT: u32 = 8;
pub const TZR_KEY_SHIFT: u32 = 9;
pub const TZR_KEY_CONTROL: u32 = 10;
pub const TZR_KEY_ALT: u32 = 11;
pub const TZR_KEY_META: u32 = 12;
/// Same reasoning as `TZR_BUTTON_LEFT` above — it's the `match` fallback.
#[allow(dead_code)]
pub const TZR_KEY_CHAR: u32 = 13;
/// Added D116 Phase 28 Step 6 (Known Issue #15) — `rosace_platform::Key`
/// gained `Delete`/`Home`/`End` in Phase 28 Step 1 for real `TextInput`
/// keyboard editing, but the FFI mapping was never extended to match,
/// leaving them unreachable from a mobile host. New constants appended
/// (not inserted alphabetically) so existing hosts' already-compiled
/// constant values never shift.
pub const TZR_KEY_DELETE: u32 = 14;
pub const TZR_KEY_HOME: u32 = 15;
pub const TZR_KEY_END: u32 = 16;

/// Keyboard-type hint values (D116 Step 6) — what
/// [`focused_keyboard_type`] returns, for a native host to poll once per
/// frame and drive the real `UIKeyboardType`/Android `InputType` (the
/// native mapping itself is unwritten — same deferred-to-a-real-device
/// status `TZR_KEY_DELETE`/`_HOME`/`_END` had before this same step).
pub const TZR_KEYBOARD_DEFAULT: u32 = 0;
pub const TZR_KEYBOARD_EMAIL: u32 = 1;
pub const TZR_KEYBOARD_NUMERIC: u32 = 2;
pub const TZR_KEYBOARD_URL: u32 = 3;
pub const TZR_KEYBOARD_PHONE: u32 = 4;

/// The currently-focused field's keyboard-type hint, encoded as a
/// `TZR_KEYBOARD_*` constant — a native host polls this once per frame
/// tick (same polling shape `take_camera_request` uses) to know which
/// software keyboard layout to show.
pub fn focused_keyboard_type() -> u32 {
    match rosace_core::keyboard_type() {
        rosace_core::KeyboardType::Default => TZR_KEYBOARD_DEFAULT,
        rosace_core::KeyboardType::Email => TZR_KEYBOARD_EMAIL,
        rosace_core::KeyboardType::Numeric => TZR_KEYBOARD_NUMERIC,
        rosace_core::KeyboardType::Url => TZR_KEYBOARD_URL,
        rosace_core::KeyboardType::Phone => TZR_KEYBOARD_PHONE,
    }
}

/// One input event crossing the FFI boundary. Unused fields for a given
/// `kind` are ignored. `character` holds a Unicode scalar value (as `u32`)
/// for `TZR_EVENT_TEXT` and `TZR_KEY_CHAR`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TzrInputEventFfi {
    pub kind: u32,
    pub x: f32,
    pub y: f32,
    pub button: u32,
    pub key: u32,
    pub character: u32,
    pub width: u32,
    pub height: u32,
    pub delta_x: f32,
    pub delta_y: f32,
}

fn button_from_ffi(b: u32) -> MouseButton {
    match b {
        TZR_BUTTON_RIGHT => MouseButton::Right,
        TZR_BUTTON_MIDDLE => MouseButton::Middle,
        _ => MouseButton::Left, // TZR_BUTTON_LEFT and any unrecognized value
    }
}

fn key_from_ffi(k: u32, character: u32) -> Key {
    match k {
        TZR_KEY_ENTER => Key::Enter,
        TZR_KEY_ESCAPE => Key::Escape,
        TZR_KEY_SPACE => Key::Space,
        TZR_KEY_BACKSPACE => Key::Backspace,
        TZR_KEY_TAB => Key::Tab,
        TZR_KEY_ARROW_UP => Key::ArrowUp,
        TZR_KEY_ARROW_DOWN => Key::ArrowDown,
        TZR_KEY_ARROW_LEFT => Key::ArrowLeft,
        TZR_KEY_ARROW_RIGHT => Key::ArrowRight,
        TZR_KEY_SHIFT => Key::Shift,
        TZR_KEY_CONTROL => Key::Control,
        TZR_KEY_ALT => Key::Alt,
        TZR_KEY_META => Key::Meta,
        TZR_KEY_DELETE => Key::Delete,
        TZR_KEY_HOME => Key::Home,
        TZR_KEY_END => Key::End,
        // TZR_KEY_CHAR and any unrecognized value
        _ => Key::Char(char::from_u32(character).unwrap_or('\u{FFFD}')),
    }
}

impl From<TzrInputEventFfi> for InputEvent {
    fn from(e: TzrInputEventFfi) -> Self {
        match e.kind {
            TZR_EVENT_MOUSE_MOVE => InputEvent::MouseMove { x: e.x, y: e.y },
            TZR_EVENT_MOUSE_DOWN => InputEvent::MouseDown {
                x: e.x, y: e.y, button: button_from_ffi(e.button),
            },
            TZR_EVENT_MOUSE_UP => InputEvent::MouseUp {
                x: e.x, y: e.y, button: button_from_ffi(e.button),
            },
            TZR_EVENT_KEY_DOWN => InputEvent::KeyDown { key: key_from_ffi(e.key, e.character) },
            TZR_EVENT_KEY_UP => InputEvent::KeyUp { key: key_from_ffi(e.key, e.character) },
            TZR_EVENT_TEXT => InputEvent::Text {
                character: char::from_u32(e.character).unwrap_or('\u{FFFD}'),
            },
            TZR_EVENT_WINDOW_RESIZED => InputEvent::WindowResized { width: e.width, height: e.height },
            TZR_EVENT_SCROLL => InputEvent::Scroll {
                x: e.x, y: e.y, delta_x: e.delta_x, delta_y: e.delta_y,
            },
            _ => InputEvent::MouseMove { x: e.x, y: e.y },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mouse_down_round_trips_button() {
        let ffi = TzrInputEventFfi {
            kind: TZR_EVENT_MOUSE_DOWN, x: 1.0, y: 2.0, button: TZR_BUTTON_RIGHT,
            key: 0, character: 0, width: 0, height: 0, delta_x: 0.0, delta_y: 0.0,
        };
        match InputEvent::from(ffi) {
            InputEvent::MouseDown { x, y, button } => {
                assert_eq!((x, y), (1.0, 2.0));
                assert_eq!(button, MouseButton::Right);
            }
            other => panic!("expected MouseDown, got {other:?}"),
        }
    }

    #[test]
    fn key_down_char_reads_character_field() {
        let ffi = TzrInputEventFfi {
            kind: TZR_EVENT_KEY_DOWN, x: 0.0, y: 0.0, button: 0,
            key: TZR_KEY_CHAR, character: 'a' as u32, width: 0, height: 0,
            delta_x: 0.0, delta_y: 0.0,
        };
        match InputEvent::from(ffi) {
            InputEvent::KeyDown { key: Key::Char(c) } => assert_eq!(c, 'a'),
            other => panic!("expected KeyDown(Char('a')), got {other:?}"),
        }
    }

    #[test]
    fn window_resized_reads_width_height() {
        let ffi = TzrInputEventFfi {
            kind: TZR_EVENT_WINDOW_RESIZED, x: 0.0, y: 0.0, button: 0,
            key: 0, character: 0, width: 390, height: 844, delta_x: 0.0, delta_y: 0.0,
        };
        match InputEvent::from(ffi) {
            InputEvent::WindowResized { width, height } => assert_eq!((width, height), (390, 844)),
            other => panic!("expected WindowResized, got {other:?}"),
        }
    }

    #[test]
    fn key_down_delete_home_end_round_trip() {
        // Known Issue #15 (D116 Phase 28 Step 6) — these three were
        // reachable on desktop since Phase 28 Step 1 but had no FFI
        // constant at all until now.
        for (ffi_key, expected) in [
            (TZR_KEY_DELETE, Key::Delete),
            (TZR_KEY_HOME, Key::Home),
            (TZR_KEY_END, Key::End),
        ] {
            let ffi = TzrInputEventFfi {
                kind: TZR_EVENT_KEY_DOWN, x: 0.0, y: 0.0, button: 0,
                key: ffi_key, character: 0, width: 0, height: 0, delta_x: 0.0, delta_y: 0.0,
            };
            match InputEvent::from(ffi) {
                InputEvent::KeyDown { key } => assert_eq!(key, expected, "TZR key {ffi_key} must map to {expected:?}"),
                other => panic!("expected KeyDown, got {other:?}"),
            }
        }
    }

    /// `focused_keyboard_type` reads a process-global (`GlobalAtom`), and
    /// tests in one binary run in parallel threads — the two tests below
    /// must not interleave or the "still at factory default" assertion
    /// races the mutating test. The mutating test resets the global
    /// before releasing the lock, so either order passes.
    static KEYBOARD_TYPE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn focused_keyboard_type_defaults_to_default_when_nothing_set_it() {
        let _guard = KEYBOARD_TYPE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // Nothing else in this process has the lock, so the GlobalAtom is
        // at its factory default (or reset back to it).
        assert_eq!(focused_keyboard_type(), TZR_KEYBOARD_DEFAULT);
    }

    #[test]
    fn focused_keyboard_type_reflects_rosace_core_state() {
        let _guard = KEYBOARD_TYPE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        rosace_core::set_keyboard_type(rosace_core::KeyboardType::Email);
        assert_eq!(focused_keyboard_type(), TZR_KEYBOARD_EMAIL);
        rosace_core::set_keyboard_type(rosace_core::KeyboardType::Numeric);
        assert_eq!(focused_keyboard_type(), TZR_KEYBOARD_NUMERIC);
        // Reset BEFORE the lock releases so the defaults test above can
        // never observe a leaked value.
        rosace_core::set_keyboard_type(rosace_core::KeyboardType::Default);
    }

    #[test]
    fn scroll_reads_deltas() {
        let ffi = TzrInputEventFfi {
            kind: TZR_EVENT_SCROLL, x: 5.0, y: 6.0, button: 0,
            key: 0, character: 0, width: 0, height: 0, delta_x: 1.5, delta_y: -2.5,
        };
        match InputEvent::from(ffi) {
            InputEvent::Scroll { x, y, delta_x, delta_y } => {
                assert_eq!((x, y, delta_x, delta_y), (5.0, 6.0, 1.5, -2.5));
            }
            other => panic!("expected Scroll, got {other:?}"),
        }
    }
}
