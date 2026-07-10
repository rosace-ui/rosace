use rosace_core::types::{Point, Rect, Size};
use rosace_platform::{InputEvent, MouseButton, App};
use rosace_render::{Color, FontCache, SkiaCanvas};
use rosace_state::use_atom;

const W: u32 = 480;
const H: u32 = 320;

const BG:     Color = Color::rgb(18, 18, 28);
const ACCENT: Color = Color::rgb(103, 80, 164);
const TEXT:   Color = Color::rgb(230, 225, 229);

fn main() {
    let font = FontCache::system_mono().expect("no system font found");
    let count = use_atom(0_i32);
    let mut mx = 0.0_f32;
    let mut my = 0.0_f32;

    App::new()
        .title("TEST-PHASE3-APP — ROSACE")
        .size(W, H)
        .run(move |canvas: &mut SkiaCanvas, events: &[InputEvent]| {
            for ev in events {
                match ev {
                    InputEvent::MouseDown { x, y, button: MouseButton::Left } => {
                        let bx = W as f32 / 2.0 - 60.0;
                        let by = H as f32 / 2.0 + 20.0;
                        if *x >= bx && *x <= bx + 120.0 && *y >= by && *y <= by + 40.0 {
                            count.update(|n| n + 1);
                        }
                    }
                    InputEvent::MouseMove { x, y } => { mx = *x; my = *y; }
                    _ => {}
                }
            }

            canvas.clear(BG);

            canvas.draw_text(
                "TEST-PHASE3-APP",
                Point { x: W as f32 / 2.0 - 15.0 * 9.0 / 2.0, y: 40.0 },
                TEXT, &font, 18.0,
            );

            let label = format!("{}", count.get());
            canvas.draw_text(
                &label,
                Point { x: W as f32 / 2.0 - label.len() as f32 * 12.0 / 2.0, y: H as f32 / 2.0 - 20.0 },
                ACCENT, &font, 36.0,
            );

            let bx = W as f32 / 2.0 - 60.0;
            let by = H as f32 / 2.0 + 20.0;
            let hovered = mx >= bx && mx <= bx + 120.0 && my >= by && my <= by + 40.0;
            let btn_color = if hovered { Color::rgb(130, 100, 200) } else { ACCENT };
            canvas.fill_rect(
                Rect { origin: Point { x: bx, y: by }, size: Size { width: 120.0, height: 40.0 } },
                btn_color,
            );
            canvas.draw_text("Click me", Point { x: bx + 18.0, y: by + 12.0 }, Color::WHITE, &font, 14.0);
        });
}
