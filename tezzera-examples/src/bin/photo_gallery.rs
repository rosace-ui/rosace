//! Photo Gallery — 3×2 grid of image cards with titles and tag badges.
//!
//! Demonstrates: Grid layout, layered rectangles, circles, color palette.
//! Run:   cargo run -p tezzera-examples --bin photo_gallery
//! Output: photo_gallery.png (720×560)

use tezzera_core::types::{Point, Rect, Size};
use tezzera_layout::{Constraints, Row};
use tezzera_render::{Color, FontCache, SkiaCanvas};

const W: u32 = 720;
const H: u32 = 560;

const BG:           Color = Color::rgb(245, 245, 250);
const HEADER_BG:    Color = Color::rgb(30,  30,  46);
const HEADER_TEXT:  Color = Color::rgb(240, 240, 255);
const CARD_BG:      Color = Color::rgb(255, 255, 255);
const CARD_BORDER:  Color = Color::rgb(215, 215, 228);
const TITLE_TEXT:   Color = Color::rgb(30,  30,  50);
const MUTED:        Color = Color::rgb(130, 130, 150);
const FOOTER_TEXT:  Color = Color::rgb(100, 100, 120);

const PHOTO_TINTS: [Color; 6] = [
    Color::rgb(255, 100, 80),
    Color::rgb( 80, 160, 255),
    Color::rgb( 90, 200, 120),
    Color::rgb(200,  90, 200),
    Color::rgb(255, 180, 40),
    Color::rgb( 60, 200, 200),
];
const LABELS: [&str; 6] = ["Sunset Cliffs", "Ocean Dive", "Forest Path", "Lavender Field", "Golden Hour", "Arctic Shore"];
const TAGS:   [&str; 6] = ["Nature", "Marine", "Hiking", "Flowers", "Travel", "Arctic"];
const TAG_TINTS: [Color; 6] = [
    Color::rgb(220, 70, 50),
    Color::rgb( 40, 120, 220),
    Color::rgb( 40, 150, 70),
    Color::rgb(160, 50, 160),
    Color::rgb(200, 120, 10),
    Color::rgb( 20, 150, 150),
];

fn card(c: &mut SkiaCanvas, font: &FontCache, x: f32, y: f32, w: f32, h: f32, i: usize) {
    // drop-shadow
    c.fill_rect(Rect { origin: Point { x: x+3.0, y: y+3.0 }, size: Size { width: w, height: h } }, Color::rgba(0,0,0,22));
    // card body
    c.fill_rect(Rect { origin: Point { x, y }, size: Size { width: w, height: h } }, CARD_BG);
    c.stroke_rect(Rect { origin: Point { x, y }, size: Size { width: w, height: h } }, CARD_BORDER, 1.0);

    let img_h = h * 0.58;
    // photo fill
    c.fill_rect(Rect { origin: Point { x, y }, size: Size { width: w, height: img_h } }, PHOTO_TINTS[i]);
    // bottom gradient overlay
    c.fill_rect(Rect { origin: Point { x, y: y + img_h - 28.0 }, size: Size { width: w, height: 28.0 } }, Color::rgba(0,0,0,55));
    // decorative bokeh circles
    c.fill_circle(Point { x: x + w*0.28, y: y + img_h*0.38 }, img_h*0.20, Color::rgba(255,255,255,30));
    c.fill_circle(Point { x: x + w*0.68, y: y + img_h*0.62 }, img_h*0.13, Color::rgba(255,255,255,20));
    // top-right index dot
    c.fill_circle(Point { x: x + w - 15.0, y: y + 15.0 }, 9.0, Color::rgba(0,0,0,90));

    let ty = y + img_h + 10.0;
    c.draw_text(LABELS[i], Point { x: x + 10.0, y: ty }, TITLE_TEXT, font, 12.0);

    // tag badge — keep rect width estimate based on char count
    let bw = TAGS[i].len() as f32 * 6.5 + 12.0;
    c.fill_rect(Rect { origin: Point { x: x+10.0, y: ty+18.0 }, size: Size { width: bw, height: 15.0 } }, TAG_TINTS[i]);
    c.draw_text(TAGS[i], Point { x: x+14.0, y: ty+20.5 }, Color::WHITE, font, 10.0);
    // like count
    c.draw_text("142 likes", Point { x: x + w - 72.0, y: ty+20.0 }, MUTED, font, 10.0);
}

fn main() {
    let font = FontCache::system_mono().expect("no system font");

    let mut c = SkiaCanvas::new(W, H);
    c.clear(BG);

    // Header
    c.fill_rect(Rect { origin: Point { x: 0.0, y: 0.0 }, size: Size { width: W as f32, height: 52.0 } }, HEADER_BG);
    c.draw_text("TEZZERA  Gallery",    Point { x: 24.0,              y: 18.0 }, HEADER_TEXT,                 &font, 16.0);
    c.draw_text("Grid  List  Filter",  Point { x: W as f32 - 168.0,  y: 20.0 }, Color::rgba(200,200,220,190), &font, 12.0);

    // 3-column × 2-row grid
    let cw = 208.0_f32;
    let ch = 196.0_f32;
    let gap = 12.0_f32;
    let ox = 18.0_f32;
    let oy = 64.0_f32;
    let cols = vec![
        Size { width: cw, height: ch },
        Size { width: cw, height: ch },
        Size { width: cw, height: ch },
    ];
    let row_con = Constraints::loose(W as f32 - ox * 2.0, ch);

    for row in 0..2usize {
        let layout = Row::new().spacing(gap).layout(row_con.clone(), &cols);
        let ry = oy + row as f32 * (ch + gap);
        for col in 0..3usize {
            let p = layout.child_positions[col];
            card(&mut c, &font, ox + p.x, ry + p.y, cw, ch, row * 3 + col);
        }
    }

    // Footer
    let fy = H as f32 - 34.0;
    c.fill_rect(Rect { origin: Point { x: 0.0, y: fy }, size: Size { width: W as f32, height: 34.0 } }, Color::rgb(250,250,255));
    c.stroke_rect(Rect { origin: Point { x: 0.0, y: fy }, size: Size { width: W as f32, height: 1.0 } }, CARD_BORDER, 1.0);
    c.draw_text("6 photos  •  TEZZERA UI Framework  •  Phase 1 Demo", Point { x: 24.0, y: fy + 11.0 }, FOOTER_TEXT, &font, 11.0);

    let png = c.encode_png().expect("encode png");
    std::fs::write("photo_gallery.png", png).expect("write png");
    println!("Saved  photo_gallery.png  ({W}x{H})");
}
