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
    Char(char),
}
