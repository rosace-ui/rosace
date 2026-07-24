//! Template diffing (D103 / D102 Tier 1 — rollout step 4): decide whether an
//! edited template can be HOT-SWAPPED as data, or must ESCALATE to a rebuild.
//!
//! This is the gate the whole universal-reload story rests on. The running
//! binary produces, each frame, a fixed hole array `[h0, h1, …]` whose TYPES are
//! frozen at compile time. A swap re-inflates a NEW descriptor with those SAME
//! compiled holes — so it is only safe if every hole still feeds a slot of the
//! same type. [`diff`] enforces exactly that.
//!
//! # The safety rule (the locked per-slot check — see `.steering/HOT_RELOAD.md`)
//! A hole's "type site" is the `(widget kind, prop name)` it fills — that pair
//! determines the setter, hence the type the compiled value must be. A swap is
//! safe only when, for EVERY hole index, that site is unchanged between the
//! running template and the edited one. Then:
//! - static-only edits (retext / restyle / wrap / add-remove static elements)
//!   → [`TemplateDiff::Swappable`];
//! - a hole added/removed → count changed → [`EscalationReason::HoleCountChanged`];
//! - a hole retargeted to a different `(widget, prop)` → its compiled type may
//!   no longer fit → [`EscalationReason::HoleSlotRetargeted`].
//!
//! `hole_count` alone is NOT enough (a String slot could become an f32 slot at
//! the same count) — the per-slot site comparison is what makes this sound.
//!
//! # Known limitation (positional binding)
//! Because holes bind by INDEX, this guarantees TYPE safety, not value identity
//! across a reorder of two SAME-typed holes (e.g. swapping two `Button`
//! `on_press` slots): no crash, but the values could bind to the swapped
//! position until the next real recompile. Name-based hole binding (deferred,
//! see D125) removes that caveat.

use std::collections::BTreeMap;

use super::{PropValue, Template, TemplateNode};

/// The verdict for one edited `view!` site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateDiff {
    /// Byte-identical shape — nothing to do.
    Unchanged,
    /// Shape changed but every hole's type site is preserved: re-inflate the
    /// new descriptor with the running binary's current holes.
    Swappable,
    /// The change touched compiled logic — escalate to Tier 2 (dylib swap) or
    /// Tier 0 (restart). Never inflate.
    Escalate(EscalationReason),
}

/// Why a diff cannot be hot-swapped as data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EscalationReason {
    /// Diffing two different `view!` sites — a caller bug, not a real edit.
    KeyMismatch,
    /// A hole was added or removed (new/removed compiled `{expr}`).
    HoleCountChanged { old: usize, new: usize },
    /// Hole `index` now feeds a different `(widget, prop)` than the running
    /// binary compiled it for — its type may not match. The per-slot guard.
    HoleSlotRetargeted { index: usize },
}

/// Diff the currently-running template (`old`) against an edited one (`new`).
pub fn diff(old: &Template, new: &Template) -> TemplateDiff {
    if old.key != new.key {
        return TemplateDiff::Escalate(EscalationReason::KeyMismatch);
    }
    if old.root == new.root {
        return TemplateDiff::Unchanged;
    }
    if old.hole_count != new.hole_count {
        return TemplateDiff::Escalate(EscalationReason::HoleCountChanged {
            old: old.hole_count,
            new: new.hole_count,
        });
    }

    // Per-slot type-site check: every hole must still feed the same
    // (widget kind, prop name) so the compiled value's type still fits.
    let old_slots = hole_slots(old);
    let new_slots = hole_slots(new);
    for (index, old_site) in &old_slots {
        if new_slots.get(index) != Some(old_site) {
            return TemplateDiff::Escalate(EscalationReason::HoleSlotRetargeted { index: *index });
        }
    }

    TemplateDiff::Swappable
}

/// Map each hole index → the `(widget kind, prop name)` it feeds. Walk order
/// matches the macro's hole-indexing (props before children), but the map is
/// keyed by the recorded index, so ordering never matters for the comparison.
fn hole_slots(t: &Template) -> BTreeMap<usize, (String, String)> {
    let mut slots = BTreeMap::new();
    collect_slots(&t.root, &mut slots);
    slots
}

fn collect_slots(node: &TemplateNode, slots: &mut BTreeMap<usize, (String, String)>) {
    // Positional constructor-arg holes: type-site keyed by position.
    for (pos, value) in node.args.iter().enumerate() {
        if let PropValue::Hole(i) = value {
            slots.insert(*i, (node.widget.clone(), format!("$arg{pos}")));
        }
    }
    for (prop, value) in &node.props {
        if let PropValue::Hole(i) = value {
            slots.insert(*i, (node.widget.clone(), prop.clone()));
        }
    }
    for child in &node.children {
        collect_slots(child, slots);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::{StaticValue, TemplateKey, TemplateNode};

    fn key() -> TemplateKey {
        TemplateKey::new("src/app.rs", 10, 5)
    }
    fn t(root: TemplateNode) -> Template {
        Template::new(key(), root)
    }

    #[test]
    fn identical_templates_are_unchanged() {
        let a = t(TemplateNode::new("Column").with_static("spacing", StaticValue::Float(8.0)));
        let b = t(TemplateNode::new("Column").with_static("spacing", StaticValue::Float(8.0)));
        assert_eq!(diff(&a, &b), TemplateDiff::Unchanged);
    }

    #[test]
    fn changing_a_static_literal_is_swappable() {
        // retext: a pure data edit, no holes touched.
        let a = t(TemplateNode::new("Text").with_static("content", StaticValue::Str("Save".into())));
        let b = t(TemplateNode::new("Text").with_static("content", StaticValue::Str("Store".into())));
        assert_eq!(diff(&a, &b), TemplateDiff::Swappable);
    }

    #[test]
    fn wrapping_in_a_static_container_preserves_hole_sites_and_is_swappable() {
        // Hole 0 = (Text, content) in both, just nested deeper.
        let a = t(TemplateNode::new("Column").with_child(TemplateNode::new("Text").with_hole("content", 0)));
        let b = t(TemplateNode::new("Column")
            .with_child(TemplateNode::new("Container").with_child(TemplateNode::new("Text").with_hole("content", 0))));
        assert_eq!(diff(&a, &b), TemplateDiff::Swappable);
    }

    #[test]
    fn adding_a_hole_escalates_on_count() {
        let a = t(TemplateNode::new("Column").with_hole("spacing", 0));
        let b = t(TemplateNode::new("Column")
            .with_hole("spacing", 0)
            .with_child(TemplateNode::new("Text").with_hole("content", 1)));
        assert_eq!(
            diff(&a, &b),
            TemplateDiff::Escalate(EscalationReason::HoleCountChanged { old: 1, new: 2 })
        );
    }

    #[test]
    fn removing_a_hole_escalates_on_count() {
        let a = t(TemplateNode::new("Column").with_hole("spacing", 0).with_hole("cross", 1));
        let b = t(TemplateNode::new("Column").with_hole("spacing", 0));
        assert_eq!(
            diff(&a, &b),
            TemplateDiff::Escalate(EscalationReason::HoleCountChanged { old: 2, new: 1 })
        );
    }

    #[test]
    fn retargeting_a_hole_to_a_different_prop_escalates_even_at_same_count() {
        // Hole 0 moves from Column.spacing (f32) to Text.content (String):
        // the compiled value's type would no longer fit → must escalate.
        let a = t(TemplateNode::new("Row")
            .with_child(TemplateNode::new("Column").with_hole("spacing", 0))
            .with_child(TemplateNode::new("Text").with_static("content", StaticValue::Str("x".into()))));
        let b = t(TemplateNode::new("Row")
            .with_child(TemplateNode::new("Column").with_static("spacing", StaticValue::Float(5.0)))
            .with_child(TemplateNode::new("Text").with_hole("content", 0)));
        assert_eq!(
            diff(&a, &b),
            TemplateDiff::Escalate(EscalationReason::HoleSlotRetargeted { index: 0 })
        );
    }

    #[test]
    fn same_count_same_sites_but_reordered_static_neighbours_is_swappable() {
        // Adding a static sibling before the held Text shifts nothing about
        // hole 0's site (still Text.content) → swappable.
        let a = t(TemplateNode::new("Column").with_child(TemplateNode::new("Text").with_hole("content", 0)));
        let b = t(TemplateNode::new("Column")
            .with_child(TemplateNode::new("Text").with_static("content", StaticValue::Str("header".into())))
            .with_child(TemplateNode::new("Text").with_hole("content", 0)));
        assert_eq!(diff(&a, &b), TemplateDiff::Swappable);
    }

    #[test]
    fn different_site_keys_are_a_caller_bug() {
        let a = Template::new(TemplateKey::new("src/a.rs", 1, 1), TemplateNode::new("Column"));
        let b = Template::new(TemplateKey::new("src/b.rs", 2, 2), TemplateNode::new("Column"));
        assert_eq!(diff(&a, &b), TemplateDiff::Escalate(EscalationReason::KeyMismatch));
    }
}
