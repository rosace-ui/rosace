use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    braced,
    parse::{Parse, ParseStream, Result as ParseResult},
    Expr, Ident, Token,
};

// ---------------------------------------------------------------------------
// AST types
// ---------------------------------------------------------------------------

/// A single element node in the view tree.
struct Element {
    /// The widget/component type name (e.g. `Column`, `Text`).
    name: Ident,
    /// `key: expr` pairs — become builder setter calls.
    props: Vec<(Ident, Expr)>,
    /// Nested child elements — become `.child(…)` calls.
    children: Vec<Element>,
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

/// Parse the mixed list of `key: value` props and `Name { … }` child elements
/// from the interior of an element's braces.
///
/// Disambiguation rule:
///   - `ident :`  → prop (the value is parsed as an `Expr`).
///   - `ident {`  → child element.
///
/// Note: struct-literal expressions in prop values (`Prop: Foo { x: 1 }`) are
/// not supported in Phase 1 because the two syntaxes are ambiguous. Use a
/// variable or function call instead.
type ParseItemsResult = (Vec<(Ident, Expr)>, Vec<Element>);
fn parse_items(input: ParseStream) -> ParseResult<ParseItemsResult> {
    let mut props: Vec<(Ident, Expr)> = Vec::new();
    let mut children: Vec<Element> = Vec::new();

    while !input.is_empty() {
        let ident: Ident = input.parse()?;

        if input.peek(Token![:]) {
            // Prop: `ident : expr`
            input.parse::<Token![:]>()?;
            // Parse the value expression. Struct literals are intentionally
            // excluded here (see note above); users should wrap them in parens
            // or assign them to a variable first.
            let val: Expr = input.call(Expr::parse_without_eager_brace)?;
            // Allow an optional comma separator between props.
            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
            props.push((ident, val));
        } else {
            // Child element: `Name { … }`
            let child_content;
            braced!(child_content in input);
            let (child_props, child_children) = parse_items(&child_content)?;
            children.push(Element {
                name: ident,
                props: child_props,
                children: child_children,
            });
        }
    }

    Ok((props, children))
}

impl Parse for Element {
    fn parse(input: ParseStream) -> ParseResult<Self> {
        let name: Ident = input.parse()?;
        let content;
        braced!(content in input);
        let (props, children) = parse_items(&content)?;
        Ok(Element { name, props, children })
    }
}

// ---------------------------------------------------------------------------
// Code generation
// ---------------------------------------------------------------------------

fn emit_element(el: &Element) -> TokenStream {
    let name = &el.name;
    let prop_calls = el.props.iter().map(|(k, v)| quote! { .#k(#v) });
    let child_calls = el.children.iter().map(|child| {
        let child_ts = emit_element(child);
        quote! { .child(#child_ts) }
    });

    quote! {
        #name::new()
        #(#prop_calls)*
        #(#child_calls)*
    }
}

/// Expand the `view! { … }` macro.
pub fn expand(input: TokenStream) -> TokenStream {
    match syn::parse2::<Element>(input) {
        Ok(el) => emit_element(&el),
        Err(e) => e.to_compile_error(),
    }
}
