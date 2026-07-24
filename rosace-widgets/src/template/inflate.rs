//! The widget registry + interpreter (D103 / D102 Tier 1 — rollout step 3).
//!
//! The "inflater, not a renderer" (see `.steering/HOT_RELOAD.md`): [`inflate`]
//! walks a [`Template`] and reconstructs a `Box<dyn Widget>` tree by calling the
//! SAME widget constructors the release builder would — so the result is
//! byte-for-byte the tree hand-written builder code produces, and the engine's
//! normal `layout()`/`paint()` run on it unchanged. Nothing here paints; this
//! only changes how the tree is *constructed* (from data instead of compiled
//! calls).
//!
//! The one genuinely new runtime piece is the **widget registry**: a map from a
//! widget's string name to a build closure that knows how to construct it from
//! resolved props + children. Built-ins seed it; third-party widgets register
//! the same way via [`register_widget`] (the D115 icon-registry / D124
//! material-registry extensibility bar).
//!
//! # Holes
//! Dynamic `{expr}` slots are supplied positionally as `&[Box<dyn Any>]` — the
//! compiled values the running binary produces each frame. A build closure
//! downcasts a hole to the type its prop needs. Value holes (numbers, strings)
//! work today; **handler/closure holes** (`on_press`) need a typed
//! handler-registry and are a named deferral (they tie into the SDUI
//! name-binding note in D125).
//!
//! Trace: per the widget-layer convention (widgets don't emit `RosaceTrace`
//! themselves — that's the engine's job), inflate instrumentation attaches when
//! this is wired into the frame loop / hot-swap, not in this pure function.

use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use super::{StaticValue, PropValue, Template, TemplateNode};
use crate::tree::{Button, Column, Row, Text, Widget};

/// A nullary event handler (e.g. `Button::on_press`). Handlers travel through a
/// hole wrapped as this type — concrete (so it round-trips through `Box<dyn
/// Any>`), and callable. Arg-taking handlers (`Fn(T)`) are a future extension.
pub type Handler = Arc<dyn Fn() + Send + Sync>;

/// A resolved prop value handed to a build closure: either a template literal,
/// or the compiled value at a hole slot (type-erased).
pub enum PropInput<'a> {
    Static(&'a StaticValue),
    Hole(&'a dyn Any),
}

/// Register a widget with the interpreter WITHOUT hand-writing the build
/// closure — the ergonomic front door to hot-reload extensibility.
///
/// You give the widget's registry name, its zero-arg constructor, whether it
/// takes children, and a `"prop" => setter: Type` table. Each entry maps a
/// template prop to a builder method + the type to extract (via [`FromProp`]).
/// This is the boilerplate the future `#[derive(Widget)]` would generate; it is
/// also the single place a widget's prop schema is declared (the same data a
/// tooling/IDE schema would read).
///
/// ```ignore
/// inflatable!("Column", Column::new(), children, {
///     "spacing" => spacing: f32,
/// });
/// inflatable!("Text", Text::new(""), leaf, {});   // leaf: no children
/// ```
#[macro_export]
macro_rules! inflatable {
    // Container form: props + `.child(..)` children. (Setter-style widgets;
    // positional constructor args aren't handled by this form — `_args`.)
    ($name:literal, $ctor:expr, children, { $($prop:literal => $setter:ident : $ty:ty),* $(,)? }) => {
        $crate::template::register_widget($name, |_args: &[$crate::template::PropInput], props, children| {
            let mut w = $ctor;
            for (k, v) in props {
                // `v` legitimately goes unused when the prop table is empty.
                let _ = &v;
                match k.as_str() {
                    $( $prop => w = w.$setter(<$ty as $crate::template::FromProp>::from_prop(v, $name, $prop)?), )*
                    _ => return ::core::result::Result::Err(
                        $crate::template::InflateError::UnknownProp { widget: $name.into(), prop: k.clone() }
                    ),
                }
            }
            for kid in children { w = w.child(kid); }
            let _ = &mut w; // `mut` may go unused for a propless, childless widget.
            ::core::result::Result::Ok(::std::boxed::Box::new(w) as ::std::boxed::Box<dyn $crate::tree::Widget>)
        });
    };
    // Leaf form: props only, no children (given children → escalate).
    ($name:literal, $ctor:expr, leaf, { $($prop:literal => $setter:ident : $ty:ty),* $(,)? }) => {
        $crate::template::register_widget($name, |_args: &[$crate::template::PropInput], props, children: ::std::vec::Vec<::std::boxed::Box<dyn $crate::tree::Widget>>| {
            if !children.is_empty() {
                return ::core::result::Result::Err(
                    $crate::template::InflateError::UnexpectedChildren { widget: $name.into() }
                );
            }
            let mut w = $ctor;
            for (k, v) in props {
                let _ = &v;
                match k.as_str() {
                    $( $prop => w = w.$setter(<$ty as $crate::template::FromProp>::from_prop(v, $name, $prop)?), )*
                    _ => return ::core::result::Result::Err(
                        $crate::template::InflateError::UnknownProp { widget: $name.into(), prop: k.clone() }
                    ),
                }
            }
            let _ = &mut w;
            ::core::result::Result::Ok(::std::boxed::Box::new(w) as ::std::boxed::Box<dyn $crate::tree::Widget>)
        });
    };
}

/// Why an [`inflate`] failed. All are "escalate, don't paint garbage" cases.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InflateError {
    /// No registered widget for this name.
    UnknownWidget(String),
    /// A registered widget got a prop it doesn't understand.
    UnknownProp { widget: String, prop: String },
    /// A prop's value was the wrong type for the setter (e.g. a closure where
    /// an `f32` was expected) — the slot-signature mismatch guard in miniature.
    PropType { widget: String, prop: String, expected: &'static str },
    /// A leaf widget was given children it can't hold.
    UnexpectedChildren { widget: String },
    /// A `Hole(i)` referenced a slot past the end of the supplied hole array.
    HoleOutOfRange { index: usize, len: usize },
}

impl std::fmt::Display for InflateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InflateError::UnknownWidget(w) => write!(f, "unknown widget `{w}`"),
            InflateError::UnknownProp { widget, prop } => write!(f, "`{widget}` has no prop `{prop}`"),
            InflateError::PropType { widget, prop, expected } => {
                write!(f, "`{widget}.{prop}` expected {expected}")
            }
            InflateError::UnexpectedChildren { widget } => write!(f, "`{widget}` cannot have children"),
            InflateError::HoleOutOfRange { index, len } => {
                write!(f, "hole #{index} out of range (only {len} supplied)")
            }
        }
    }
}
impl std::error::Error for InflateError {}

/// A widget build closure: construct the widget from its resolved props and
/// already-inflated children. Higher-ranked over the props' lifetime so a
/// single boxed closure works for any call.
pub type BuildFn = Box<
    dyn for<'a> Fn(&'a [PropInput<'a>], &'a [(String, PropInput<'a>)], Vec<Box<dyn Widget>>) -> Result<Box<dyn Widget>, InflateError>
        + Send
        + Sync,
>;

fn registry() -> &'static RwLock<HashMap<String, BuildFn>> {
    static REG: OnceLock<RwLock<HashMap<String, BuildFn>>> = OnceLock::new();
    REG.get_or_init(|| RwLock::new(builtin_widgets()))
}

/// Register (or replace) a widget build closure by name. Third-party widgets
/// call this — same extensibility path as built-ins, no edit to rosace-* crates.
pub fn register_widget<F>(name: impl Into<String>, build: F)
where
    F: for<'a> Fn(&'a [PropInput<'a>], &'a [(String, PropInput<'a>)], Vec<Box<dyn Widget>>) -> Result<Box<dyn Widget>, InflateError>
        + Send
        + Sync
        + 'static,
{
    registry().write().unwrap_or_else(|e| e.into_inner()).insert(name.into(), Box::new(build));
}

/// Whether a widget name is registered (diagnostics / tests).
pub fn is_registered(name: &str) -> bool {
    registry().read().unwrap_or_else(|e| e.into_inner()).contains_key(name)
}

/// Inflate a template into a live widget tree, binding hole slots by index.
pub fn inflate(template: &Template, holes: &[Box<dyn Any>]) -> Result<Box<dyn Widget>, InflateError> {
    inflate_node(&template.root, holes)
}

/// Resolve one descriptor value to a live input: a static passes through, a
/// hole binds to the compiled value at its index (out-of-range → escalate).
fn resolve<'a>(value: &'a PropValue, holes: &'a [Box<dyn Any>]) -> Result<PropInput<'a>, InflateError> {
    match value {
        PropValue::Static(s) => Ok(PropInput::Static(s)),
        PropValue::Hole(i) => holes
            .get(*i)
            .map(|h| PropInput::Hole(h.as_ref()))
            .ok_or(InflateError::HoleOutOfRange { index: *i, len: holes.len() }),
    }
}

fn inflate_node(node: &TemplateNode, holes: &[Box<dyn Any>]) -> Result<Box<dyn Widget>, InflateError> {
    // Positional constructor args, then named props — both resolved the same way.
    let mut args: Vec<PropInput<'_>> = Vec::with_capacity(node.args.len());
    for value in &node.args {
        args.push(resolve(value, holes)?);
    }
    let mut props: Vec<(String, PropInput<'_>)> = Vec::with_capacity(node.props.len());
    for (key, value) in &node.props {
        props.push((key.clone(), resolve(value, holes)?));
    }

    // Children first (depth-first), so the build closure receives live widgets.
    let mut children: Vec<Box<dyn Widget>> = Vec::with_capacity(node.children.len());
    for child in &node.children {
        children.push(inflate_node(child, holes)?);
    }

    let reg = registry().read().unwrap_or_else(|e| e.into_inner());
    let build = reg
        .get(&node.widget)
        .ok_or_else(|| InflateError::UnknownWidget(node.widget.clone()))?;
    build(&args, &props, children)
}

// ── typed prop extraction ───────────────────────────────────────────────────

/// Convert a resolved prop ([`PropInput`]) into a setter's argument type.
///
/// This is the typed edge between the untyped template/hole world and a
/// widget's strongly-typed builder. A build closure calls
/// `f32::from_prop(v, ..)` to get the value for `.spacing(f32)`. Implement it
/// for your own prop types so `inflatable!`-registered widgets can accept them
/// (mirrors the extensibility of the widget registry itself).
pub trait FromProp: Sized {
    /// The name shown in a [`InflateError::PropType`] when extraction fails.
    const TYPE_NAME: &'static str;
    fn from_prop(pi: &PropInput, widget: &str, prop: &str) -> Result<Self, InflateError>;
}

/// Shared error constructor for a type mismatch at a slot.
fn prop_type_err<T: FromProp>(widget: &str, prop: &str) -> InflateError {
    InflateError::PropType { widget: widget.to_string(), prop: prop.to_string(), expected: T::TYPE_NAME }
}

macro_rules! impl_from_prop_number {
    ($($t:ty),+) => {$(
        impl FromProp for $t {
            const TYPE_NAME: &'static str = stringify!($t);
            fn from_prop(pi: &PropInput, widget: &str, prop: &str) -> Result<Self, InflateError> {
                match pi {
                    PropInput::Static(StaticValue::Float(f)) => Ok(*f as $t),
                    PropInput::Static(StaticValue::Int(i)) => Ok(*i as $t),
                    PropInput::Static(StaticValue::Bool(b)) => Ok(*b as i64 as $t),
                    PropInput::Hole(any) => any
                        .downcast_ref::<$t>()
                        .copied()
                        .or_else(|| any.downcast_ref::<f32>().map(|v| *v as $t))
                        .or_else(|| any.downcast_ref::<f64>().map(|v| *v as $t))
                        .or_else(|| any.downcast_ref::<i64>().map(|v| *v as $t))
                        .ok_or_else(|| prop_type_err::<$t>(widget, prop)),
                    _ => Err(prop_type_err::<$t>(widget, prop)),
                }
            }
        }
    )+};
}
impl_from_prop_number!(f32, f64, i64);

impl FromProp for bool {
    const TYPE_NAME: &'static str = "bool";
    fn from_prop(pi: &PropInput, widget: &str, prop: &str) -> Result<Self, InflateError> {
        match pi {
            PropInput::Static(StaticValue::Bool(b)) => Ok(*b),
            PropInput::Hole(any) => any.downcast_ref::<bool>().copied().ok_or_else(|| prop_type_err::<bool>(widget, prop)),
            _ => Err(prop_type_err::<bool>(widget, prop)),
        }
    }
}

impl FromProp for String {
    const TYPE_NAME: &'static str = "string";
    fn from_prop(pi: &PropInput, widget: &str, prop: &str) -> Result<Self, InflateError> {
        match pi {
            PropInput::Static(StaticValue::Str(s)) => Ok(s.clone()),
            PropInput::Hole(any) => any
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| any.downcast_ref::<&str>().map(|s| s.to_string()))
                .ok_or_else(|| prop_type_err::<String>(widget, prop)),
            _ => Err(prop_type_err::<String>(widget, prop)),
        }
    }
}

/// A handler is always a hole (a closure can't be a literal). The compiled
/// binary wraps it as [`Handler`] and puts it in the hole array; here we
/// downcast it back.
impl FromProp for Handler {
    const TYPE_NAME: &'static str = "handler";
    fn from_prop(pi: &PropInput, widget: &str, prop: &str) -> Result<Self, InflateError> {
        match pi {
            PropInput::Hole(any) => any
                .downcast_ref::<Handler>()
                .cloned()
                .ok_or_else(|| prop_type_err::<Handler>(widget, prop)),
            _ => Err(prop_type_err::<Handler>(widget, prop)),
        }
    }
}

// ── built-in widgets ────────────────────────────────────────────────────────

fn builtin_widgets() -> HashMap<String, BuildFn> {
    let mut m: HashMap<String, BuildFn> = HashMap::new();
    m.insert("Column".into(), Box::new(build_column));
    m.insert("Row".into(), Box::new(build_row));
    m.insert("Text".into(), Box::new(build_text));
    m.insert("Button".into(), Box::new(build_button));
    m
}

// Button's label is a positional arg (`Button("Save")`); `on_press` is a
// handler hole (a nullary closure the compiled binary wrapped as `Handler`).
fn build_button(args: &[PropInput], props: &[(String, PropInput)], children: Vec<Box<dyn Widget>>) -> Result<Box<dyn Widget>, InflateError> {
    if !children.is_empty() {
        return Err(InflateError::UnexpectedChildren { widget: "Button".into() });
    }
    let label = match args.first() {
        Some(a) => String::from_prop(a, "Button", "label")?,
        None => String::new(),
    };
    let mut button = Button::new(label);
    for (k, v) in props {
        match k.as_str() {
            "on_press" => {
                let handler = Handler::from_prop(v, "Button", "on_press")?;
                button = button.on_press(move || (*handler)());
            }
            _ => return Err(InflateError::UnknownProp { widget: "Button".into(), prop: k.clone() }),
        }
    }
    Ok(Box::new(button))
}

// Column/Row take no positional args (`Column::new()`); `_args` is ignored (a
// stray `Column(x)` would already fail the release builder's `Column::new(x)`).
fn build_column(_args: &[PropInput], props: &[(String, PropInput)], children: Vec<Box<dyn Widget>>) -> Result<Box<dyn Widget>, InflateError> {
    let mut col = Column::new();
    for (k, v) in props {
        match k.as_str() {
            "spacing" => col = col.spacing(f32::from_prop(v, "Column", "spacing")?),
            _ => return Err(InflateError::UnknownProp { widget: "Column".into(), prop: k.clone() }),
        }
    }
    for kid in children {
        col = col.child(kid);
    }
    Ok(Box::new(col))
}

fn build_row(_args: &[PropInput], props: &[(String, PropInput)], children: Vec<Box<dyn Widget>>) -> Result<Box<dyn Widget>, InflateError> {
    let mut row = Row::new();
    for (k, v) in props {
        match k.as_str() {
            "spacing" => row = row.spacing(f32::from_prop(v, "Row", "spacing")?),
            _ => return Err(InflateError::UnknownProp { widget: "Row".into(), prop: k.clone() }),
        }
    }
    for kid in children {
        row = row.child(kid);
    }
    Ok(Box::new(row))
}

// Text's content is a POSITIONAL constructor arg: `Text("Hi")` → `Text::new("Hi")`.
fn build_text(args: &[PropInput], props: &[(String, PropInput)], children: Vec<Box<dyn Widget>>) -> Result<Box<dyn Widget>, InflateError> {
    if !children.is_empty() {
        return Err(InflateError::UnexpectedChildren { widget: "Text".into() });
    }
    let content = match args.first() {
        Some(a) => String::from_prop(a, "Text", "text")?,
        None => String::new(),
    };
    // Text has no named-prop setters wired yet; reject unknowns rather than
    // silently drop them.
    if let Some((k, _)) = props.first() {
        return Err(InflateError::UnknownProp { widget: "Text".into(), prop: k.clone() });
    }
    Ok(Box::new(Text::new(content)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::{Template, TemplateKey, TemplateNode};
    use crate::tree::{LayoutCtx, Widget};
    use rosace_layout::Constraints;

    fn tmpl(root: TemplateNode) -> Template {
        Template::new(TemplateKey::new("src/inflate_test.rs", 1, 1), root)
    }

    /// Lay a widget out under a shared headless context → its measured size.
    fn measure(w: &dyn Widget) -> rosace_core::types::Size {
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let ctx = LayoutCtx::new(Constraints::loose(400.0, 400.0), &font, &theme);
        w.layout(&ctx)
    }

    #[test]
    fn button_inflates_with_a_handler_hole_and_matches_the_builder() {
        // A nullary handler wrapped as `Handler`, supplied via a hole.
        let handler: Handler = Arc::new(|| {});
        let t = tmpl(
            TemplateNode::new("Button")
                .with_arg_static(StaticValue::Str("Save".into()))
                .with_hole("on_press", 0),
        );
        let holes: Vec<Box<dyn Any>> = vec![Box::new(handler)];
        let inflated = inflate(&t, &holes).expect("button with a handler hole inflates");
        assert_eq!(measure(&*inflated), measure(&Button::new("Save").on_press(|| {})));
    }

    #[test]
    fn a_non_handler_value_in_a_handler_slot_escalates() {
        // A number where on_press expects a Handler → PropType, never garbage.
        let t = tmpl(
            TemplateNode::new("Button")
                .with_arg_static(StaticValue::Str("x".into()))
                .with_hole("on_press", 0),
        );
        let holes: Vec<Box<dyn Any>> = vec![Box::new(42i64)];
        assert!(matches!(inflate(&t, &holes).err(), Some(InflateError::PropType { .. })));
    }

    #[test]
    fn positional_arg_constructs_a_text_like_the_builder() {
        // Text("Hi") — the content is a positional constructor arg, static.
        let t = tmpl(TemplateNode::new("Text").with_arg_static(StaticValue::Str("Hi".into())));
        let inflated = inflate(&t, &[]).expect("inflate Text(\"Hi\")");
        assert_eq!(measure(&*inflated), measure(&Text::new("Hi")));
    }

    #[test]
    fn positional_arg_binds_a_hole() {
        // Text(title) — content comes from a runtime hole.
        let t = tmpl(TemplateNode::new("Text").with_arg_hole(0));
        let holes: Vec<Box<dyn Any>> = vec![Box::new(String::from("live"))];
        let inflated = inflate(&t, &holes).expect("inflate Text(hole)");
        assert_eq!(measure(&*inflated), measure(&Text::new("live")));
    }

    #[test]
    fn unknown_widget_escalates() {
        let t = tmpl(TemplateNode::new("NoSuchWidget"));
        assert_eq!(inflate(&t, &[]).err(), Some(InflateError::UnknownWidget("NoSuchWidget".into())));
    }

    #[test]
    fn inflates_children_and_matches_the_builder_for_a_multi_child_tree() {
        // Column does its own layout over private children (it doesn't expose
        // them via children()), so prove nesting through observable layout:
        // two Text children must (a) match the equivalent builder tree and
        // (b) make the column taller than an empty one.
        let t = tmpl(
            TemplateNode::new("Column")
                .with_static("spacing", StaticValue::Float(8.0))
                .with_child(TemplateNode::new("Text").with_arg_static(StaticValue::Str("A".into())))
                .with_child(TemplateNode::new("Text").with_arg_static(StaticValue::Str("B".into()))),
        );
        let inflated = inflate(&t, &[]).expect("inflate");
        let built = Column::new().spacing(8.0).child(Text::new("A")).child(Text::new("B"));
        assert_eq!(measure(&*inflated), measure(&built), "two-child inflate must match builder");

        let empty = inflate(&tmpl(TemplateNode::new("Column")), &[]).expect("inflate empty");
        assert!(measure(&*inflated).height > measure(&*empty).height, "children should add height");
    }

    #[test]
    fn inflated_static_tree_lays_out_identically_to_the_builder() {
        let t = tmpl(
            TemplateNode::new("Column")
                .with_static("spacing", StaticValue::Float(8.0))
                .with_child(TemplateNode::new("Text").with_arg_static(StaticValue::Str("Hi".into()))),
        );
        let inflated = inflate(&t, &[]).expect("inflate");
        let built = Column::new().spacing(8.0).child(Text::new("Hi"));
        assert_eq!(measure(&*inflated), measure(&built), "inflater must match builder output");
    }

    #[test]
    fn binds_value_holes_by_index_matching_the_builder() {
        // spacing and text both come from holes, bound by position.
        let t = tmpl(
            TemplateNode::new("Column")
                .with_hole("spacing", 0)
                .with_child(TemplateNode::new("Text").with_arg_hole(1)),
        );
        let holes: Vec<Box<dyn Any>> = vec![Box::new(8.0f32), Box::new(String::from("Hi"))];
        let inflated = inflate(&t, &holes).expect("inflate");
        let built = Column::new().spacing(8.0).child(Text::new("Hi"));
        assert_eq!(measure(&*inflated), measure(&built), "hole binding must match builder output");
    }

    #[test]
    fn hole_out_of_range_escalates() {
        let t = tmpl(TemplateNode::new("Column").with_hole("spacing", 5));
        assert_eq!(inflate(&t, &[]).err(), Some(InflateError::HoleOutOfRange { index: 5, len: 0 }));
    }

    #[test]
    fn wrong_hole_type_escalates() {
        // spacing hole holds a String, not an f32 → PropType, never a bad widget.
        let t = tmpl(TemplateNode::new("Column").with_hole("spacing", 0));
        let holes: Vec<Box<dyn Any>> = vec![Box::new(String::from("not a number"))];
        assert_eq!(
            inflate(&t, &holes).err(),
            Some(InflateError::PropType { widget: "Column".into(), prop: "spacing".into(), expected: "f32" })
        );
    }

    #[test]
    fn unknown_prop_escalates() {
        let t = tmpl(TemplateNode::new("Column").with_static("bogus", StaticValue::Int(1)));
        assert_eq!(
            inflate(&t, &[]).err(),
            Some(InflateError::UnknownProp { widget: "Column".into(), prop: "bogus".into() })
        );
    }

    #[test]
    fn third_party_widget_registers_and_inflates() {
        // Mirrors the D115 extensibility bar: a widget rosace never heard of.
        register_widget("MyBadge", |_args, _props, _children| Ok(Box::new(Text::new("badge")) as Box<dyn Widget>));
        assert!(is_registered("MyBadge"));
        let t = tmpl(TemplateNode::new("MyBadge"));
        let w = inflate(&t, &[]).expect("custom widget inflates");
        // It behaves like the Text it wraps.
        assert_eq!(measure(&*w), measure(&Text::new("badge")));
    }

    // ── inflatable! macro (ergonomic registration) ─────────────────────────

    #[test]
    fn inflatable_macro_registers_a_container_with_props_and_children() {
        // No hand-written closure — the macro generates it from a prop table.
        crate::inflatable!("MacroCol", Column::new(), children, {
            "spacing" => spacing: f32,
        });
        assert!(is_registered("MacroCol"));

        let t = tmpl(
            TemplateNode::new("MacroCol")
                .with_static("spacing", StaticValue::Float(8.0))
                .with_child(TemplateNode::new("Text").with_arg_static(StaticValue::Str("A".into())))
                .with_child(TemplateNode::new("Text").with_arg_static(StaticValue::Str("B".into()))),
        );
        let inflated = inflate(&t, &[]).expect("macro-registered widget inflates");
        let built = Column::new().spacing(8.0).child(Text::new("A")).child(Text::new("B"));
        assert_eq!(measure(&*inflated), measure(&built), "macro closure must match builder");
    }

    #[test]
    fn inflatable_macro_binds_a_hole_and_reports_unknown_props() {
        crate::inflatable!("MacroCol2", Column::new(), children, {
            "spacing" => spacing: f32,
        });
        // Hole binding through the generated closure.
        let t = tmpl(TemplateNode::new("MacroCol2").with_hole("spacing", 0));
        let holes: Vec<Box<dyn Any>> = vec![Box::new(6.0f32)];
        let inflated = inflate(&t, &holes).expect("hole binds");
        assert_eq!(measure(&*inflated), measure(&Column::new().spacing(6.0)));
        // Unknown prop still escalates.
        let bad = tmpl(TemplateNode::new("MacroCol2").with_static("nope", StaticValue::Int(1)));
        assert_eq!(
            inflate(&bad, &[]).err(),
            Some(InflateError::UnknownProp { widget: "MacroCol2".into(), prop: "nope".into() })
        );
    }

    #[test]
    fn inflatable_macro_leaf_rejects_children() {
        crate::inflatable!("MacroLeaf", Text::new("leaf"), leaf, {});
        assert!(is_registered("MacroLeaf"));
        // Inflates as a leaf.
        assert_eq!(
            measure(&*inflate(&tmpl(TemplateNode::new("MacroLeaf")), &[]).unwrap()),
            measure(&Text::new("leaf"))
        );
        // Given children → escalate, don't silently drop them.
        let with_kids = tmpl(TemplateNode::new("MacroLeaf").with_child(TemplateNode::new("Text")));
        assert_eq!(
            inflate(&with_kids, &[]).err(),
            Some(InflateError::UnexpectedChildren { widget: "MacroLeaf".into() })
        );
    }
}
