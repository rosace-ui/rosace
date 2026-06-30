use tezzera_core::types::Size;
use super::{Widget, LayoutCtx, PaintCtx, BoxedWidget, avail_w, avail_h};

/// Z-axis overlay — all children drawn at the same position (back-to-front).
pub struct Stack {
    children: Vec<BoxedWidget>,
    fit: StackFit,
}

/// How a Stack sizes itself relative to its children.
#[derive(Debug, Clone, Copy, Default)]
pub enum StackFit {
    /// Size to the largest child.
    #[default]
    Loose,
    /// Expand to fill all available space.
    Expand,
}

impl Stack {
    pub fn new() -> Self { Self { children: Vec::new(), fit: StackFit::Loose } }
    pub fn fit(mut self, f: StackFit) -> Self { self.fit = f; self }
    pub fn child(mut self, w: impl Widget + 'static) -> Self {
        self.children.push(Box::new(w)); self
    }
}

impl Default for Stack {
    fn default() -> Self { Self::new() }
}

impl Widget for Stack {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let constraints = ctx.constraints;
        match self.fit {
            StackFit::Expand => Size {
                width: avail_w(constraints),
                height: avail_h(constraints),
            },
            StackFit::Loose => {
                let mut max_w = 0.0_f32;
                let mut max_h = 0.0_f32;
                for child in &self.children {
                    let s = child.layout(ctx);
                    max_w = max_w.max(s.width);
                    max_h = max_h.max(s.height);
                }
                Size { width: max_w, height: max_h }
            }
        }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        for child in &self.children {
            child.paint(&mut ctx.child(ctx.rect));
        }
    }
}
