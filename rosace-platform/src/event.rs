#[derive(Debug, Clone)]
pub enum InputEvent {
    MouseMove     { x: f32, y: f32 },
    MouseDown     { x: f32, y: f32, button: MouseButton },
    MouseUp       { x: f32, y: f32, button: MouseButton },
    KeyDown       { key: Key },
    KeyUp         { key: Key },
    Text          { character: char },
    WindowResized { width: u32, height: u32 },
    /// Mouse scroll wheel / trackpad. `delta_y` < 0 = scroll up, > 0 = scroll
    /// down; `delta_x` < 0 = scroll left, > 0 = scroll right.
    Scroll        { x: f32, y: f32, delta_x: f32, delta_y: f32 },
    /// Trackpad pinch-to-zoom (macOS/iOS only — winit's `PinchGesture`,
    /// Phase 32 `InteractiveViewer`). `delta` mirrors winit's own field:
    /// positive = magnify, negative = shrink, NOT a multiplier.
    Pinch         { x: f32, y: f32, delta: f32 },
    /// A real OS IME session event (D116 Step 6) — CJK/complex-script
    /// composition. Desktop: translated from winit's `WindowEvent::Ime`.
    /// Reuses `rosace_ime::ImeEvent` as the wire payload rather than
    /// re-declaring the same four variants here — `rosace-ime` is a
    /// tiny, dependency-light crate (only `rosace-trace`) both this
    /// crate and `rosace`'s dispatch layer can depend on without a
    /// layering cycle.
    Ime(rosace_ime::ImeEvent),
    /// An OS app-lifecycle transition (D042/D110, Phase 29 Step 1) —
    /// reported by a mobile native host over the FFI bridge
    /// (`RSC_EVENT_LIFECYCLE_*`). Desktop winit never sends this (desktop
    /// lifecycle is explicitly out of Phase 29's scope); the engine's
    /// dispatch writes it to `rosace_core::set_app_lifecycle`.
    Lifecycle(rosace_core::LifecycleState),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MouseButton { Left, Right, Middle }

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Key {
    Enter, Escape, Space, Backspace, Tab,
    ArrowUp, ArrowDown, ArrowLeft, ArrowRight,
    /// Forward delete — distinct from `Backspace` (D112/Phase 28 Step 1:
    /// real `TextInput` editing needs both directions).
    Delete,
    Home, End,
    Shift, Control, Alt, Meta,
    /// Function key F12 — the DevTools element-inspector toggle (D123/O2).
    /// The only F-key the framework routes; app widgets ignore it.
    F12,
    /// Function key F11 — the DevTools trace/network panel toggle (D123/O5).
    F11,
    Char(char),
}
