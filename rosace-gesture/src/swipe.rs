use rosace_platform::{InputEvent, MouseButton};
use crate::event::{GestureEvent, SwipeDirection};
use crate::recognizer::GestureRecognizer;

const MIN_SWIPE_DISTANCE: f32 = 80.0;
const MIN_SWIPE_VELOCITY: f32 = 200.0; // px/s

pub struct SwipeRecognizer {
    start: Option<(f32, f32)>,
    start_time: f32,
    elapsed: f32,
    last_pos: Option<(f32, f32)>,
}

impl SwipeRecognizer {
    pub fn new() -> Self {
        Self { start: None, start_time: 0.0, elapsed: 0.0, last_pos: None }
    }
}

impl Default for SwipeRecognizer {
    fn default() -> Self {
        Self::new()
    }
}

impl GestureRecognizer for SwipeRecognizer {
    fn on_event(&mut self, event: &InputEvent, dt: f32) -> Option<GestureEvent> {
        self.elapsed += dt;

        match event {
            InputEvent::MouseDown { x, y, button: MouseButton::Left } => {
                self.start = Some((*x, *y));
                self.start_time = self.elapsed;
                self.last_pos = Some((*x, *y));
                None
            }
            InputEvent::MouseMove { x, y } => {
                self.last_pos = Some((*x, *y));
                None
            }
            InputEvent::MouseUp { x, y, button: MouseButton::Left } => {
                let (sx, sy) = self.start.take()?;
                let dx = x - sx;
                let dy = y - sy;
                let dist = (dx * dx + dy * dy).sqrt();
                if dist < MIN_SWIPE_DISTANCE { return None; }

                let duration = (self.elapsed - self.start_time).max(0.001);
                let velocity = dist / duration;
                if velocity < MIN_SWIPE_VELOCITY { return None; }

                let direction = if dx.abs() > dy.abs() {
                    if dx > 0.0 { SwipeDirection::Right } else { SwipeDirection::Left }
                } else {
                    if dy > 0.0 { SwipeDirection::Down } else { SwipeDirection::Up }
                };

                Some(GestureEvent::Swipe { direction, velocity, x: *x, y: *y })
            }
            _ => None,
        }
    }

    fn reset(&mut self) {
        self.start = None;
        self.last_pos = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_platform::InputEvent;

    fn down(x: f32, y: f32) -> InputEvent {
        InputEvent::MouseDown { x, y, button: MouseButton::Left }
    }

    fn up(x: f32, y: f32) -> InputEvent {
        InputEvent::MouseUp { x, y, button: MouseButton::Left }
    }

    fn mv(x: f32, y: f32) -> InputEvent {
        InputEvent::MouseMove { x, y }
    }

    // Simulate a fast swipe: down at start, move, up at end with enough dt
    // to keep velocity above threshold but short enough to be fast.
    // MIN_SWIPE_DISTANCE = 80px, MIN_SWIPE_VELOCITY = 200 px/s
    // If we swipe 100px in 0.2s -> velocity = 500 px/s -> passes
    fn make_swipe(r: &mut SwipeRecognizer, sx: f32, sy: f32, ex: f32, ey: f32) -> Option<GestureEvent> {
        r.on_event(&down(sx, sy), 0.0);
        r.on_event(&mv(ex, ey), 0.1);
        r.on_event(&up(ex, ey), 0.1) // 0.2s total
    }

    #[test]
    fn swipe_recognizer_new() {
        let r = SwipeRecognizer::new();
        assert!(r.start.is_none());
        assert!(r.last_pos.is_none());
    }

    #[test]
    fn swipe_detects_right() {
        let mut r = SwipeRecognizer::new();
        let result = make_swipe(&mut r, 0.0, 0.0, 100.0, 0.0);
        match result {
            Some(GestureEvent::Swipe { direction, .. }) => {
                assert_eq!(direction, SwipeDirection::Right);
            }
            other => panic!("Expected Swipe Right, got {:?}", other),
        }
    }

    #[test]
    fn swipe_detects_left() {
        let mut r = SwipeRecognizer::new();
        let result = make_swipe(&mut r, 100.0, 0.0, 0.0, 0.0);
        match result {
            Some(GestureEvent::Swipe { direction, .. }) => {
                assert_eq!(direction, SwipeDirection::Left);
            }
            other => panic!("Expected Swipe Left, got {:?}", other),
        }
    }

    #[test]
    fn swipe_detects_up() {
        let mut r = SwipeRecognizer::new();
        let result = make_swipe(&mut r, 0.0, 100.0, 0.0, 0.0);
        match result {
            Some(GestureEvent::Swipe { direction, .. }) => {
                assert_eq!(direction, SwipeDirection::Up);
            }
            other => panic!("Expected Swipe Up, got {:?}", other),
        }
    }

    #[test]
    fn swipe_detects_down() {
        let mut r = SwipeRecognizer::new();
        let result = make_swipe(&mut r, 0.0, 0.0, 0.0, 100.0);
        match result {
            Some(GestureEvent::Swipe { direction, .. }) => {
                assert_eq!(direction, SwipeDirection::Down);
            }
            other => panic!("Expected Swipe Down, got {:?}", other),
        }
    }

    #[test]
    fn swipe_too_short_no_event() {
        let mut r = SwipeRecognizer::new();
        // Only 20px — below MIN_SWIPE_DISTANCE
        let result = make_swipe(&mut r, 0.0, 0.0, 20.0, 0.0);
        assert_eq!(result, None);
    }

    #[test]
    fn swipe_reset() {
        let mut r = SwipeRecognizer::new();
        r.on_event(&down(0.0, 0.0), 0.0);
        r.reset();
        assert!(r.start.is_none());
        assert!(r.last_pos.is_none());
    }

    #[test]
    fn swipe_no_event_without_down() {
        let mut r = SwipeRecognizer::new();
        // MouseUp without a prior MouseDown should return None
        let result = r.on_event(&up(100.0, 0.0), 0.2);
        assert_eq!(result, None);
    }
}
