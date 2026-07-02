use tezzera_core::types::{Point, Rect, Size};
use tezzera_layout::Constraints;
use tezzera_render::{Color, FontCache, PictureRecorder, SkiaCanvas};
use tezzera_theme::{ThemeData, built_in};
use super::{Widget, LayoutCtx, PaintCtx};

/// Off-screen widget renderer — renders a widget tree to a [`SkiaCanvas`] or PNG bytes.
///
/// Use this for **golden / snapshot tests**: render a widget to PNG, save it, and
/// compare future runs byte-for-byte to catch visual regressions.
///
/// For a real windowed app, use `tezzera::App` from the umbrella crate instead.
///
/// ```rust,ignore
/// // Render to PNG bytes (dark theme by default):
/// let png = WidgetApp::new(800, 600).render_png(&root);
///
/// // Light theme:
/// let png = WidgetApp::new(800, 600).light().render_png(&root);
///
/// // Custom theme:
/// let png = WidgetApp::new(800, 600).theme(my_theme).render_png(&root);
/// ```
pub struct WidgetApp {
    pub width: u32,
    pub height: u32,
    /// `None` = derive from `theme.colors.background`; `Some` = explicit override.
    background: Option<Color>,
    pub font: FontCache,
    pub theme: ThemeData,
}

impl WidgetApp {
    /// Create a new app with the default dark theme.
    pub fn new(width: u32, height: u32) -> Self {
        let font = FontCache::system_ui()
            .or_else(FontCache::system_mono)
            .expect("no system font found");
        Self {
            width,
            height,
            background: None,
            font,
            theme: built_in::dark_theme(),
        }
    }

    /// Override the background color explicitly (ignores theme background).
    pub fn background(mut self, c: Color) -> Self { self.background = Some(c); self }

    /// Replace the font.
    pub fn with_font(mut self, f: FontCache) -> Self { self.font = f; self }

    /// Set a fully custom theme.
    pub fn theme(mut self, t: ThemeData) -> Self { self.theme = t; self }

    /// Use the built-in dark theme (this is already the default).
    pub fn dark(mut self) -> Self { self.theme = built_in::dark_theme(); self }

    /// Use the built-in light theme.
    pub fn light(mut self) -> Self { self.theme = built_in::light_theme(); self }

    fn resolve_bg(&self) -> Color {
        self.background.unwrap_or_else(|| {
            let c = self.theme.colors.background;
            Color::rgba(
                (c.r * 255.0) as u8,
                (c.g * 255.0) as u8,
                (c.b * 255.0) as u8,
                (c.a * 255.0) as u8,
            )
        })
    }

    /// Render the widget tree to a [`SkiaCanvas`].
    pub fn render(&self, root: &dyn Widget) -> SkiaCanvas {
        tezzera_theme::set_theme(self.theme.clone());

        let mut canvas = SkiaCanvas::new(self.width, self.height);
        canvas.clear(self.resolve_bg());

        let constraints = Constraints::tight(self.width as f32, self.height as f32);
        let lctx = LayoutCtx::new(constraints, &self.font, &self.theme);
        let size = root.layout(&lctx);

        let mut recorder = PictureRecorder::new();
        let mut ctx = PaintCtx {
            recorder: &mut recorder,
            rect: Rect {
                origin: Point { x: 0.0, y: 0.0 },
                size: Size {
                    width:  size.width.min(self.width as f32),
                    height: size.height.min(self.height as f32),
                },
            },
            font: &self.font,
            theme: self.theme.clone(),
            hit_targets: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
            focus_nodes: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
            transform_entries: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
            clip_rect: None,
        };

        root.paint(&mut ctx);
        let picture = recorder.finish();
        canvas.play_picture(&picture, &self.font);
        canvas
    }

    /// Paint the widget tree onto an existing canvas (for windowed apps).
    pub fn render_into(&self, canvas: &mut SkiaCanvas, root: &dyn Widget) {
        tezzera_theme::set_theme(self.theme.clone());
        canvas.clear(self.resolve_bg());

        let w = canvas.width() as f32;
        let h = canvas.height() as f32;
        let constraints = Constraints::tight(w, h);
        let lctx = LayoutCtx::new(constraints, &self.font, &self.theme);
        let size = root.layout(&lctx);

        let mut recorder = PictureRecorder::new();
        let mut ctx = PaintCtx {
            recorder: &mut recorder,
            rect: Rect {
                origin: Point { x: 0.0, y: 0.0 },
                size: Size {
                    width:  size.width.min(w),
                    height: size.height.min(h),
                },
            },
            font: &self.font,
            theme: self.theme.clone(),
            hit_targets: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
            focus_nodes: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
            transform_entries: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
            clip_rect: None,
        };
        root.paint(&mut ctx);
        let picture = recorder.finish();
        canvas.play_picture(&picture, &self.font);
    }

    /// Render and encode to PNG bytes (convenience).
    pub fn render_png(&self, root: &dyn Widget) -> Vec<u8> {
        self.render(root)
            .encode_png()
            .expect("png encode failed")
    }
}
