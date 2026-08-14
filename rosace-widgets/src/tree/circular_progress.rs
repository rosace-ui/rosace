use rosace_core::types::{Point, Size};
use rosace_render::Color;
use super::{Widget, LayoutCtx, PaintCtx};

/// A circular progress indicator — a determinate ring (`value` 0..1) or an
/// indeterminate spinner. Draws a ring segment (the FillArc primitive).
pub struct CircularProgress {
    value: Option<f32>,     // None = indeterminate spinner
    diameter: f32,
    thickness: f32,
    color: Option<Color>,
    track: Option<Color>,
}

impl CircularProgress {
    /// Determinate ring filled to `value` (0..1).
    pub fn new(value: f32) -> Self {
        Self { value: Some(value.clamp(0.0, 1.0)), diameter: 36.0, thickness: 4.0, color: None, track: None }
    }
    /// Indeterminate spinner.
    pub fn spinner() -> Self {
        Self { value: None, diameter: 36.0, thickness: 4.0, color: None, track: None }
    }
    pub fn diameter(mut self, d: f32) -> Self { self.diameter = d; self }
    pub fn thickness(mut self, t: f32) -> Self { self.thickness = t; self }
    pub fn color(mut self, c: Color) -> Self { self.color = Some(c); self }
    pub fn track(mut self, c: Color) -> Self { self.track = Some(c); self }
}

impl Widget for CircularProgress {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        ctx.constraints.constrain(Size { width: self.diameter, height: self.diameter })
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        // Quality Bar §5. An indeterminate spinner has no value to report —
        // announcing "0" would be a lie, so only a determinate ring carries one.
        let mut sem = super::SemanticsProps::new(rosace_core::Role::ProgressBar).label("Progress");
        if let Some(v) = self.value {
            sem = sem.value(format!("{}%", (v * 100.0).round() as i32));
        }
        ctx.semantics(sem);
        let r = ctx.rect;
        let color = self.color.unwrap_or_else(|| ctx.tc(ctx.theme.colors.primary));
        let center = Point { x: r.origin.x + r.size.width / 2.0, y: r.origin.y + r.size.height / 2.0 };
        let radius = (self.diameter - self.thickness) / 2.0;

        match self.value {
            Some(v) => {
                // Track ring + value arc from 12 o'clock, clockwise.
                // A white track is invisible on a light theme.
                let ts = ctx.tc(ctx.theme.colors.on_surface);
                let track = self.track.unwrap_or(Color::rgba(ts.r, ts.g, ts.b, 28));
                ctx.fill_arc(center, radius, self.thickness, 0.0, 360.0, track);
                if v > 0.0 {
                    ctx.fill_arc(center, radius, self.thickness, -90.0, 360.0 * v, color);
                }
            }
            None => {
                // Spinner: a 270° arc whose start rotates with the clock.
                let t = super::anim_clock();
                let start = (t * 360.0) % 360.0;
                ctx.fill_arc(center, radius, self.thickness, start, 270.0, color);
                ctx.request_animation();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_core::types::{Point, Rect, Size};
    use rosace_render::{DrawCommand, FontCache, PictureRecorder};
    use crate::tree::RenderTree;
    use std::{cell::RefCell, rc::Rc};

    /// Every arc drawn: (start_deg, sweep_deg, colour).
    fn arcs(w: CircularProgress) -> Vec<(f32, f32, rosace_render::Color)> {
        let font = FontCache::embedded();
        let mut rec = PictureRecorder::new();
        {
            let mut ctx = PaintCtx::root(
                &mut rec,
                Rect { origin: Point { x: 0.0, y: 0.0 }, size: Size { width: 64.0, height: 64.0 } },
                &font,
                rosace_theme::built_in::dark_theme(),
                Rc::new(RefCell::new(RenderTree::new())),
            );
            w.paint(&mut ctx);
        }
        rec.finish().commands.iter().filter_map(|c| match c {
            DrawCommand::FillArc { start_deg, sweep_deg, color, .. } =>
                Some((*start_deg, *sweep_deg, *color)),
            _ => None,
        }).collect()
    }

    /// Determinate: a full track ring plus a value arc starting at 12
    /// o'clock. Starting anywhere else makes progress appear to begin
    /// mid-circle.
    #[test]
    fn a_determinate_ring_draws_a_track_then_a_value_arc_from_twelve_oclock() {
        let a = arcs(CircularProgress::new(0.25));
        assert_eq!(a.len(), 2, "track + value");
        assert_eq!((a[0].0, a[0].1), (0.0, 360.0), "the track is a full ring");
        assert_eq!(a[1].0, -90.0, "the value arc starts at 12 o'clock");
        assert_eq!(a[1].1, 90.0, "and sweeps a quarter for 0.25");
    }

    #[test]
    fn a_zero_value_draws_the_track_only() {
        assert_eq!(arcs(CircularProgress::new(0.0)).len(), 1);
    }

    /// The indeterminate spinner is a partial arc with no track — a full
    /// ring behind it would make it read as "nearly complete" rather than
    /// "working".
    #[test]
    fn the_spinner_is_a_partial_arc_with_no_track() {
        let a = arcs(CircularProgress::spinner());
        assert_eq!(a.len(), 1, "no track ring");
        assert!(a[0].1 < 360.0, "a partial sweep, not a closed ring");
    }

    /// The track defaults to a tint of `on_surface`. It was hardcoded WHITE,
    /// which is invisible on a light theme — the ring looked like it had no
    /// track at all.
    #[test]
    fn the_track_follows_the_theme_rather_than_being_hardcoded_white() {
        let light = {
            let font = FontCache::embedded();
            let mut rec = PictureRecorder::new();
            {
                let mut ctx = PaintCtx::root(
                    &mut rec,
                    Rect { origin: Point { x: 0.0, y: 0.0 }, size: Size { width: 64.0, height: 64.0 } },
                    &font,
                    rosace_theme::built_in::light_theme(),
                    Rc::new(RefCell::new(RenderTree::new())),
                );
                CircularProgress::new(0.5).paint(&mut ctx);
            }
            rec.finish().commands.iter().find_map(|c| match c {
                DrawCommand::FillArc { color, .. } => Some(*color),
                _ => None,
            }).expect("a track must be drawn")
        };
        assert!(light.r < 250 || light.g < 250 || light.b < 250,
            "the track is white on a LIGHT theme, so it is invisible: {light:?}");
    }

    #[test]
    fn an_explicit_track_colour_wins() {
        let a = arcs(CircularProgress::new(0.5).track(rosace_render::Color::rgb(9, 9, 200)));
        assert_eq!((a[0].2.r, a[0].2.g, a[0].2.b), (9, 9, 200));
    }
}
