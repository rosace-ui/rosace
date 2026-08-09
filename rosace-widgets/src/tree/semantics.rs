//! The `Semantics` widget — explicit accessibility annotation for any subtree.
//!
//! Built-in widgets declare their own role and label (see
//! `WIDGET_QUALITY_BAR.md` §5), so most apps never need this. It exists for
//! the cases the widget itself cannot know:
//!
//! * **Custom-painted content.** A `CustomPaint`/`ShaderPaint` chart is a
//!   rectangle of pixels to the framework; only the app knows it is
//!   "Revenue, up 12% this quarter".
//! * **Composites that read as one control.** An icon plus a label plus a
//!   tap target is three nodes to the tree but one button to a user.
//! * **Silencing decoration.** [`Semantics::exclude`] removes a subtree
//!   entirely, so a purely ornamental flourish — or an icon that merely
//!   repeats adjacent text — stops being announced twice.
//!
//! Mirrors Flutter's `Semantics` / `ExcludeSemantics` pair, collapsed into one
//! widget because the two never usefully combine: an excluded subtree has no
//! semantics to annotate.

use rosace_core::Role;

use super::{Children, PaintCtx, SemanticsProps, Widget};

/// Annotates a subtree with an explicit accessibility role and label, or
/// removes it from the accessibility tree entirely.
///
/// ```rust,ignore
/// // Give a hand-painted chart a meaning
/// Semantics::new(my_chart)
///     .role(Role::Image)
///     .label("Revenue, up 12% this quarter")
///
/// // Silence a decorative flourish
/// Semantics::new(sparkle).exclude()
/// ```
///
/// Layout and painting are pass-through: this widget adds no box, no padding
/// and no extra tree depth to the visual result — it only annotates.
pub struct Semantics<W: Widget> {
    child: W,
    role: Role,
    label: Option<String>,
    value: Option<String>,
    heading_level: Option<u8>,
    href: Option<String>,
    excluded: bool,
}

impl<W: Widget> Semantics<W> {
    /// Wraps `child`. Without a further call this declares a `Role::Unknown`
    /// node with no label — harmless, but you almost certainly want at least
    /// [`label`](Self::label) or [`exclude`](Self::exclude).
    pub fn new(child: W) -> Self {
        Self {
            child,
            role: Role::Unknown,
            label: None,
            value: None,
            heading_level: None,
            href: None,
            excluded: false,
        }
    }

    /// The kind of thing this is — button, image, heading, …
    pub fn role(mut self, role: Role) -> Self {
        self.role = role;
        self
    }

    /// The accessible **name**: what this control *is* ("Save", "Search").
    /// Not its contents — see [`value`](Self::value).
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// The current **content or state**: a field's typed text, a slider's
    /// number. Distinct from [`label`](Self::label), and screen readers
    /// announce them differently — a text field is "Email, edit text,
    /// ada@example.com", not "ada@example.com, edit text".
    pub fn value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }

    /// Heading depth, `1..=6`. Only meaningful with [`Role::Heading`]; it is
    /// what lets screen-reader users jump by heading level, and what maps to
    /// `<h1>`–`<h6>` in the web/SEO output.
    pub fn heading_level(mut self, level: u8) -> Self {
        self.heading_level = Some(level);
        self
    }

    /// Link target. Only meaningful with [`Role::Link`].
    pub fn href(mut self, href: impl Into<String>) -> Self {
        self.href = Some(href.into());
        self
    }

    /// Removes this subtree from the accessibility tree.
    ///
    /// Wins over any role/label set on the same widget — an excluded subtree
    /// is silent by definition, so annotating it would be contradictory. The
    /// child is still laid out and painted exactly as before; only its
    /// *announcement* disappears.
    pub fn exclude(mut self) -> Self {
        self.excluded = true;
        self
    }
}

impl<W: Widget + 'static> Widget for Semantics<W> {
    fn children(&self) -> Children<'_> {
        Children::One(&self.child)
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        if self.excluded {
            ctx.exclude_semantics();
        } else {
            let mut s = SemanticsProps::new(self.role.clone());
            if let Some(l) = &self.label {
                s = s.label(l.clone());
            }
            if let Some(v) = &self.value {
                s = s.value(v.clone());
            }
            if let Some(lvl) = self.heading_level {
                s = s.heading_level(lvl);
            }
            if let Some(h) = &self.href {
                s = s.href(h.clone());
            }
            ctx.semantics(s);
        }
        // Pass-through paint — annotation must not change what is drawn.
        let r = ctx.rect;
        self.child.paint(&mut ctx.child(r));
    }
}
