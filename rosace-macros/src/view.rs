//! `view!` codegen. The GRAMMAR lives in `rosace-view-syntax` (shared with the
//! runtime hot-reload parser so they can never diverge — D103); this module
//! only turns the parsed [`ViewElement`] AST into tokens.

use proc_macro2::TokenStream;
use quote::quote;
use rosace_view_syntax::{parse_tokens, ViewElement, ViewLiteral};

// ---------------------------------------------------------------------------
// Builder codegen (release path)
// ---------------------------------------------------------------------------

fn emit_element(el: &ViewElement) -> TokenStream {
    let name = &el.name;
    // Positional args become constructor arguments: `Text("Hi")` → `Text::new("Hi")`.
    let args = el.args.iter().map(|a| &a.expr);
    // Named props become builder setters. Use each prop's ORIGINAL expression
    // (not a reconstructed literal) so untyped literals still coerce to the
    // setter's type (`12.0` → `f32`).
    let prop_calls = el.props.iter().map(|p| {
        let k = &p.name;
        let v = &p.expr;
        quote! { .#k(#v) }
    });
    let child_calls = el.children.iter().map(|child| {
        let child_ts = emit_element(child);
        quote! { .child(#child_ts) }
    });

    quote! {
        #name::new(#(#args),*)
        #(#prop_calls)*
        #(#child_calls)*
    }
}

// ---------------------------------------------------------------------------
// Template descriptor codegen (dev path, `rsc-hot`)
// ---------------------------------------------------------------------------

/// Known **nullary** handler props (`Fn()`). In dev mode these are wrapped as
/// `Arc<dyn Fn()>` so the inflater can bind them from a hole. Arg-taking
/// handlers (`on_change(T)`) are a future extension and are NOT in this set.
fn is_handler_prop(name: &str) -> bool {
    matches!(name, "on_press" | "on_click" | "on_tap" | "on_long_press")
}

/// Emit a `StaticValue` from a parsed literal.
fn emit_static_value(lit: &ViewLiteral) -> TokenStream {
    match lit {
        ViewLiteral::Bool(b) => quote! { ::rosace::widgets::StaticValue::Bool(#b) },
        ViewLiteral::Int(i) => quote! { ::rosace::widgets::StaticValue::Int(#i) },
        ViewLiteral::Float(f) => quote! { ::rosace::widgets::StaticValue::Float(#f) },
        ViewLiteral::Str(s) => quote! { ::rosace::widgets::StaticValue::Str(#s.to_string()) },
    }
}

/// Emit the `TemplateNode` for one element, assigning hole indices in the SAME
/// traversal order the runtime parser uses (props before children) and
/// collecting each hole's `{expr}` for the hole-filler array.
fn emit_template_node(el: &ViewElement, hole: &mut usize, hole_exprs: &mut Vec<TokenStream>) -> TokenStream {
    let name = el.name.to_string();

    // Positional args first (they fill the earliest hole slots), matching the
    // hole order the runtime parser assigns.
    let arg_calls: Vec<TokenStream> = el
        .args
        .iter()
        .map(|a| match &a.literal {
            Some(lit) => {
                let sv = emit_static_value(lit);
                quote! { .with_arg_static(#sv) }
            }
            None => {
                let idx = *hole;
                *hole += 1;
                let expr = &a.expr;
                hole_exprs.push(quote! { #expr });
                quote! { .with_arg_hole(#idx) }
            }
        })
        .collect();

    let prop_calls: Vec<TokenStream> = el
        .props
        .iter()
        .map(|p| {
            let key = p.name.to_string();
            match &p.literal {
                Some(lit) => {
                    let sv = emit_static_value(lit);
                    quote! { .with_static(#key, #sv) }
                }
                None => {
                    let idx = *hole;
                    *hole += 1;
                    let expr = &p.expr;
                    // A handler prop (`on_press: || …`) is a closure — wrap it as
                    // a concrete, callable, type-erasable `Arc<dyn Fn()>` so the
                    // inflater can round-trip it through the hole array. Value
                    // props travel as-is.
                    if is_handler_prop(&key) {
                        hole_exprs.push(quote! {
                            ::std::sync::Arc::new(#expr)
                                as ::std::sync::Arc<dyn ::core::ops::Fn() + ::core::marker::Send + ::core::marker::Sync>
                        });
                    } else {
                        hole_exprs.push(quote! { #expr });
                    }
                    quote! { .with_hole(#key, #idx) }
                }
            }
        })
        .collect();

    let child_calls: Vec<TokenStream> = el
        .children
        .iter()
        .map(|child| {
            let child_ts = emit_template_node(child, hole, hole_exprs);
            quote! { .with_child(#child_ts) }
        })
        .collect();

    quote! {
        ::rosace::widgets::TemplateNode::new(#name)
        #(#arg_calls)*
        #(#prop_calls)*
        #(#child_calls)*
    }
}

/// Expand the `view! { … }` macro.
///
/// Two modes (D103 Option A — pure builders in release):
/// - **release** (default): emit only the builder calls — identical to
///   hand-written code, zero template machinery in the binary.
/// - **dev** (`rsc-hot`): build the widget by **inflating** the template
///   descriptor with the compiled `{expr}` hole values — the SAME path a
///   hot-swap takes. Registers the site's shape once (keyed by source location)
///   for the watcher to diff. Widgets used here must be registered.
pub fn expand(input: TokenStream) -> TokenStream {
    let el = match parse_tokens(input) {
        Ok(el) => el,
        Err(e) => return e.to_compile_error(),
    };

    if cfg!(feature = "rsc-hot") {
        let mut hole = 0usize;
        let mut hole_exprs: Vec<TokenStream> = Vec::new();
        let node = emit_template_node(&el, &mut hole, &mut hole_exprs);
        quote! {{
            let __rosace_key = ::rosace::widgets::TemplateKey::new(
                ::core::file!(),
                ::core::line!(),
                ::core::column!(),
            );
            // Register the compiled-in BASELINE shape once; a hot-swap replaces
            // this registry entry with an edited descriptor.
            static __ROSACE_TEMPLATE_ONCE: ::std::sync::Once = ::std::sync::Once::new();
            __ROSACE_TEMPLATE_ONCE.call_once(|| {
                ::rosace::widgets::template::register(
                    ::rosace::widgets::Template::new(__rosace_key.clone(), #node),
                );
            });
            // Inflate the CURRENT registered descriptor (the baseline, or a
            // hot-swapped one) with this frame's compiled hole values — so a
            // swap takes effect on the next rebuild with no recompile.
            let __rosace_current = ::rosace::widgets::template::get(&__rosace_key)
                .expect("view! baseline template must be registered");
            let __rosace_holes: ::std::vec::Vec<::std::boxed::Box<dyn ::core::any::Any>> = ::std::vec![
                #( ::std::boxed::Box::new(#hole_exprs) as ::std::boxed::Box<dyn ::core::any::Any> ),*
            ];
            ::rosace::widgets::template::inflate(&__rosace_current, &__rosace_holes)
                .unwrap_or_else(|__e| ::core::panic!("view! inflate failed: {}", __e))
        }}
    } else {
        emit_element(&el)
    }
}
