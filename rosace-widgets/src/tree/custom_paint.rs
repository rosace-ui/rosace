use std::sync::Arc;

use rosace_core::types::Size;
use super::{Widget, LayoutCtx, PaintCtx};

/// A leaf widget that draws with a closure (D100).
///
/// The closure receives the standard [`PaintCtx`] — it records DrawCommands
/// into the display list like every built-in widget, so caching, replay,
/// clipping, and HiDPI scaling all apply. It never touches pixels; for
/// pixel-level control use [`DrawCommand::BlitRgba`].
///
/// ```rust,ignore
/// CustomPaint::new(|cx, size| {
///     cx.fill_circle(
///         Point { x: cx.rect.origin.x + size.width / 2.0,
///                 y: cx.rect.origin.y + size.height / 2.0 },
///         size.width.min(size.height) / 2.0,
///         Color::rgb(255, 111, 97),
///     );
/// })
/// .size(120.0, 120.0)
/// ```
///
/// Repaint coupling is automatic: read your atoms in the owning component's
/// `build()` and the painter re-records whenever they change. (A per-widget
/// `repaint_when` knob becomes meaningful once per-child picture caching
/// lands — Phase 20 Step 5.)
///
/// [`DrawCommand::BlitRgba`]: rosace_render::DrawCommand::BlitRgba
pub struct CustomPaint {
    painter: Arc<dyn Fn(&mut PaintCtx, Size) + Send + Sync>,
    width: Option<f32>,
    height: Option<f32>,
}

impl CustomPaint {
    pub fn new(painter: impl Fn(&mut PaintCtx, Size) + Send + Sync + 'static) -> Self {
        Self { painter: Arc::new(painter), width: None, height: None }
    }

    pub fn width(mut self, w: f32) -> Self { self.width = Some(w); self }
    pub fn height(mut self, h: f32) -> Self { self.height = Some(h); self }
    pub fn size(mut self, w: f32, h: f32) -> Self {
        self.width = Some(w);
        self.height = Some(h);
        self
    }
}

impl Widget for CustomPaint {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let c = ctx.constraints;
        // Explicit size wins; otherwise fill the available bounded space.
        let w = self.width.unwrap_or_else(|| c.max_width_f32());
        let h = self.height.unwrap_or_else(|| c.max_height_f32());
        c.constrain(Size {
            width:  if w.is_finite() { w } else { 0.0 },
            height: if h.is_finite() { h } else { 0.0 },
        })
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let size = ctx.rect.size;
        (self.painter)(ctx, size);
    }
}
