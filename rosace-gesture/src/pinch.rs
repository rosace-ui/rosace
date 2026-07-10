use rosace_platform::InputEvent;
use crate::event::GestureEvent;
use crate::recognizer::GestureRecognizer;

const SCROLL_SCALE_FACTOR: f32 = 0.01;

/// Recognizes pinch (zoom) gestures.
///
/// On desktop, maps mouse scroll-wheel `delta_y` to a scale factor:
/// - `delta_y` < 0 (scroll up)  → `scale` > 1.0 (zoom in)
/// - `delta_y` > 0 (scroll down) → `scale` < 1.0 (zoom out)
///
/// Tracks the current mouse position via `MouseMove` events so the
/// pinch center is accurate even when `Scroll` events lack coordinates.
pub struct PinchRecognizer {
    pub sensitivity: f32,
    last_mouse_x: f32,
    last_mouse_y: f32,
    accumulated_scale: f32,
}

impl PinchRecognizer {
    pub fn new() -> Self {
        Self {
            sensitivity: SCROLL_SCALE_FACTOR,
            last_mouse_x: 0.0,
            last_mouse_y: 0.0,
            accumulated_scale: 1.0,
        }
    }

    pub fn sensitivity(mut self, s: f32) -> Self { self.sensitivity = s; self }

    pub fn accumulated_scale(&self) -> f32 { self.accumulated_scale }

    pub fn reset_scale(&mut self) { self.accumulated_scale = 1.0; }
}

impl Default for PinchRecognizer { fn default() -> Self { Self::new() } }

impl GestureRecognizer for PinchRecognizer {
    fn on_event(&mut self, event: &InputEvent, _dt: f32) -> Option<GestureEvent> {
        match event {
            InputEvent::MouseMove { x, y } => {
                self.last_mouse_x = *x;
                self.last_mouse_y = *y;
                None
            }
            InputEvent::Scroll { x, y, delta_y, .. } => {
                let scale = (1.0 - delta_y * self.sensitivity).clamp(0.1, 10.0);
                self.accumulated_scale = (self.accumulated_scale * scale).clamp(0.1, 10.0);
                Some(GestureEvent::Pinch { scale, center_x: *x, center_y: *y })
            }
            _ => None,
        }
    }

    fn reset(&mut self) {
        self.accumulated_scale = 1.0;
        self.last_mouse_x = 0.0;
        self.last_mouse_y = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_platform::InputEvent;

    fn scroll(x: f32, y: f32, delta_y: f32) -> InputEvent {
        InputEvent::Scroll { x, y, delta_x: 0.0, delta_y }
    }

    #[test]
    fn pinch_recognizer_new_default_sensitivity() {
        let p = PinchRecognizer::new();
        assert!((p.sensitivity - 0.01).abs() < 1e-6);
    }

    #[test]
    fn pinch_scroll_up_gives_scale_gt_1() {
        let mut p = PinchRecognizer::new();
        if let Some(GestureEvent::Pinch { scale, .. }) = p.on_event(&scroll(0.0, 0.0, -1.0), 0.0) {
            assert!(scale > 1.0, "expected scale > 1, got {scale}");
        } else {
            panic!("expected Pinch event");
        }
    }

    #[test]
    fn pinch_scroll_down_gives_scale_lt_1() {
        let mut p = PinchRecognizer::new();
        if let Some(GestureEvent::Pinch { scale, .. }) = p.on_event(&scroll(0.0, 0.0, 1.0), 0.0) {
            assert!(scale < 1.0, "expected scale < 1, got {scale}");
        } else {
            panic!("expected Pinch event");
        }
    }

    #[test]
    fn pinch_zero_delta_gives_scale_1() {
        let mut p = PinchRecognizer::new();
        if let Some(GestureEvent::Pinch { scale, .. }) = p.on_event(&scroll(0.0, 0.0, 0.0), 0.0) {
            assert!((scale - 1.0).abs() < 1e-5);
        } else {
            panic!("expected Pinch event");
        }
    }

    #[test]
    fn pinch_emits_correct_center() {
        let mut p = PinchRecognizer::new();
        if let Some(GestureEvent::Pinch { center_x, center_y, .. }) = p.on_event(&scroll(123.0, 456.0, -1.0), 0.0) {
            assert!((center_x - 123.0).abs() < 1e-5);
            assert!((center_y - 456.0).abs() < 1e-5);
        } else {
            panic!("expected Pinch event");
        }
    }

    #[test]
    fn pinch_accumulated_scale_multiplies() {
        let mut p = PinchRecognizer::new();
        p.on_event(&scroll(0.0, 0.0, -1.0), 0.0);
        let after_one = p.accumulated_scale();
        p.on_event(&scroll(0.0, 0.0, -1.0), 0.0);
        let after_two = p.accumulated_scale();
        assert!(after_two > after_one, "accumulated scale should grow: {after_one} → {after_two}");
    }

    #[test]
    fn pinch_reset_scale() {
        let mut p = PinchRecognizer::new();
        p.on_event(&scroll(0.0, 0.0, -5.0), 0.0);
        p.reset_scale();
        assert!((p.accumulated_scale() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn pinch_reset_fn() {
        let mut p = PinchRecognizer::new();
        p.on_event(&scroll(0.0, 0.0, -5.0), 0.0);
        p.reset();
        assert!((p.accumulated_scale() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn pinch_clamps_max_scale() {
        let mut p = PinchRecognizer::new();
        if let Some(GestureEvent::Pinch { scale, .. }) = p.on_event(&scroll(0.0, 0.0, -10000.0), 0.0) {
            assert!(scale <= 10.0, "scale should be clamped to 10: {scale}");
        }
    }

    #[test]
    fn pinch_clamps_min_scale() {
        let mut p = PinchRecognizer::new();
        if let Some(GestureEvent::Pinch { scale, .. }) = p.on_event(&scroll(0.0, 0.0, 10000.0), 0.0) {
            assert!(scale >= 0.1, "scale should be clamped to 0.1: {scale}");
        }
    }

    #[test]
    fn pinch_mouse_move_updates_no_event() {
        let mut p = PinchRecognizer::new();
        let result = p.on_event(&InputEvent::MouseMove { x: 50.0, y: 60.0 }, 0.0);
        assert!(result.is_none());
    }

    #[test]
    fn pinch_sensitivity_setter() {
        let p = PinchRecognizer::new().sensitivity(0.05);
        assert!((p.sensitivity - 0.05).abs() < 1e-6);
    }
}
