//! D-DEF-012 verification: REAL glassmorphism — a rounded panel that
//! BLURS everything beneath it (two-pass separable Gaussian through the
//! scene texture), tinted, with widgets painted on top of the glass.
//! Sharp content outside the panel, frosted inside = the proof.

use rosace::prelude::*;

struct GlassDemo;

impl Component for GlassDemo {
    fn build(&self, _ctx: &mut Context) -> Element {
        Scaffold::new(
            CustomPaint::new(|cx, _size| {
                let ox = cx.rect.origin.x;
                let oy = cx.rect.origin.y;

                // Busy, high-contrast content — blur must be OBVIOUS.
                cx.record(rosace::render::DrawCommand::FillGradient {
                    rect: r(ox + 20.0, oy + 20.0, 640.0, 330.0),
                    radius: 16.0,
                    from: Color { r: 187, g: 134, b: 252, a: 255 },
                    to: Color { r: 20, g: 60, b: 160, a: 255 },
                    vertical: true,
                });
                for i in 0..7 {
                    cx.fill_circle(
                        Point { x: ox + 70.0 + i as f32 * 90.0, y: oy + 110.0 + (i % 2) as f32 * 60.0 },
                        30.0,
                        Color { r: 255, g: 120 + (i * 18) as u8, b: 60, a: 255 },
                    );
                }
                for i in 0..6 {
                    cx.text(
                        "sharp text sharp text sharp text sharp text",
                        40.0, 40.0 + i as f32 * 52.0,
                        Color::WHITE, 17.0,
                    );
                }

                // THE GLASS: everything above, blurred+tinted behind a panel.
                cx.backdrop_blur(
                    r(ox + 130.0, oy + 70.0, 420.0, 230.0),
                    22.0,   // corner radius
                    14.0,   // blur strength (logical px)
                    Color { r: 255, g: 255, b: 255, a: 46 }, // faint white tint
                );

                // Widgets ON TOP of the glass stay sharp.
                cx.text_styled("Frosted glass — real backdrop blur",
                    160.0, 100.0, Color::WHITE, 22.0, rosace::render::FontWeight::Bold);
                cx.text("content behind is blurred; this text is not",
                    160.0, 140.0, Color { r: 235, g: 235, b: 245, a: 255 }, 14.0);
                cx.fill_rrect(r(ox + 160.0, oy + 190.0, 150.0, 44.0), 12.0,
                    Color { r: 124, g: 77, b: 255, a: 255 });
                cx.text("A button", 205.0, 202.0, Color::WHITE, 15.0);
            }),
        )
        .app_bar(AppBar::new("glassmorphism — D-DEF-012"))
        .into_element()
    }
}

fn r(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect { origin: Point { x, y }, size: Size { width: w, height: h } }
}

fn main() {
    env_logger::init();
    App::new().title("glass_demo").size(680, 440).launch(GlassDemo);
}
