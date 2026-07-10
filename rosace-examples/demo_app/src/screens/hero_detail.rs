//! The Hero detail screen. The photo shares a `.hero_tag("cover-photo")`
//! with Home's thumbnail (D108/Phase 26 Step 5) — pushing/popping between
//! the two screens morphs it between their two rects instead of just
//! popping in/out at its new size and position.

use rosace::prelude::*;

pub fn hero_detail_screen() -> impl Widget {
    Column::new()
        .padding(EdgeInsets::all(16.0))
        .spacing(16.0)
        .child(
            Image::file("assets_photo.png")
                .width(320.0)
                .height(213.0)
                .hero_tag("cover-photo"),
        )
        .child(Text::title("Shared-element transition"))
        .child(Text::new(
            "This photo is tagged .hero_tag(\"cover-photo\") on both the \
             Home screen (small) and here (large). Pushing/popping between \
             them morphs the same floating copy between the two rects — \
             it doesn't just pop to its new size.",
        ))
}
