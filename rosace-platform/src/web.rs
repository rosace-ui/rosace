//! Web (WASM) platform backend for ROSACE.
//!
//! Activated automatically when compiled for `wasm32-unknown-unknown`.
//! Uses a `<canvas id="rosace-canvas">` element in the host page.

#[cfg(target_arch = "wasm32")]
pub mod web_app {
    use wasm_bindgen::prelude::*;
    use web_sys::{window, CanvasRenderingContext2d, HtmlCanvasElement, ImageData};

    use rosace_render::canvas::SkiaCanvas;

    use crate::event::InputEvent;

    /// WASM entry point — call this from your `#[wasm_bindgen(start)]` fn.
    ///
    /// Finds the `<canvas id="rosace-canvas">` element, sets its dimensions,
    /// invokes `paint_fn` once to render the initial frame, and blits the
    /// pixel buffer onto the canvas via `putImageData`.
    pub fn run_web<F>(width: u32, height: u32, mut paint_fn: F)
    where
        F: FnMut(&mut SkiaCanvas, &[InputEvent]) + 'static,
    {
        console_error_panic_hook::set_once();

        let window = window().expect("no global window");
        let document = window.document().expect("no document on window");
        let canvas = document
            .get_element_by_id("rosace-canvas")
            .expect("no #rosace-canvas element in page")
            .dyn_into::<HtmlCanvasElement>()
            .expect("#rosace-canvas is not a <canvas> element");

        canvas.set_width(width);
        canvas.set_height(height);

        let ctx = canvas
            .get_context("2d")
            .unwrap()
            .unwrap()
            .dyn_into::<CanvasRenderingContext2d>()
            .unwrap();

        // Single-frame render (MVP — paint once on startup).
        // A full requestAnimationFrame loop requires Rc<RefCell<>> state
        // sharing; that is deferred to a later phase.
        let mut tez_canvas = SkiaCanvas::new(width, height);
        paint_fn(&mut tez_canvas, &[]);

        let pixels = tez_canvas.pixels();
        // `pixels()` returns pre-multiplied BGRA from tiny-skia; we need RGBA.
        // Swap R and B channels in-place before handing to the browser.
        let mut rgba = pixels.to_vec();
        for chunk in rgba.chunks_exact_mut(4) {
            chunk.swap(0, 2); // B <-> R
        }

        let data = wasm_bindgen::Clamped(rgba.as_slice());
        if let Ok(image_data) =
            ImageData::new_with_u8_clamped_array_and_sh(data, width, height)
        {
            let _ = ctx.put_image_data(&image_data, 0.0, 0.0);
        }
    }
}
