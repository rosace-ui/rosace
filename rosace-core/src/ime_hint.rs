//! IME candidate-window anchor (D116 Step 6): a `GlobalAtom<Option<Rect>>`
//! reporting the focused editable's caret rect for the platform layer to
//! forward to `winit::window::Window::set_ime_cursor_area`.
//!
//! This lives here (not in `rosace-widgets`, where the caret geometry is
//! actually computed) because it's a bridge BETWEEN two layers that don't
//! depend on each other: `rosace-widgets` (owns `TextLayoutSnapshot`, knows
//! the real caret rect) and `rosace-platform` (owns the winit `Window`,
//! the only thing that can call `set_ime_cursor_area`). `rosace-core` is
//! the lowest common layer both already depend on — the same shape as
//! `platform.rs`'s `use_platform`/`set_platform` and `safe_area`'s
//! provider.
//!
//! `TextInput`/`TextArea` set this every paint while focused (never write
//! `None` from a widget — an unfocused field must not clobber a
//! DIFFERENT, currently-focused field's rect if painted later in the same
//! frame); `FrameEngine::paint` clears it once at the start of each frame
//! so it self-corrects if focus moved to nothing at all — the same
//! "declare fresh, read once" convention every other per-frame render-tree
//! field already uses.

use rosace_state::GlobalAtom;
use rosace_trace::event::AtomId;
use crate::types::Rect;

/// Reserved atom ID — see `platform.rs`'s `PLATFORM_ATOM_ID` (0xFFFD),
/// `safe_area::SAFE_AREA_ATOM_ID` (0xFFFE), `rosace_theme`'s `THEME_ATOM_ID`
/// (0xFFFF), and `rosace-ffi`'s `CAMERA_PERMISSION_ATOM_ID` (0xFFFC).
const IME_CURSOR_AREA_ATOM_ID: AtomId = AtomId(0xFFFB);

static IME_CURSOR_AREA: GlobalAtom<Option<Rect>> = GlobalAtom::new(IME_CURSOR_AREA_ATOM_ID, || None);

/// Report the focused editable's caret rect (world-space logical pixels)
/// this frame — called by `TextInput`/`TextArea` paint while focused.
pub fn set_ime_cursor_area(rect: Option<Rect>) {
    IME_CURSOR_AREA.set(rect);
}

/// The current IME anchor rect, if any editable is focused — read once
/// per frame by `rosace-platform` after the paint closure returns.
pub fn ime_cursor_area() -> Option<Rect> {
    IME_CURSOR_AREA.get()
}

/// Keyboard-type hint (D116 Step 6) — which OS soft-keyboard layout a
/// mobile host should show for the focused field. A desktop hardware
/// keyboard has no such concept, so this is a pure no-op there (checked:
/// `rosace-platform`/`app.rs` never reads it) — it exists ONLY for the
/// D106 FFI bridge, the same "real Rust plumbing, native host piece
/// verified separately on real hardware" shape as camera permission and
/// the `RSC_KEY_*` mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KeyboardType {
    #[default]
    Default,
    Email,
    Numeric,
    Url,
    Phone,
}

/// Reserved atom ID — see this file's other constant for the full list.
const KEYBOARD_TYPE_ATOM_ID: AtomId = AtomId(0xFFFA);

static KEYBOARD_TYPE: GlobalAtom<KeyboardType> = GlobalAtom::new(KEYBOARD_TYPE_ATOM_ID, || KeyboardType::Default);

/// Report the focused field's keyboard-type hint this frame — called by
/// `TextInput` paint while focused. Same "declare fresh every frame,
/// widget only ever writes while it's the focused one" convention as
/// [`set_ime_cursor_area`].
pub fn set_keyboard_type(kt: KeyboardType) {
    KEYBOARD_TYPE.set(kt);
}

/// The focused field's keyboard-type hint — polled by a native mobile
/// host (`rosace-ffi`) once per frame to drive `UIKeyboardType`/
/// `InputType` on the real OS software keyboard.
pub fn keyboard_type() -> KeyboardType {
    KEYBOARD_TYPE.get()
}
