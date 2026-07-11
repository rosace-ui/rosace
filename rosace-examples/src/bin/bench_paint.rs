//! Phase 27 Step 3 measurement harness (D109): per-frame CPU cost of the
//! rasterization stage, CPU (tiny-skia) vs GPU-shapes mode, on a
//! representative screen's command stream.
//!
//! This measures exactly the stage the phase moves — `clear` +
//! `play_picture` on a full-window canvas — for a gallery-like screen
//! (cards with shadows, buttons, strokes, circles, arcs, text). It is the
//! cost an ANIMATED frame pays on the CPU every frame for the animation's
//! duration: in CPU mode that's full shape+text rasterization; in
//! GPU-shapes mode it's shape→quad conversion + text rasterization +
//! segment extraction (shape pixels never touch the CPU). The GPU present
//! itself runs off-CPU in both modes (the CPU-mode full-buffer upload cost
//! isn't captured here, so the real-world gap is LARGER than reported).
//!
//! Run: `cargo run --release -p rosace-examples --bin bench_paint`

use std::sync::Arc;
use std::time::Instant;

use rosace::core::types::{Point, Rect, Size};
use rosace::render::canvas::Color;
use rosace::render::{DrawCommand, FontCache, FontWeight, PictureRecorder, SkiaCanvas};

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect { origin: Point { x, y }, size: Size { width: w, height: h } }
}

/// A gallery-like screen: 24 rows of card + shadow + button + stroke +
/// badge circle + label text, plus a spinner arc — the shape/text mix a
/// real app paints.
fn representative_picture(frame: usize) -> rosace::render::Picture {
    let mut rec = PictureRecorder::new();
    let violet = Color { r: 187, g: 134, b: 252, a: 255 };
    let card   = Color { r: 45,  g: 47,  b: 51,  a: 255 };
    let white  = Color { r: 230, g: 230, b: 235, a: 255 };
    // The animated scalar — changes every frame like a real animation, so
    // no stage can cache the whole frame away.
    let t = (frame % 120) as f32 / 120.0;

    for i in 0..24 {
        let y = 8.0 + i as f32 * 30.0;
        rec.push(DrawCommand::DrawShadow { rect: rect(20.0, y, 400.0, 24.0), radius: 8.0, color: Color { r: 0, g: 0, b: 0, a: 120 }, blur: 8.0 });
        rec.push(DrawCommand::FillRRect { rect: rect(20.0, y, 400.0, 24.0), radius: 8.0, color: card });
        rec.push(DrawCommand::FillRRect { rect: rect(28.0, y + 4.0, 70.0 + t * 20.0, 16.0), radius: 6.0, color: violet });
        rec.push(DrawCommand::StrokeRRect { rect: rect(110.0, y + 4.0, 60.0, 16.0), radius: 6.0, color: white, width: 1.5 });
        rec.push(DrawCommand::FillCircle { center: Point { x: 195.0, y: y + 12.0 }, radius: 7.0, color: violet });
        rec.push(DrawCommand::DrawText {
            text: format!("Row label {i} — kerned text run"),
            origin: Point { x: 215.0, y: y + 5.0 },
            color: white, px: 12.0, weight: FontWeight::Regular,
        });
    }
    rec.push(DrawCommand::FillArc {
        center: Point { x: 450.0, y: 40.0 }, radius: 18.0, thickness: 4.0,
        start_deg: t * 360.0, sweep_deg: 270.0, color: violet,
    });
    // One image blit — stays CPU in both modes.
    let px: Arc<Vec<u8>> = Arc::new(vec![180u8; 32 * 32 * 4]);
    rec.push(DrawCommand::BlitRgba { pixels: px, src_width: 32, src_height: 32, dest_rect: rect(440.0, 80.0, 32.0, 32.0), opacity: 1.0 });
    rec.finish()
}

fn bench(gpu: bool, frames: usize, font: &FontCache) -> f64 {
    // Retina-sized window buffer: 900×640 logical at 2x.
    let mut canvas = SkiaCanvas::new_hidpi(1800, 1280, 2.0);
    canvas.set_gpu_shapes(gpu);
    let bg = Color { r: 30, g: 31, b: 34, a: 255 };

    // Warm caches (glyphs, shadow masks) so we measure steady-state
    // animated frames, not first-frame cache builds.
    for f in 0..10 {
        canvas.clear(bg);
        canvas.play_picture(&representative_picture(f), font);
        let _ = canvas.take_frame_items();
        let _ = canvas.take_shader_quads();
    }

    let start = Instant::now();
    for f in 0..frames {
        canvas.clear(bg);
        canvas.play_picture(&representative_picture(f), font);
        // Drain like the platform does each painted frame.
        let _ = canvas.take_frame_items();
        let _ = canvas.take_shader_quads();
    }
    start.elapsed().as_secs_f64() * 1000.0 / frames as f64
}

fn main() {
    let font = FontCache::system_ui().unwrap_or_else(FontCache::embedded);
    let frames = 300;
    let cpu = bench(false, frames, &font);
    let gpu = bench(true, frames, &font);
    println!("representative animated frame, {frames} frames each (steady-state):");
    println!("  CPU shapes (tiny-skia): {cpu:.3} ms/frame");
    println!("  GPU shapes (quads+segments): {gpu:.3} ms/frame");
    println!("  CPU-side rasterization cost: {:.1}x lower on GPU path", cpu / gpu);
    println!("  (excludes CPU-mode's full-frame texture upload — real gap is larger)");
}
