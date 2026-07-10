//! A long list to feel real momentum/bounce scroll physics (D108/Phase 26
//! Step 2 — drag-to-pan, momentum coast, platform-default physics).

use rosace::prelude::*;

pub fn scroll_screen() -> impl Widget {
    let mut col = Column::new().padding(EdgeInsets::all(8.0));
    for i in 1..=40 {
        col = col.child(
            ListTile::new(format!("Row {i}"))
                .subtitle("Drag to pan; release to coast")
                .leading(Avatar::new(i.to_string())),
        );
    }
    col.scrollable()
}
