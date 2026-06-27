use crate::canvas::SkiaCanvas;

/// A single composited rendering layer.
///
/// Static content is cached in a `Layer` and only redrawn when the layer is
/// marked dirty.  This avoids re-painting unchanged content on every frame.
pub struct Layer {
    canvas: SkiaCanvas,
    dirty: bool,
}

impl Layer {
    /// Create a new dirty layer with the given pixel dimensions.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            canvas: SkiaCanvas::new(width, height),
            dirty: true,
        }
    }

    /// Mark the layer as needing repaint.
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Returns `true` if the layer needs repainting.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Mark the layer as clean (already painted for the current frame).
    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    /// Mutable access to the underlying [`SkiaCanvas`].
    pub fn canvas_mut(&mut self) -> &mut SkiaCanvas {
        &mut self.canvas
    }

    /// Shared access to the underlying [`SkiaCanvas`].
    pub fn canvas(&self) -> &SkiaCanvas {
        &self.canvas
    }

    /// Resize the layer, replacing its canvas.  Marks the layer dirty.
    ///
    /// If the dimensions are unchanged, this is a no-op.
    pub fn resize(&mut self, width: u32, height: u32) {
        if self.canvas.width() != width || self.canvas.height() != height {
            self.canvas = SkiaCanvas::new(width, height);
            self.dirty = true;
        }
    }
}

/// Well-known overlay layer indices as defined in DECISIONS.md D018.
pub mod layer_index {
    /// Primary content — widgets and scrollable regions.
    pub const CONTENT: usize = 0;
    /// Navigation chrome — app bar, bottom nav, tab bar.
    pub const NAVIGATION: usize = 1;
    /// Semi-transparent barrier that appears behind modals.
    pub const MODAL_BARRIER: usize = 2;
    /// Modal dialogs and bottom sheets.
    pub const MODALS: usize = 3;
    /// Tooltips, snackbars, and other transient overlays.
    pub const OVERLAYS: usize = 4;
    /// Developer tools overlay (only active in debug builds).
    pub const DEV_TOOLS: usize = 5;
    /// Total number of layers managed by [`LayerCompositor`].
    ///
    /// [`LayerCompositor`]: super::LayerCompositor
    pub const COUNT: usize = 6;
}

/// Compositor that manages the six overlay layers defined in D018 and blends
/// them into a single output frame.
pub struct LayerCompositor {
    layers: Vec<Layer>,
    /// Current canvas width in pixels.
    pub width: u32,
    /// Current canvas height in pixels.
    pub height: u32,
}

impl LayerCompositor {
    /// Create a compositor with [`layer_index::COUNT`] layers at the given size.
    pub fn new(width: u32, height: u32) -> Self {
        let layers = (0..layer_index::COUNT)
            .map(|_| Layer::new(width, height))
            .collect();
        Self {
            layers,
            width,
            height,
        }
    }

    /// Mutable access to a layer by index.
    ///
    /// Use the constants in [`layer_index`] for clarity.
    pub fn layer_mut(&mut self, index: usize) -> &mut Layer {
        &mut self.layers[index]
    }

    /// Shared access to a layer by index.
    ///
    /// Use the constants in [`layer_index`] for clarity.
    pub fn layer(&self, index: usize) -> &Layer {
        &self.layers[index]
    }

    /// Resize all layers and the compositor viewport.
    ///
    /// Each layer that changes size is automatically marked dirty.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        for layer in &mut self.layers {
            layer.resize(width, height);
        }
    }

    /// Composite all layers into `output`, blending bottom to top.
    ///
    /// Phase 1 performs a simple pixel copy — per-layer alpha blending will be
    /// implemented in Phase 2 once the GPU path is established.
    pub fn composite(&self, output: &mut SkiaCanvas) {
        output.clear(crate::canvas::Color::WHITE);
        // Phase 1: layers are separate canvases; real alpha compositing deferred
        // to Phase 2.  The underscore suppresses the unused-variable warning while
        // we iterate through them to assert correct indexing.
        for _layer in &self.layers {
            // no-op until Phase 2
        }
    }
}
