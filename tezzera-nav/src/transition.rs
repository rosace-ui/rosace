//! Screen-transition spring physics (D108/Phase 26 Step 3). Moved here from
//! `tezzera-nav-anim` so `ScreenNav` (the type real apps actually use) can
//! drive it directly — `tezzera-nav-anim` already depends on `tezzera-nav`,
//! so the dependency could not run the other way without a cycle.
//! `tezzera-nav-anim::transition` re-exports these same types so
//! `NavigatorAnimated` (unwired to anything real, kept as-is) still
//! compiles against the same public names.

use tezzera_animate::Spring;

#[derive(Debug, Clone, PartialEq)]
pub enum SlideDirection {
    Left,
    Right,
    Up,
    Down,
}

impl SlideDirection {
    /// Returns the (dx, dy) offset multiplier for the ENTERING screen.
    /// E.g. slide Right: entering comes from left → starts at (-1, 0)
    pub fn enter_from(&self) -> (f32, f32) {
        match self {
            SlideDirection::Right => (-1.0,  0.0),
            SlideDirection::Left  => ( 1.0,  0.0),
            SlideDirection::Down  => ( 0.0, -1.0),
            SlideDirection::Up    => ( 0.0,  1.0),
        }
    }

    /// Returns the (dx, dy) offset multiplier for the EXITING screen.
    pub fn exit_to(&self) -> (f32, f32) {
        match self {
            SlideDirection::Right => ( 1.0,  0.0),
            SlideDirection::Left  => (-1.0,  0.0),
            SlideDirection::Down  => ( 0.0,  1.0),
            SlideDirection::Up    => ( 0.0, -1.0),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TransitionStyle {
    /// No animation — instant switch.
    None,
    /// Slide the screens horizontally or vertically.
    Slide(SlideDirection),
    /// Cross-fade (enter fades in, exit fades out).
    Fade,
    /// New screen scales from 0.85 → 1.0, old screen scales 1.0 → 1.15.
    Scale,
}

/// Manages the animation state for a screen transition.
pub struct ScreenTransition {
    style: TransitionStyle,
    /// Spring for enter screen (moves from offset → 0).
    enter_spring_x: Spring,
    enter_spring_y: Spring,
    /// Spring for exit screen (moves from 0 → offset).
    exit_spring_x: Spring,
    exit_spring_y: Spring,
    /// 0.0 = fully entering, 1.0 = fully entered (for fade/scale).
    progress_spring: Spring,
    active: bool,
    viewport_w: f32,
    viewport_h: f32,
}

impl ScreenTransition {
    pub fn new(viewport_w: f32, viewport_h: f32) -> Self {
        let make_spring = || Spring::new(0.0, 0.0).stiffness(280.0).damping(26.0).mass(1.0);
        Self {
            style: TransitionStyle::None,
            enter_spring_x: make_spring(),
            enter_spring_y: make_spring(),
            exit_spring_x: make_spring(),
            exit_spring_y: make_spring(),
            progress_spring: Spring::new(0.0, 0.0).stiffness(200.0).damping(20.0).mass(1.0),
            active: false,
            viewport_w,
            viewport_h,
        }
    }

    /// Start a transition. Call when pushing or popping a route.
    pub fn trigger(&mut self, style: TransitionStyle) {
        self.style = style.clone();
        self.active = true;

        match &style {
            TransitionStyle::None => { self.active = false; }
            TransitionStyle::Slide(dir) => {
                let (ex, ey) = dir.enter_from();
                let (ox, oy) = dir.exit_to();
                // Enter: start offset, target 0
                self.enter_spring_x = Spring::new(ex * self.viewport_w, 0.0).stiffness(280.0).damping(26.0).mass(1.0);
                self.enter_spring_y = Spring::new(ey * self.viewport_h, 0.0).stiffness(280.0).damping(26.0).mass(1.0);
                // Exit: start 0, target offset
                self.exit_spring_x = Spring::new(0.0, ox * self.viewport_w).stiffness(280.0).damping(26.0).mass(1.0);
                self.exit_spring_y = Spring::new(0.0, oy * self.viewport_h).stiffness(280.0).damping(26.0).mass(1.0);
                self.progress_spring = Spring::new(0.0, 1.0).stiffness(200.0).damping(20.0).mass(1.0);
            }
            TransitionStyle::Fade | TransitionStyle::Scale => {
                self.enter_spring_x = Spring::new(0.0, 0.0).stiffness(1.0).damping(1.0).mass(1.0);
                self.enter_spring_y = Spring::new(0.0, 0.0).stiffness(1.0).damping(1.0).mass(1.0);
                self.exit_spring_x = Spring::new(0.0, 0.0).stiffness(1.0).damping(1.0).mass(1.0);
                self.exit_spring_y = Spring::new(0.0, 0.0).stiffness(1.0).damping(1.0).mass(1.0);
                self.progress_spring = Spring::new(0.0, 1.0).stiffness(200.0).damping(20.0).mass(1.0);
            }
        }
    }

    /// Advance the transition by `dt` seconds.
    /// Returns `(enter_offset_x, enter_offset_y, exit_offset_x, exit_offset_y, progress, is_complete)`.
    pub fn update(&mut self, dt: f32) -> (f32, f32, f32, f32, f32, bool) {
        if !self.active {
            return (0.0, 0.0, 0.0, 0.0, 1.0, true);
        }

        let ex = self.enter_spring_x.update(dt);
        let ey = self.enter_spring_y.update(dt);
        let ox = self.exit_spring_x.update(dt);
        let oy = self.exit_spring_y.update(dt);
        let progress = self.progress_spring.update(dt);

        let complete = self.enter_spring_x.is_settled()
            && self.enter_spring_y.is_settled()
            && self.progress_spring.is_settled();

        if complete { self.active = false; }

        (ex, ey, ox, oy, progress, complete)
    }

    pub fn is_active(&self) -> bool { self.active }
    pub fn style(&self) -> &TransitionStyle { &self.style }

    /// Resize viewport (e.g. on window resize).
    pub fn set_viewport(&mut self, w: f32, h: f32) {
        self.viewport_w = w;
        self.viewport_h = h;
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slide_direction_enter_from_right() {
        let (dx, dy) = SlideDirection::Right.enter_from();
        assert_eq!(dx, -1.0);
        assert_eq!(dy, 0.0);
    }

    #[test]
    fn slide_direction_exit_to_right() {
        let (dx, dy) = SlideDirection::Right.exit_to();
        assert_eq!(dx, 1.0);
        assert_eq!(dy, 0.0);
    }

    #[test]
    fn slide_direction_left() {
        let (ex, ey) = SlideDirection::Left.enter_from();
        assert_eq!(ex, 1.0);
        assert_eq!(ey, 0.0);
        let (ox, oy) = SlideDirection::Left.exit_to();
        assert_eq!(ox, -1.0);
        assert_eq!(oy, 0.0);
    }

    #[test]
    fn slide_direction_up() {
        let (ex, ey) = SlideDirection::Up.enter_from();
        assert_eq!(ex, 0.0);
        assert_eq!(ey, 1.0);
        let (ox, oy) = SlideDirection::Up.exit_to();
        assert_eq!(ox, 0.0);
        assert_eq!(oy, -1.0);
    }

    #[test]
    fn slide_direction_down() {
        let (ex, ey) = SlideDirection::Down.enter_from();
        assert_eq!(ex, 0.0);
        assert_eq!(ey, -1.0);
        let (ox, oy) = SlideDirection::Down.exit_to();
        assert_eq!(ox, 0.0);
        assert_eq!(oy, 1.0);
    }

    #[test]
    fn transition_style_eq() {
        assert_eq!(TransitionStyle::Fade, TransitionStyle::Fade);
        assert_eq!(TransitionStyle::Scale, TransitionStyle::Scale);
        assert_eq!(TransitionStyle::None, TransitionStyle::None);
        assert_eq!(
            TransitionStyle::Slide(SlideDirection::Right),
            TransitionStyle::Slide(SlideDirection::Right),
        );
        assert_ne!(
            TransitionStyle::Slide(SlideDirection::Left),
            TransitionStyle::Slide(SlideDirection::Right),
        );
    }

    #[test]
    fn screen_transition_new_not_active() {
        let st = ScreenTransition::new(640.0, 480.0);
        assert!(!st.is_active());
    }

    #[test]
    fn screen_transition_none_is_complete() {
        let mut st = ScreenTransition::new(640.0, 480.0);
        st.trigger(TransitionStyle::None);
        let (ex, ey, ox, oy, progress, complete) = st.update(1.0 / 60.0);
        assert!(!st.is_active());
        assert!(complete);
        assert_eq!(ex, 0.0);
        assert_eq!(ey, 0.0);
        assert_eq!(ox, 0.0);
        assert_eq!(oy, 0.0);
        assert_eq!(progress, 1.0);
    }

    #[test]
    fn screen_transition_trigger_slide_activates() {
        let mut st = ScreenTransition::new(640.0, 480.0);
        st.trigger(TransitionStyle::Slide(SlideDirection::Right));
        assert!(st.is_active());
        assert_eq!(st.style(), &TransitionStyle::Slide(SlideDirection::Right));
    }

    #[test]
    fn screen_transition_update_moves_toward_zero() {
        let mut st = ScreenTransition::new(640.0, 480.0);
        st.trigger(TransitionStyle::Slide(SlideDirection::Right));
        // Enter x starts at -640 (entering from left), should move toward 0
        let (ex, _ey, _ox, _oy, _progress, _complete) = st.update(1.0 / 60.0);
        // After one frame the enter x should be closer to 0 than -640
        assert!(ex > -640.0, "enter x should have moved toward 0, got {}", ex);
        assert!(ex < 0.0, "enter x should not have reached 0 yet, got {}", ex);
    }

    #[test]
    fn screen_transition_settles_after_many_frames() {
        let mut st = ScreenTransition::new(640.0, 480.0);
        st.trigger(TransitionStyle::Slide(SlideDirection::Right));
        for _ in 0..120 {
            st.update(1.0 / 60.0);
        }
        assert!(!st.is_active(), "transition should have settled after 120 frames");
    }

    #[test]
    fn screen_transition_fade_trigger() {
        let mut st = ScreenTransition::new(640.0, 480.0);
        st.trigger(TransitionStyle::Fade);
        assert!(st.is_active());
        assert_eq!(st.style(), &TransitionStyle::Fade);
        // For fade/scale, x/y springs are trivial (0→0), progress drives it
        let (ex, ey, ox, oy, _progress, _complete) = st.update(1.0 / 60.0);
        // Springs are 0→0, so positions stay near 0
        assert!(ex.abs() < 0.01);
        assert!(ey.abs() < 0.01);
        assert!(ox.abs() < 0.01);
        assert!(oy.abs() < 0.01);
    }

    #[test]
    fn screen_transition_scale_trigger() {
        let mut st = ScreenTransition::new(640.0, 480.0);
        st.trigger(TransitionStyle::Scale);
        assert!(st.is_active());
        assert_eq!(st.style(), &TransitionStyle::Scale);
    }

    #[test]
    fn screen_transition_set_viewport() {
        let mut st = ScreenTransition::new(640.0, 480.0);
        st.set_viewport(1280.0, 720.0);
        // Trigger a slide and verify the enter spring starts at the new viewport width
        st.trigger(TransitionStyle::Slide(SlideDirection::Right));
        // Enter x starts at -1280 (new viewport width)
        let (ex, _ey, _ox, _oy, _progress, _complete) = st.update(1.0 / 60.0);
        // Should be greater than -1280 (moved toward 0) but less than -640
        assert!(ex > -1280.0, "enter x should have moved from -1280, got {}", ex);
        assert!(ex < -640.0, "enter x should reflect new viewport, got {}", ex);
    }
}
