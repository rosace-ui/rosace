//! An icon's `size` is a design dimension, not type: it must render at
//! exactly the size the app asked for at every OS text setting.
//!
//! This lives in its own integration binary ON PURPOSE. It mutates
//! `set_media_query`, which is a process-global; the unit-test suite runs in
//! parallel and any test measuring text at the same moment would see the
//! changed scale. A separate binary is a separate process, so there is
//! nothing to race with.

use rosace_core::media_query::{set_media_query, MediaQuery};
use rosace_core::types::{Point, Rect, Size};
use rosace_render::{DrawCommand, FontCache, PictureRecorder};
use rosace_widgets::tree::{Icon, IconKind, PaintCtx, RenderTree, Widget};
use std::cell::RefCell;
use std::rc::Rc;

/// The px of every glyph the icon drew.
fn drawn_px(size: f32) -> Vec<f32> {
    let font = FontCache::embedded();
    let mut rec = PictureRecorder::new();
    let tree = Rc::new(RefCell::new(RenderTree::new()));
    {
        let mut ctx = PaintCtx::root(
            &mut rec,
            Rect { origin: Point { x: 0.0, y: 0.0 }, size: Size { width: 100.0, height: 100.0 } },
            &font,
            rosace_theme::built_in::dark_theme(),
            tree,
        );
        Icon::new(IconKind::Star).size(size).paint(&mut ctx);
    }
    rec.finish().commands.iter().filter_map(|c| match c {
        DrawCommand::DrawText { px, .. } => Some(*px),
        _ => None,
    }).collect()
}

#[test]
fn an_icon_renders_at_its_designed_size_at_every_os_text_scale() {
    let base = drawn_px(24.0);
    assert_eq!(base, vec![24.0], "baseline: one glyph at exactly the requested size");

    // Icons used to be drawn through `draw_text_at`, which multiplies px by
    // `text_scale` — while the centring metrics (`glyph`/`ascender`) do not
    // scale, and `layout` returns the raw size. So at 2.0 the glyph was
    // rendered at 48px, positioned by metrics solved at 24px, inside a 24px
    // box: oversized AND mis-centred, with the error growing linearly.
    for scale in [1.5_f32, 2.0, 3.0] {
        set_media_query(MediaQuery { text_scale: scale, ..MediaQuery::default() });
        assert_eq!(
            drawn_px(24.0), vec![24.0],
            "icon must not grow at text_scale {scale}",
        );
    }
    set_media_query(MediaQuery::default());
}
