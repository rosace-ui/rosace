use rosace_core::types::{Rect, Size};
use rosace_state::Atom;
use super::{Widget, LayoutCtx, PaintCtx, BoxedWidget};

/// Fires `atom.set(Some(ctx.rect))` after paint, surfacing the widget's
/// window-pixel coordinates to user code without any widget modification.
///
/// ```rust,ignore
/// let anchor: Atom<Option<Rect>> = ctx.state(None);
/// RectReader::new(anchor.clone(), Button::new("Open"))
/// // After first paint: anchor.get() == Some(Rect { ... })
/// ```
pub struct RectReader {
    atom:  Atom<Option<Rect>>,
    child: BoxedWidget,
}

impl RectReader {
    pub fn new(atom: Atom<Option<Rect>>, child: impl Widget + 'static) -> Self {
        Self { atom, child: Box::new(child) }
    }
}

impl Widget for RectReader {
    fn children(&self) -> super::Children<'_> {
        super::Children::One(&*self.child)
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let r = ctx.rect;
        self.child.paint(&mut ctx.child(r));
        self.atom.set(Some(r));
    }
    // layout, flex_factor: protocol defaults delegate to the child.
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_core::types::{Point, Rect, Size};
    use rosace_layout::Constraints;
    use rosace_render::{FontCache, PictureRecorder};
    use rosace_state::use_atom;
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
    fn fires_atom_with_paint_rect() {
        let atom: Atom<Option<Rect>> = use_atom(None);
        let font = FontCache::system_ui()
            .or_else(FontCache::system_mono)
            .expect("no system font");
        let widget = RectReader::new(atom.clone(), Text::new("hi"));
        let mut recorder = PictureRecorder::new();
        let mut ctx = make_paint_ctx(&mut recorder, &font);
        widget.paint(&mut ctx);
        let rect = atom.get().expect("atom should be Some after paint");
        assert_eq!(rect.origin.x, 10.0);
        assert_eq!(rect.origin.y, 20.0);
        assert_eq!(rect.size.width, 100.0);
        assert_eq!(rect.size.height, 50.0);
    }
}
