extern crate proc_macro;

mod component;
mod state;
mod view;

use proc_macro::TokenStream;

/// Transforms a function into a ROSACE component.
///
/// Each parameter becomes a public struct field and a builder-style setter
/// method. The original function body is preserved inside `build()`.
///
/// # Example
/// ```rust,ignore
/// #[component]
/// pub fn Greeting(name: String, size: f32) -> Element {
///     Text::new().content(name).size(size).build()
/// }
///
/// // Usage:
/// let elem = Greeting::new().name("World".into()).size(16.0).build();
/// ```
#[proc_macro_attribute]
pub fn component(attr: TokenStream, item: TokenStream) -> TokenStream {
    component::expand(attr.into(), item.into()).into()
}

/// Transforms a field declaration into an `Atom<T>` binding.
///
/// # Example
/// ```rust,ignore
/// #[state]
/// pub count: i32 = 0;
///
/// // Expands to:
/// pub count: rosace_state::Atom<i32> = rosace_state::Atom::new(0);
/// ```
#[proc_macro_attribute]
pub fn state(attr: TokenStream, item: TokenStream) -> TokenStream {
    state::expand(attr.into(), item.into()).into()
}

/// Declarative macro for building element trees.
///
/// ```text
/// view! {
///     Column {
///         Text { content: "Hello" }
///         Button { label: "Click" on_click: handle_click }
///     }
/// }
/// ```
///
/// Expands to builder calls:
///
/// ```rust,ignore
/// Column::new()
///     .child(Text::new().content("Hello"))
///     .child(Button::new().label("Click").on_click(handle_click))
/// ```
///
/// ## Syntax rules
/// - `key: value` — sets a prop; the value is any Rust expression (struct
///   literals must be wrapped in parentheses or assigned to a variable first
///   to avoid parsing ambiguity with child element syntax).
/// - `Name { … }` — a child element; generates a `.child(…)` call.
/// - Props and children may be interleaved in any order.
/// - An optional comma may follow each `key: value` pair.
#[proc_macro]
pub fn view(input: TokenStream) -> TokenStream {
    view::expand(input.into()).into()
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use proc_macro2::TokenStream;
    use quote::quote;

    // -- #[component] --------------------------------------------------------

    #[test]
    fn component_generates_struct_and_impl() {
        let attr = TokenStream::new();
        let item = quote! {
            pub fn Greeting(name: String) -> String {
                name.clone()
            }
        };
        let out = crate::component::expand(attr, item).to_string();
        assert!(out.contains("struct Greeting"), "struct missing: {out}");
        assert!(out.contains("fn new"), "new() missing: {out}");
        assert!(out.contains("fn build"), "build() missing: {out}");
        assert!(out.contains("fn name"), "setter missing: {out}");
    }

    #[test]
    fn component_no_params() {
        let attr = TokenStream::new();
        let item = quote! {
            pub fn Empty() -> u32 { 42u32 }
        };
        let out = crate::component::expand(attr, item).to_string();
        assert!(out.contains("struct Empty"), "{out}");
        assert!(out.contains("fn build"), "{out}");
    }

    #[test]
    fn component_multiple_params() {
        let attr = TokenStream::new();
        let item = quote! {
            pub fn Counter(initial: i32, step: i32) -> i32 {
                initial + step
            }
        };
        let out = crate::component::expand(attr, item).to_string();
        assert!(out.contains("fn initial"), "initial setter missing: {out}");
        assert!(out.contains("fn step"), "step setter missing: {out}");
    }

    #[test]
    fn component_requires_return_type() {
        let attr = TokenStream::new();
        let item = quote! {
            pub fn NoReturn(x: i32) {}
        };
        let out = crate::component::expand(attr, item).to_string();
        assert!(
            out.contains("compile_error"),
            "expected compile_error, got: {out}"
        );
    }

    // -- #[state] ------------------------------------------------------------

    #[test]
    fn state_expands_to_atom() {
        let attr = TokenStream::new();
        let item = quote! { pub count: i32 = 0 };
        let out = crate::state::expand(attr, item).to_string();
        assert!(out.contains("Atom"), "Atom missing: {out}");
        assert!(out.contains("count"), "field name missing: {out}");
    }

    #[test]
    fn state_preserves_field_name() {
        let attr = TokenStream::new();
        let item = quote! { pub username: String = String::new() };
        let out = crate::state::expand(attr, item).to_string();
        assert!(out.contains("username"), "{out}");
    }

    #[test]
    fn state_preserves_type() {
        let attr = TokenStream::new();
        let item = quote! { value: f32 = 0.0 };
        let out = crate::state::expand(attr, item).to_string();
        assert!(out.contains("f32"), "type missing: {out}");
    }

    #[test]
    fn state_preserves_default() {
        let attr = TokenStream::new();
        let item = quote! { count: i32 = 42 };
        let out = crate::state::expand(attr, item).to_string();
        assert!(out.contains("42"), "default expr missing: {out}");
    }

    #[test]
    fn state_invalid_input_gives_compile_error() {
        let attr = TokenStream::new();
        let item = quote! { this is not valid };
        let out = crate::state::expand(attr, item).to_string();
        assert!(out.contains("compile_error"), "expected error: {out}");
    }

    #[test]
    fn state_wraps_in_atom_new() {
        let attr = TokenStream::new();
        let item = quote! { active: bool = false };
        let out = crate::state::expand(attr, item).to_string();
        assert!(out.contains("Atom"), "{out}");
        assert!(out.contains("false"), "{out}");
    }

    // -- view! ---------------------------------------------------------------

    #[test]
    fn view_single_empty_element() {
        let input = quote! { Column { } };
        let out = crate::view::expand(input).to_string();
        assert!(
            out.contains("Column") && out.contains("new"),
            "unexpected output: {out}"
        );
    }

    #[test]
    fn view_element_with_props() {
        let input = quote! { Text { content: "Hello" } };
        let out = crate::view::expand(input).to_string();
        assert!(out.contains("content"), "prop missing: {out}");
        assert!(out.contains("Hello"), "prop value missing: {out}");
    }

    #[test]
    fn view_nested_children() {
        let input = quote! {
            Column {
                Text { content: "Hello" }
            }
        };
        let out = crate::view::expand(input).to_string();
        assert!(out.contains("child"), ".child() call missing: {out}");
        assert!(out.contains("Text"), "child name missing: {out}");
    }

    #[test]
    fn view_multiple_children() {
        let input = quote! {
            Column {
                Text { content: "First" }
                Text { content: "Second" }
            }
        };
        let out = crate::view::expand(input).to_string();
        assert_eq!(out.matches("child").count(), 2, "expected 2 child calls: {out}");
    }

    #[test]
    fn view_multiple_props() {
        let input = quote! {
            Button { label: "Click" on_click: my_handler }
        };
        let out = crate::view::expand(input).to_string();
        assert!(out.contains("label"), "label missing: {out}");
        assert!(out.contains("on_click"), "on_click missing: {out}");
        assert!(out.contains("my_handler"), "handler missing: {out}");
    }

    #[test]
    fn view_deeply_nested() {
        let input = quote! {
            Stack {
                Column {
                    Text { content: "deep" }
                }
            }
        };
        let out = crate::view::expand(input).to_string();
        assert!(out.contains("Stack"), "{out}");
        assert!(out.contains("Column"), "{out}");
        assert!(out.contains("Text"), "{out}");
    }
}
