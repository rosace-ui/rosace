use tezzera_platform::{InputEvent, Key, MouseButton};

/// Simulates `InputEvent` sequences without a real window.
pub struct EventSim;

impl EventSim {
    /// Generate a MouseDown + MouseUp pair at `(x, y)`.
    pub fn tap(x: f32, y: f32) -> Vec<InputEvent> {
        vec![
            InputEvent::MouseDown { x, y, button: MouseButton::Left },
            InputEvent::MouseUp   { x, y, button: MouseButton::Left },
        ]
    }

    /// Generate a right-click at `(x, y)`.
    pub fn right_click(x: f32, y: f32) -> Vec<InputEvent> {
        vec![
            InputEvent::MouseDown { x, y, button: MouseButton::Right },
            InputEvent::MouseUp   { x, y, button: MouseButton::Right },
        ]
    }

    /// Generate KeyDown + KeyUp + Text events for each character in `s`.
    pub fn type_text(s: &str) -> Vec<InputEvent> {
        let mut events = Vec::with_capacity(s.len() * 3);
        for c in s.chars() {
            events.push(InputEvent::KeyDown { key: Key::Char(c) });
            events.push(InputEvent::Text { character: c });
            events.push(InputEvent::KeyUp   { key: Key::Char(c) });
        }
        events
    }

    /// Generate a Scroll event at `(x, y)` with vertical `delta`.
    pub fn scroll(x: f32, y: f32, delta: f32) -> Vec<InputEvent> {
        vec![InputEvent::Scroll { x, y, delta_x: 0.0, delta_y: delta }]
    }

    /// Generate a mouse move event.
    pub fn mouse_move(x: f32, y: f32) -> Vec<InputEvent> {
        vec![InputEvent::MouseMove { x, y }]
    }

    /// Generate a KeyDown + KeyUp pair for a non-character key.
    pub fn key_press(key: Key) -> Vec<InputEvent> {
        vec![
            InputEvent::KeyDown { key },
            InputEvent::KeyUp   { key },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tap_produces_two_events() {
        let events = EventSim::tap(10.0, 20.0);
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn tap_first_is_mouse_down() {
        let events = EventSim::tap(0.0, 0.0);
        assert!(matches!(events[0], InputEvent::MouseDown { .. }));
    }

    #[test]
    fn tap_second_is_mouse_up() {
        let events = EventSim::tap(0.0, 0.0);
        assert!(matches!(events[1], InputEvent::MouseUp { .. }));
    }

    #[test]
    fn tap_coordinates() {
        let events = EventSim::tap(5.0, 7.0);
        if let InputEvent::MouseDown { x, y, .. } = &events[0] {
            assert_eq!(*x, 5.0);
            assert_eq!(*y, 7.0);
        }
    }

    #[test]
    fn right_click_produces_two_events() {
        let events = EventSim::right_click(1.0, 2.0);
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], InputEvent::MouseDown { button: MouseButton::Right, .. }));
    }

    #[test]
    fn type_text_events_per_char() {
        let events = EventSim::type_text("ab");
        // 3 events per char (KeyDown + Text + KeyUp)
        assert_eq!(events.len(), 6);
    }

    #[test]
    fn type_text_empty() {
        let events = EventSim::type_text("");
        assert!(events.is_empty());
    }

    #[test]
    fn type_text_contains_text_event() {
        let events = EventSim::type_text("x");
        assert!(events.iter().any(|e| matches!(e, InputEvent::Text { character: 'x' })));
    }

    #[test]
    fn scroll_produces_one_event() {
        let events = EventSim::scroll(0.0, 0.0, -3.0);
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], InputEvent::Scroll { delta_y: _, .. }));
    }

    #[test]
    fn scroll_delta() {
        let events = EventSim::scroll(1.0, 2.0, -5.0);
        if let InputEvent::Scroll { delta_y, .. } = &events[0] {
            assert_eq!(*delta_y, -5.0);
        }
    }

    #[test]
    fn key_press_produces_down_and_up() {
        let events = EventSim::key_press(Key::Enter);
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], InputEvent::KeyDown { key: Key::Enter }));
        assert!(matches!(events[1], InputEvent::KeyUp   { key: Key::Enter }));
    }

    #[test]
    fn mouse_move_produces_one_event() {
        let events = EventSim::mouse_move(3.0, 4.0);
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], InputEvent::MouseMove { .. }));
    }
}
