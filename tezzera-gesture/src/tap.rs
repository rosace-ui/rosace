use tezzera_platform::{InputEvent, MouseButton};
use crate::event::GestureEvent;
use crate::recognizer::GestureRecognizer;

const DOUBLE_TAP_WINDOW: f32 = 0.300; // seconds
const MAX_TAP_MOVE: f32 = 10.0;       // pixels — movement beyond this cancels tap

pub struct TapRecognizer {
    down_pos: Option<(f32, f32)>,
    last_tap_pos: Option<(f32, f32)>,
    last_tap_time: f32,   // elapsed seconds
    elapsed: f32,
    long_press_threshold: f32,
    down_elapsed: f32,
}

impl TapRecognizer {
    pub fn new() -> Self {
        Self {
            down_pos: None,
            last_tap_pos: None,
            last_tap_time: -1.0,
            elapsed: 0.0,
            long_press_threshold: 0.5,
            down_elapsed: 0.0,
        }
    }

    pub fn long_press_threshold(mut self, secs: f32) -> Self {
        self.long_press_threshold = secs;
        self
    }
}

impl Default for TapRecognizer {
    fn default() -> Self {
        Self::new()
    }
}

impl GestureRecognizer for TapRecognizer {
    fn on_event(&mut self, event: &InputEvent, dt: f32) -> Option<GestureEvent> {
        self.elapsed += dt;

        match event {
            InputEvent::MouseDown { x, y, button: MouseButton::Left } => {
                self.down_pos = Some((*x, *y));
                self.down_elapsed = self.elapsed;
                None
            }
            InputEvent::MouseMove { x, y } => {
                if let Some((dx, dy)) = self.down_pos {
                    if (x - dx).abs() > MAX_TAP_MOVE || (y - dy).abs() > MAX_TAP_MOVE {
                        self.down_pos = None; // cancelled — too much movement
                    }
                }
                None
            }
            InputEvent::MouseUp { x, y, button: MouseButton::Left } => {
                let Some((dx, dy)) = self.down_pos.take() else { return None; };
                if (*x - dx).abs() > MAX_TAP_MOVE || (*y - dy).abs() > MAX_TAP_MOVE {
                    return None;
                }
                let held = self.elapsed - self.down_elapsed;
                if held >= self.long_press_threshold {
                    return Some(GestureEvent::LongPress { x: *x, y: *y, duration_secs: held });
                }

                // Check for double tap
                let since_last = self.elapsed - self.last_tap_time;
                if since_last < DOUBLE_TAP_WINDOW {
                    if let Some((lx, ly)) = self.last_tap_pos {
                        if (x - lx).abs() < 40.0 && (y - ly).abs() < 40.0 {
                            self.last_tap_time = -1.0;
                            self.last_tap_pos = None;
                            return Some(GestureEvent::DoubleTap { x: *x, y: *y });
                        }
                    }
                }

                self.last_tap_time = self.elapsed;
                self.last_tap_pos = Some((*x, *y));
                Some(GestureEvent::Tap { x: *x, y: *y })
            }
            _ => None,
        }
    }

    fn reset(&mut self) {
        self.down_pos = None;
        self.last_tap_pos = None;
        self.last_tap_time = -1.0;
        self.down_elapsed = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tezzera_platform::InputEvent;

    fn down(x: f32, y: f32) -> InputEvent {
        InputEvent::MouseDown { x, y, button: MouseButton::Left }
    }

    fn up(x: f32, y: f32) -> InputEvent {
        InputEvent::MouseUp { x, y, button: MouseButton::Left }
    }

    fn mv(x: f32, y: f32) -> InputEvent {
        InputEvent::MouseMove { x, y }
    }

    #[test]
    fn tap_recognizer_new() {
        let r = TapRecognizer::new();
        assert!(r.down_pos.is_none());
        assert!(r.last_tap_pos.is_none());
        assert_eq!(r.last_tap_time, -1.0);
        assert_eq!(r.long_press_threshold, 0.5);
    }

    #[test]
    fn tap_fires_on_mouse_up() {
        let mut r = TapRecognizer::new();
        r.on_event(&down(10.0, 20.0), 0.0);
        let result = r.on_event(&up(10.0, 20.0), 0.1);
        assert_eq!(result, Some(GestureEvent::Tap { x: 10.0, y: 20.0 }));
    }

    #[test]
    fn tap_cancelled_by_movement() {
        let mut r = TapRecognizer::new();
        r.on_event(&down(0.0, 0.0), 0.0);
        r.on_event(&mv(50.0, 0.0), 0.05); // large move cancels
        let result = r.on_event(&up(50.0, 0.0), 0.1);
        assert_eq!(result, None);
    }

    #[test]
    fn tap_long_press_detected() {
        let mut r = TapRecognizer::new();
        r.on_event(&down(5.0, 5.0), 0.0);
        // dt = 0.6 simulates 600ms held down
        let result = r.on_event(&up(5.0, 5.0), 0.6);
        match result {
            Some(GestureEvent::LongPress { x, y, duration_secs }) => {
                assert_eq!(x, 5.0);
                assert_eq!(y, 5.0);
                assert!(duration_secs >= 0.5);
            }
            other => panic!("Expected LongPress, got {:?}", other),
        }
    }

    #[test]
    fn tap_double_tap_within_window() {
        let mut r = TapRecognizer::new();
        // First tap
        r.on_event(&down(10.0, 10.0), 0.0);
        let first = r.on_event(&up(10.0, 10.0), 0.05);
        assert_eq!(first, Some(GestureEvent::Tap { x: 10.0, y: 10.0 }));

        // Second tap within 300ms
        r.on_event(&down(10.0, 10.0), 0.05);
        let second = r.on_event(&up(10.0, 10.0), 0.05); // total elapsed = 0.15
        assert_eq!(second, Some(GestureEvent::DoubleTap { x: 10.0, y: 10.0 }));
    }

    #[test]
    fn tap_double_tap_too_slow() {
        let mut r = TapRecognizer::new();
        // First tap at elapsed=0
        r.on_event(&down(10.0, 10.0), 0.0);
        let first = r.on_event(&up(10.0, 10.0), 0.05);
        assert_eq!(first, Some(GestureEvent::Tap { x: 10.0, y: 10.0 }));

        // Second tap - but elapsed += 0.4 so since_last = 0.45 > DOUBLE_TAP_WINDOW
        r.on_event(&down(10.0, 10.0), 0.0);
        let second = r.on_event(&up(10.0, 10.0), 0.4);
        assert_eq!(second, Some(GestureEvent::Tap { x: 10.0, y: 10.0 }));
    }

    #[test]
    fn tap_reset_clears_state() {
        let mut r = TapRecognizer::new();
        r.on_event(&down(10.0, 10.0), 0.0);
        r.reset();
        assert!(r.down_pos.is_none());
        assert!(r.last_tap_pos.is_none());
        assert_eq!(r.last_tap_time, -1.0);
        assert_eq!(r.down_elapsed, 0.0);
    }

    #[test]
    fn tap_no_event_on_mouse_down() {
        let mut r = TapRecognizer::new();
        let result = r.on_event(&down(0.0, 0.0), 0.0);
        assert_eq!(result, None);
    }

    #[test]
    fn tap_no_event_on_mouse_move() {
        let mut r = TapRecognizer::new();
        let result = r.on_event(&mv(5.0, 5.0), 0.0);
        assert_eq!(result, None);
    }

    #[test]
    fn tap_long_press_threshold_setter() {
        let r = TapRecognizer::new().long_press_threshold(1.0);
        assert_eq!(r.long_press_threshold, 1.0);
    }
}
