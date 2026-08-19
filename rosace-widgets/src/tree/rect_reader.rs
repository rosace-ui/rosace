use std::sync::Arc;

use rosace_core::types::Rect;
use super::{Widget, PaintCtx, BoxedWidget};

/// Reports the widget's window-pixel rect after each paint, surfacing its
/// coordinates to user code without any widget modification.
///
/// ```rust,ignore
/// RectReader::new(Button::new("Open"), move |r| anchor.set(r))
/// ```
///
/// This is a channel OUT, not state: the rect is produced by layout and read
/// by the caller, so it is a callback rather than a value the caller owns.
pub struct RectReader {
    on_rect: Arc<dyn Fn(Rect) + Send + Sync>,
    child:   BoxedWidget,
}

impl RectReader {
    pub fn new(child: impl Widget + 'static, on_rect: impl Fn(Rect) + Send + Sync + 'static) -> Self {
        Self { on_rect: Arc::new(on_rect), child: Arc::new(child) }
    }
}

impl Widget for RectReader {
    fn children(&self) -> super::Children<'_> {
        super::Children::One(&*self.child)
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let r = ctx.rect;
        ctx.paint_child(r, &*self.child);
        (self.on_rect)(r);
    }
    // layout, flex_factor: protocol defaults delegate to the child.
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_core::types::{Point, Rect, Size};
    
    use rosace_render::{FontCache, PictureRecorder};
    use std::sync::Mutex;
    use rosace_theme::built_in;
    use std::rc::Rc;
    use std::cell::RefCell;
    use crate::tree::{RenderTree, Text};

    fn make_paint_ctx<'a>(
        recorder: &'a mut PictureRecorder,
        font: &'a FontCache,
    ) -> PaintCtx<'a> {
        let theme = built_in::dark_theme();
        let mut ctx = PaintCtx::root(
            recorder,
            Rect {
                origin: Point { x: 10.0, y: 20.0 },
                size: Size { width: 100.0, height: 50.0 },
            },
            font,
            theme,
            Rc::new(RefCell::new(RenderTree::new())),
        );
        ctx.clip_rect = None;
        ctx
    }

    #[test]
    fn reports_the_paint_rect() {
        let seen: Arc<Mutex<Option<Rect>>> = Arc::new(Mutex::new(None));
        let sink = Arc::clone(&seen);
        let font = FontCache::system_ui()
            .or_else(FontCache::system_mono)
            .expect("no system font");
        let widget = RectReader::new(Text::new("hi"), move |r| *sink.lock().unwrap() = Some(r));
        let mut recorder = PictureRecorder::new();
        let mut ctx = make_paint_ctx(&mut recorder, &font);
        widget.paint(&mut ctx);
        let rect = seen.lock().unwrap().expect("callback should have fired during paint");
        assert_eq!(rect.origin.x, 10.0);
        assert_eq!(rect.origin.y, 20.0);
        assert_eq!(rect.size.width, 100.0);
        assert_eq!(rect.size.height, 50.0);
    }
}
