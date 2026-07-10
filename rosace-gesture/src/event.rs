#[derive(Debug, Clone, PartialEq)]
pub enum SwipeDirection {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DragPhase {
    Begin,
    Move,
    End,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GestureEvent {
    Tap { x: f32, y: f32 },
    DoubleTap { x: f32, y: f32 },
    LongPress { x: f32, y: f32, duration_secs: f32 },
    Swipe { direction: SwipeDirection, velocity: f32, x: f32, y: f32 },
    Drag { dx: f32, dy: f32, x: f32, y: f32, phase: DragPhase },
    /// Pinch/zoom — scale > 1.0 means zoom in, < 1.0 means zoom out.
    Pinch { scale: f32, center_x: f32, center_y: f32 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gesture_event_tap_clone() {
        let tap = GestureEvent::Tap { x: 1.0, y: 2.0 };
        let cloned = tap.clone();
        assert_eq!(tap, cloned);
    }

    #[test]
    fn gesture_event_double_tap_eq() {
        let a = GestureEvent::DoubleTap { x: 10.0, y: 20.0 };
        let b = GestureEvent::DoubleTap { x: 10.0, y: 20.0 };
        assert_eq!(a, b);
    }

    #[test]
    fn swipe_direction_variants() {
        let directions = [
            SwipeDirection::Left,
            SwipeDirection::Right,
            SwipeDirection::Up,
            SwipeDirection::Down,
        ];
        assert_eq!(directions[0], SwipeDirection::Left);
        assert_eq!(directions[1], SwipeDirection::Right);
        assert_eq!(directions[2], SwipeDirection::Up);
        assert_eq!(directions[3], SwipeDirection::Down);
    }

    #[test]
    fn drag_phase_variants() {
        let phases = [DragPhase::Begin, DragPhase::Move, DragPhase::End];
        assert_eq!(phases[0], DragPhase::Begin);
        assert_eq!(phases[1], DragPhase::Move);
        assert_eq!(phases[2], DragPhase::End);
    }
}
