extern crate proc_macro;

mod component;

use proc_macro::TokenStream;

/// Transforms a function into a TEZZERA component.
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

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use proc_macro2::TokenStream;
    use quote::quote;

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
}
