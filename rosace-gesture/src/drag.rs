use rosace_platform::{InputEvent, MouseButton};
use crate::event::{DragPhase, GestureEvent};
use crate::recognizer::GestureRecognizer;

const DRAG_THRESHOLD: f32 = 5.0; // px before drag begins

pub struct DragRecognizer {
    down: Option<(f32, f32)>,
    prev: Option<(f32, f32)>,
    dragging: bool,
}

impl DragRecognizer {
    pub fn new() -> Self {
        Self { down: None, prev: None, dragging: false }
    }

    pub fn is_dragging(&self) -> bool {
        self.dragging
    }
}

impl Default for DragRecognizer {
    fn default() -> Self {
        Self::new()
    }
}

impl GestureRecognizer for DragRecognizer {
    fn on_event(&mut self, event: &InputEvent, _dt: f32) -> Option<GestureEvent> {
        match event {
            InputEvent::MouseDown { x, y, button: MouseButton::Left } => {
                self.down = Some((*x, *y));
                self.prev = Some((*x, *y));
                self.dragging = false;
                None
            }
            InputEvent::MouseMove { x, y } => {
                let (px, py) = self.prev?;
                let (sx, sy) = self.down?;

                let total_dist = ((x - sx).powi(2) + (y - sy).powi(2)).sqrt();

                if !self.dragging && total_dist < DRAG_THRESHOLD {
                    return None;
                }

                let phase = if !self.dragging {
                    self.dragging = true;
                    DragPhase::Begin
                } else {
                    DragPhase::Move
                };

                self.prev = Some((*x, *y));
                Some(GestureEvent::Drag { dx: x - px, dy: y - py, x: *x, y: *y, phase })
            }
            InputEvent::MouseUp { x, y, button: MouseButton::Left } => {
                let was_dragging = self.dragging;
                let prev = self.prev;
                self.down = None;
                self.prev = None;
                self.dragging = false;

                if was_dragging {
                    let (px, py) = prev.unwrap_or((*x, *y));
                    Some(GestureEvent::Drag { dx: x - px, dy: y - py, x: *x, y: *y, phase: DragPhase::End })
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn reset(&mut self) {
        self.down = None;
        self.prev = None;
        self.dragging = false;
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

    #[test]
    fn drag_recognizer_new_not_dragging() {
        let r = DragRecognizer::new();
        assert!(!r.is_dragging());
        assert!(r.down.is_none());
        assert!(r.prev.is_none());
    }

    #[test]
    fn drag_begin_after_threshold() {
        let mut r = DragRecognizer::new();
        r.on_event(&down(0.0, 0.0), 0.0);
        // Move beyond DRAG_THRESHOLD (5px)
        let result = r.on_event(&mv(10.0, 0.0), 0.016);
        match result {
            Some(GestureEvent::Drag { phase: DragPhase::Begin, .. }) => {}
            other => panic!("Expected Drag Begin, got {:?}", other),
        }
        assert!(r.is_dragging());
    }

    #[test]
    fn drag_move_emits_delta() {
        let mut r = DragRecognizer::new();
        r.on_event(&down(0.0, 0.0), 0.0);
        r.on_event(&mv(10.0, 0.0), 0.0); // Begin
        let result = r.on_event(&mv(15.0, 5.0), 0.0); // Move
        match result {
            Some(GestureEvent::Drag { dx, dy, phase: DragPhase::Move, .. }) => {
                assert_eq!(dx, 5.0);
                assert_eq!(dy, 5.0);
            }
            other => panic!("Expected Drag Move with delta, got {:?}", other),
        }
    }

    #[test]
    fn drag_end_on_mouse_up() {
        let mut r = DragRecognizer::new();
        r.on_event(&down(0.0, 0.0), 0.0);
        r.on_event(&mv(10.0, 0.0), 0.0); // Begin
        let result = r.on_event(&up(10.0, 0.0), 0.0);
        match result {
            Some(GestureEvent::Drag { phase: DragPhase::End, .. }) => {}
            other => panic!("Expected Drag End, got {:?}", other),
        }
        assert!(!r.is_dragging());
    }

    #[test]
    fn drag_no_begin_below_threshold() {
        let mut r = DragRecognizer::new();
        r.on_event(&down(0.0, 0.0), 0.0);
        // Only 3px — below threshold
        let result = r.on_event(&mv(3.0, 0.0), 0.0);
        assert_eq!(result, None);
        assert!(!r.is_dragging());
    }

    #[test]
    fn drag_reset() {
        let mut r = DragRecognizer::new();
        r.on_event(&down(0.0, 0.0), 0.0);
        r.on_event(&mv(10.0, 0.0), 0.0);
        r.reset();
        assert!(!r.is_dragging());
        assert!(r.down.is_none());
        assert!(r.prev.is_none());
    }

    #[test]
    fn drag_is_dragging_flag() {
        let mut r = DragRecognizer::new();
        assert!(!r.is_dragging());
        r.on_event(&down(0.0, 0.0), 0.0);
        assert!(!r.is_dragging());
        r.on_event(&mv(10.0, 0.0), 0.0);
        assert!(r.is_dragging());
    }

    #[test]
    fn drag_dx_dy_correct() {
        let mut r = DragRecognizer::new();
        r.on_event(&down(100.0, 100.0), 0.0);
        // Begin drag
        let begin = r.on_event(&mv(106.0, 103.0), 0.0);
        match begin {
            Some(GestureEvent::Drag { dx, dy, phase: DragPhase::Begin, .. }) => {
                assert_eq!(dx, 6.0);
                assert_eq!(dy, 3.0);
            }
            other => panic!("Expected Drag Begin, got {:?}", other),
        }
    }

    #[test]
    fn drag_no_end_without_drag() {
        let mut r = DragRecognizer::new();
        r.on_event(&down(0.0, 0.0), 0.0);
        // Release without moving past threshold
        let result = r.on_event(&up(2.0, 2.0), 0.0);
        assert_eq!(result, None);
    }
}
