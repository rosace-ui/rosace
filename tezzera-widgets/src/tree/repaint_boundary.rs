use tezzera_core::types::Size;
use super::{Widget, LayoutCtx, PaintCtx};

/// Wraps a child widget in an explicit paint cache boundary.
///
/// When the child's content has not changed since the last frame, the
/// render loop replays its cached Picture without calling `child.paint()`.
/// This is useful for expensive static subtrees — e.g. large icon sets,
/// complex backdrops, or chart backgrounds.
///
/// The cache is invalidated when the render loop marks the containing
/// component's subtree dirty (i.e. an atom the component reads has changed).
/// No manual invalidation is needed.
pub struct RepaintBoundary<W: Widget + Send + Sync + 'static> {
    pub child: W,
}

impl<W: Widget + Send + Sync + 'static> RepaintBoundary<W> {
    pub fn new(child: W) -> Self {
        Self { child }
    }
}

impl<W: Widget + Send + Sync + 'static> Widget for RepaintBoundary<W> {
    fn children(&self) -> super::Children<'_> {
        super::Children::One(&self.child)
    }
    // layout, paint, flex_factor: protocol defaults delegate to the child.
}

#[cfg(test)]
mod tests {
    use super::*;
    use tezzera_core::types::Size;
    use tezzera_layout::Constraints;
    use std::sync::{Arc, Mutex};

    struct SizeBox { w: f32, h: f32, paint_count: Arc<Mutex<u32>> }
    impl Widget for SizeBox {
        fn layout(&self, _ctx: &LayoutCtx) -> Size {
            Size { width: self.w, height: self.h }
        }
        fn paint(&self, _ctx: &mut PaintCtx) {
            *self.paint_count.lock().unwrap() += 1;
        }
    }

    #[test]
    fn repaint_boundary_delegates_layout() {
        use tezzera_render::FontCache;
        use tezzera_theme::built_in::light_theme;

        let paint_count = Arc::new(Mutex::new(0u32));
        let rb = RepaintBoundary::new(SizeBox { w: 100.0, h: 50.0, paint_count });
        let font = FontCache::system_ui()
            .or_else(FontCache::system_mono)
            .expect("font");
        let theme = light_theme();
        let lctx = LayoutCtx::new(Constraints::tight(200.0, 200.0), &font, &theme);
        let size = rb.layout(&lctx);
        assert_eq!(size.width, 100.0);
        assert_eq!(size.height, 50.0);
    }

}
