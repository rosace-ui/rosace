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

/// `kind` discriminants — keep in sync with `include/rsc_engine.h`.
pub const RSC_EVENT_MOUSE_MOVE: u32 = 0;
pub const RSC_EVENT_MOUSE_DOWN: u32 = 1;
pub const RSC_EVENT_MOUSE_UP: u32 = 2;
pub const RSC_EVENT_KEY_DOWN: u32 = 3;
pub const RSC_EVENT_KEY_UP: u32 = 4;
pub const RSC_EVENT_TEXT: u32 = 5;
pub const RSC_EVENT_WINDOW_RESIZED: u32 = 6;
pub const RSC_EVENT_SCROLL: u32 = 7;
/// App-lifecycle transitions (D042/D110, Phase 29 Step 1). Four distinct
/// kinds rather than one kind + a state field — matches this struct's
/// flat-tag convention (no field is semantically overloaded per kind).
/// iOS sends them from `UIApplication` notification observers, Android
/// from `onResume`/`onPause`/`onStop`; all other fields are ignored.
pub const RSC_EVENT_LIFECYCLE_ACTIVE: u32 = 8;
pub const RSC_EVENT_LIFECYCLE_INACTIVE: u32 = 9;
pub const RSC_EVENT_LIFECYCLE_BACKGROUND: u32 = 10;
pub const RSC_EVENT_LIFECYCLE_SUSPENDED: u32 = 11;

/// `button` values (`MouseDown`/`MouseUp`).
// `RSC_BUTTON_LEFT` is the `match`'s fallback arm below (0 is also the
// natural "no button" default), so it's never named directly — kept `pub`
// to mirror `include/rsc_engine.h`'s constant for C/Swift/Kotlin callers.
#[allow(dead_code)]
pub const RSC_BUTTON_LEFT: u32 = 0;
pub const RSC_BUTTON_RIGHT: u32 = 1;
pub const RSC_BUTTON_MIDDLE: u32 = 2;

/// `key` values (`KeyDown`/`KeyUp`); `RSC_KEY_CHAR` reads `character`.
pub const RSC_KEY_ENTER: u32 = 0;
pub const RSC_KEY_ESCAPE: u32 = 1;
pub const RSC_KEY_SPACE: u32 = 2;
pub const RSC_KEY_BACKSPACE: u32 = 3;
pub const RSC_KEY_TAB: u32 = 4;
pub const RSC_KEY_ARROW_UP: u32 = 5;
pub const RSC_KEY_ARROW_DOWN: u32 = 6;
pub const RSC_KEY_ARROW_LEFT: u32 = 7;
pub const RSC_KEY_ARROW_RIGHT: u32 = 8;
pub const RSC_KEY_SHIFT: u32 = 9;
pub const RSC_KEY_CONTROL: u32 = 10;
pub const RSC_KEY_ALT: u32 = 11;
pub const RSC_KEY_META: u32 = 12;
/// Same reasoning as `RSC_BUTTON_LEFT` above — it's the `match` fallback.
#[allow(dead_code)]
pub const RSC_KEY_CHAR: u32 = 13;
/// Added D116 Phase 28 Step 6 (Known Issue #15) — `rosace_platform::Key`
/// gained `Delete`/`Home`/`End` in Phase 28 Step 1 for real `TextInput`
/// keyboard editing, but the FFI mapping was never extended to match,
/// leaving them unreachable from a mobile host. New constants appended
/// (not inserted alphabetically) so existing hosts' already-compiled
/// constant values never shift.
pub const RSC_KEY_DELETE: u32 = 14;
pub const RSC_KEY_HOME: u32 = 15;
pub const RSC_KEY_END: u32 = 16;

/// Keyboard-type hint values (D116 Step 6) — what
/// [`focused_keyboard_type`] returns, for a native host to poll once per
/// frame and drive the real `UIKeyboardType`/Android `InputType` (the
/// native mapping itself is unwritten — same deferred-to-a-real-device
/// status `RSC_KEY_DELETE`/`_HOME`/`_END` had before this same step).
pub const RSC_KEYBOARD_DEFAULT: u32 = 0;
pub const RSC_KEYBOARD_EMAIL: u32 = 1;
pub const RSC_KEYBOARD_NUMERIC: u32 = 2;
pub const RSC_KEYBOARD_URL: u32 = 3;
pub const RSC_KEYBOARD_PHONE: u32 = 4;

/// The currently-focused field's keyboard-type hint, encoded as a
/// `RSC_KEYBOARD_*` constant — a native host polls this once per frame
/// tick (same polling shape `take_camera_request` uses) to know which
/// software keyboard layout to show.
pub fn focused_keyboard_type() -> u32 {
    match rosace_core::keyboard_type() {
        rosace_core::KeyboardType::Default => RSC_KEYBOARD_DEFAULT,
        rosace_core::KeyboardType::Email => RSC_KEYBOARD_EMAIL,
        rosace_core::KeyboardType::Numeric => RSC_KEYBOARD_NUMERIC,
        rosace_core::KeyboardType::Url => RSC_KEYBOARD_URL,
        rosace_core::KeyboardType::Phone => RSC_KEYBOARD_PHONE,
    }
}

/// One input event crossing the FFI boundary. Unused fields for a given
/// `kind` are ignored. `character` holds a Unicode scalar value (as `u32`)
/// for `RSC_EVENT_TEXT` and `RSC_KEY_CHAR`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct RscInputEventFfi {
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
        RSC_BUTTON_RIGHT => MouseButton::Right,
        RSC_BUTTON_MIDDLE => MouseButton::Middle,
        _ => MouseButton::Left, // RSC_BUTTON_LEFT and any unrecognized value
    }
}

fn key_from_ffi(k: u32, character: u32) -> Key {
    match k {
        RSC_KEY_ENTER => Key::Enter,
        RSC_KEY_ESCAPE => Key::Escape,
        RSC_KEY_SPACE => Key::Space,
        RSC_KEY_BACKSPACE => Key::Backspace,
        RSC_KEY_TAB => Key::Tab,
        RSC_KEY_ARROW_UP => Key::ArrowUp,
        RSC_KEY_ARROW_DOWN => Key::ArrowDown,
        RSC_KEY_ARROW_LEFT => Key::ArrowLeft,
        RSC_KEY_ARROW_RIGHT => Key::ArrowRight,
        RSC_KEY_SHIFT => Key::Shift,
        RSC_KEY_CONTROL => Key::Control,
        RSC_KEY_ALT => Key::Alt,
        RSC_KEY_META => Key::Meta,
        RSC_KEY_DELETE => Key::Delete,
        RSC_KEY_HOME => Key::Home,
        RSC_KEY_END => Key::End,
        // RSC_KEY_CHAR and any unrecognized value
        _ => Key::Char(char::from_u32(character).unwrap_or('\u{FFFD}')),
    }
}

impl From<RscInputEventFfi> for InputEvent {
    fn from(e: RscInputEventFfi) -> Self {
        match e.kind {
            RSC_EVENT_MOUSE_MOVE => InputEvent::MouseMove { x: e.x, y: e.y },
            RSC_EVENT_MOUSE_DOWN => InputEvent::MouseDown {
                x: e.x, y: e.y, button: button_from_ffi(e.button),
            },
            RSC_EVENT_MOUSE_UP => InputEvent::MouseUp {
                x: e.x, y: e.y, button: button_from_ffi(e.button),
            },
            RSC_EVENT_KEY_DOWN => InputEvent::KeyDown { key: key_from_ffi(e.key, e.character) },
            RSC_EVENT_KEY_UP => InputEvent::KeyUp { key: key_from_ffi(e.key, e.character) },
            RSC_EVENT_TEXT => InputEvent::Text {
                character: char::from_u32(e.character).unwrap_or('\u{FFFD}'),
            },
            RSC_EVENT_WINDOW_RESIZED => InputEvent::WindowResized { width: e.width, height: e.height },
            RSC_EVENT_SCROLL => InputEvent::Scroll {
                x: e.x, y: e.y, delta_x: e.delta_x, delta_y: e.delta_y,
            },
            RSC_EVENT_LIFECYCLE_ACTIVE =>
                InputEvent::Lifecycle(rosace_core::LifecycleState::Active),
            RSC_EVENT_LIFECYCLE_INACTIVE =>
                InputEvent::Lifecycle(rosace_core::LifecycleState::Inactive),
            RSC_EVENT_LIFECYCLE_BACKGROUND =>
                InputEvent::Lifecycle(rosace_core::LifecycleState::Background),
            RSC_EVENT_LIFECYCLE_SUSPENDED =>
                InputEvent::Lifecycle(rosace_core::LifecycleState::Suspended),
            _ => InputEvent::MouseMove { x: e.x, y: e.y },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mouse_down_round_trips_button() {
        let ffi = RscInputEventFfi {
            kind: RSC_EVENT_MOUSE_DOWN, x: 1.0, y: 2.0, button: RSC_BUTTON_RIGHT,
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
        let ffi = RscInputEventFfi {
            kind: RSC_EVENT_KEY_DOWN, x: 0.0, y: 0.0, button: 0,
            key: RSC_KEY_CHAR, character: 'a' as u32, width: 0, height: 0,
            delta_x: 0.0, delta_y: 0.0,
        };
        match InputEvent::from(ffi) {
            InputEvent::KeyDown { key: Key::Char(c) } => assert_eq!(c, 'a'),
            other => panic!("expected KeyDown(Char('a')), got {other:?}"),
        }
    }

    #[test]
    fn window_resized_reads_width_height() {
        let ffi = RscInputEventFfi {
            kind: RSC_EVENT_WINDOW_RESIZED, x: 0.0, y: 0.0, button: 0,
            key: 0, character: 0, width: 390, height: 844, delta_x: 0.0, delta_y: 0.0,
        };
        match InputEvent::from(ffi) {
            InputEvent::WindowResized { width, height } => assert_eq!((width, height), (390, 844)),
            other => panic!("expected WindowResized, got {other:?}"),
        }
    }

    #[test]
    fn lifecycle_kinds_map_to_the_matching_lifecycle_state() {
        for (kind, expected) in [
            (RSC_EVENT_LIFECYCLE_ACTIVE, rosace_core::LifecycleState::Active),
            (RSC_EVENT_LIFECYCLE_INACTIVE, rosace_core::LifecycleState::Inactive),
            (RSC_EVENT_LIFECYCLE_BACKGROUND, rosace_core::LifecycleState::Background),
            (RSC_EVENT_LIFECYCLE_SUSPENDED, rosace_core::LifecycleState::Suspended),
        ] {
            let ffi = RscInputEventFfi {
                kind, x: 0.0, y: 0.0, button: 0,
                key: 0, character: 0, width: 0, height: 0, delta_x: 0.0, delta_y: 0.0,
            };
            match InputEvent::from(ffi) {
                InputEvent::Lifecycle(state) => assert_eq!(
                    state, expected,
                    "kind {kind} must map to {expected:?}"
                ),
                other => panic!("expected Lifecycle, got {other:?}"),
            }
        }
    }

    #[test]
    fn key_down_delete_home_end_round_trip() {
        // Known Issue #15 (D116 Phase 28 Step 6) — these three were
        // reachable on desktop since Phase 28 Step 1 but had no FFI
        // constant at all until now.
        for (ffi_key, expected) in [
            (RSC_KEY_DELETE, Key::Delete),
            (RSC_KEY_HOME, Key::Home),
            (RSC_KEY_END, Key::End),
        ] {
            let ffi = RscInputEventFfi {
                kind: RSC_EVENT_KEY_DOWN, x: 0.0, y: 0.0, button: 0,
                key: ffi_key, character: 0, width: 0, height: 0, delta_x: 0.0, delta_y: 0.0,
            };
            match InputEvent::from(ffi) {
                InputEvent::KeyDown { key } => assert_eq!(key, expected, "RSC key {ffi_key} must map to {expected:?}"),
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
        assert_eq!(focused_keyboard_type(), RSC_KEYBOARD_DEFAULT);
    }

    #[test]
    fn focused_keyboard_type_reflects_rosace_core_state() {
        let _guard = KEYBOARD_TYPE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        rosace_core::set_keyboard_type(rosace_core::KeyboardType::Email);
        assert_eq!(focused_keyboard_type(), RSC_KEYBOARD_EMAIL);
        rosace_core::set_keyboard_type(rosace_core::KeyboardType::Numeric);
        assert_eq!(focused_keyboard_type(), RSC_KEYBOARD_NUMERIC);
        // Reset BEFORE the lock releases so the defaults test above can
        // never observe a leaked value.
        rosace_core::set_keyboard_type(rosace_core::KeyboardType::Default);
    }

    #[test]
    fn scroll_reads_deltas() {
        let ffi = RscInputEventFfi {
            kind: RSC_EVENT_SCROLL, x: 5.0, y: 6.0, button: 0,
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
