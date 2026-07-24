//! The load-bearing Tier 1 invariant (D103 / D102 — rollout steps 2+3 closed):
//! under `rsc-hot`, a real `view!` builds its widget by **inflating** its own
//! template descriptor with its compiled hole values, and the result lays out
//! IDENTICALLY to the equivalent hand-written builder tree. This is the guard
//! Option A requires — dev (interpreter) path == release (builder) path.
//!
//! Run: `cargo test -p rosace --features rsc-hot`. Without the feature `view!`
//! is pure builder calls, so this file compiles out.
#![cfg(feature = "rsc-hot")]

use rosace::prelude::*;
use rosace::widgets::template;
use rosace::widgets::tree::{LayoutCtx, Widget};
use rosace::widgets::{PropValue, StaticValue, TemplateKey};
use rosace::{Constraints, FontCache};

/// Lay a widget out under a shared headless context → its measured size.
fn measure(w: &dyn Widget) -> rosace::prelude::Size {
    let font = FontCache::embedded();
    let theme = rosace::prelude::dark_theme();
    let ctx = LayoutCtx::new(Constraints::loose(400.0, 400.0), &font, &theme);
    w.layout(&ctx)
}

#[test]
fn view_inflates_to_the_same_tree_the_builder_would_produce() {
    // Runtime values → these become HOLES; the literals stay static data.
    let gap: f32 = 12.0;
    let title = String::from("Hello");

    // The hand-written builder tree, with the SAME values. Built first because
    // the dev `view!` moves `title` into its hole-filler array.
    let built = Column::new()
        .spacing(gap)
        .child(Text::new(title.clone()))
        .child(Text::new("static footer"));

    // Under `rsc-hot`, this `view!` is: build descriptor → materialise holes
    // [gap, title] → inflate. `_inflated` is the interpreter's output.
    let inflated = view! {
        Column {
            spacing: gap
            Text(title)
            Text("static footer")
        }
    };

    // THE invariant: the inflated (dev/interpreter) tree measures identically
    // to the builder (release) tree.
    assert_eq!(
        measure(&*inflated),
        measure(&built),
        "inflated view! must lay out identically to the builder equivalent"
    );
}

#[test]
fn view_registers_the_expected_descriptor_shape() {
    let gap: f32 = 8.0;
    let name = String::from("hi");
    let _w = view! {
        Column {
            spacing: gap
            Text(name)
        }
    };

    // Both `spacing: gap` and `content: name` are runtime variables → 2 holes.
    let snap = template::snapshot();
    let t = snap
        .iter()
        .find(|t| {
            t.root.widget == "Column"
                && t.hole_count == 2
                && t.key.file.ends_with("view_template.rs")
        })
        .expect("view! did not register the expected Column template");

    // Root spacing is a HOLE (runtime `gap`), not static.
    assert!(
        matches!(t.root.props.first(), Some((k, PropValue::Hole(0))) if k == "spacing"),
        "root spacing should be hole #0: {:?}",
        t.root.props
    );
    // The Text child's content is a positional ARG hole (index 1 — after the
    // Column's spacing hole 0).
    let child = &t.root.children[0];
    assert_eq!(child.widget, "Text");
    assert!(
        matches!(child.args.first(), Some(PropValue::Hole(1))),
        "text content should be arg hole #1: {:?}",
        child.args
    );
    assert!(t.key.file.ends_with("view_template.rs"));

    // Guard against an accidental static-vs-hole regression: a literal prop
    // really does travel as data.
    let _s = view! { Column { spacing: 4.0 } };
    let has_static_spacing = template::snapshot().iter().any(|t| {
        t.root.widget == "Column"
            && matches!(t.root.props.first(), Some((_, PropValue::Static(StaticValue::Float(f)))) if *f == 4.0)
    });
    assert!(has_static_spacing, "a literal spacing must register as a static");
}

/// THE live-swap payoff: an edit applied via `apply_swap` changes what the SAME
/// `view!` site renders on its next rebuild — no recompile. The source text
/// never changes; only the registered descriptor does.
#[test]
fn a_hot_swap_changes_what_the_same_view_site_renders() {
    // One `view!` site, rendered via a fn we can call twice (same file+line →
    // same TemplateKey each call).
    fn build() -> Box<dyn Widget> {
        view! {
            Column {
                spacing: 4.0
                Text("a")
                Text("b")
            }
        }
    }

    let before = build(); // registers the baseline (spacing 4.0) and inflates it

    // The site's key, from its registered baseline.
    let key = template::snapshot()
        .into_iter()
        .find(|t| {
            t.root.widget == "Column"
                && t.root.children.len() == 2
                && matches!(t.root.props.first(), Some((k, PropValue::Static(StaticValue::Float(f)))) if k == "spacing" && *f == 4.0)
        })
        .expect("baseline registered")
        .key;

    // Simulate a hot edit: spacing 4.0 → 40.0 (a safe static swap).
    let edited = template::parse_template(
        "Column { spacing: 40.0 Text(\"a\") Text(\"b\") }",
        key,
    )
    .unwrap();
    assert_eq!(template::apply_swap(edited), template::SwapOutcome::Applied);

    // Re-render the SAME site — its source still says 4.0, but it now inflates
    // the swapped descriptor (spacing 40.0).
    let after = build();

    let expect_40 = Column::new().spacing(40.0).child(Text::new("a")).child(Text::new("b"));
    assert_eq!(measure(&*after), measure(&expect_40), "swap should change the rendered layout");
    assert_ne!(measure(&*after), measure(&*before), "layout must differ from before the swap");
}

/// THE no-divergence proof: the runtime parser (reading edited source text) and
/// the compile-time macro produce the SAME template for the same `view!` —
/// because they share one grammar. A hot-swap depends on this holding.
#[test]
fn runtime_parser_matches_the_compile_time_macro() {
    let g: f32 = 3.0;
    let t = String::from("hey");

    // Compile-time path: this `view!` registers its descriptor (a Row shape,
    // distinct from the Columns other tests register).
    let _w = view! {
        Row {
            spacing: g
            Text(t)
        }
    };

    // Runtime path: parse the identical `view!` body from source text.
    let parsed = template::parse_template(
        "Row { spacing: g Text(t) }",
        TemplateKey::new("runtime", 1, 1),
    )
    .expect("runtime parse");

    // Some macro-registered template's shape equals the runtime-parsed shape.
    let matched = template::snapshot().into_iter().any(|reg| reg.root == parsed.root);
    assert!(
        matched,
        "runtime parser and compile-time macro must produce identical templates"
    );
}
