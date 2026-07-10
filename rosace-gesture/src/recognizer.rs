use rosace_platform::InputEvent;
use crate::event::GestureEvent;

/// A gesture recognizer converts raw InputEvents into GestureEvents.
pub trait GestureRecognizer {
    /// Feed an input event. Returns a gesture if one was detected.
    fn on_event(&mut self, event: &InputEvent, dt: f32) -> Option<GestureEvent>;

    /// Reset recognizer state (e.g. on focus loss).
    fn reset(&mut self);
}
