use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse2, FnArg, ItemFn, Pat, PatType, ReturnType, Type};

/// Expand the `#[component]` attribute macro.
///
/// Transforms a plain function into a builder-style component struct:
///
/// ```rust,ignore
/// #[component]
/// pub fn Greeting(name: String, size: f32) -> Element { … }
/// ```
///
/// Expands to a struct `Greeting` with fields `name` and `size`, builder
/// setters for each field, a `new()` constructor (using `Default::default()`
/// for every field), and a `build(self) -> Element` method that runs the
/// original function body with all params in scope via `let` bindings.
pub fn expand(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = match parse2::<ItemFn>(item) {
        Ok(f) => f,
        Err(e) => return e.to_compile_error(),
    };

    let vis = &input.vis;
    let name = &input.sig.ident;

    // Return type is required.
    let ret_type: Box<Type> = match &input.sig.output {
        ReturnType::Type(_, ty) => ty.clone(),
        ReturnType::Default => {
            return syn::Error::new_spanned(
                &input.sig,
                "#[component] function must have an explicit return type",
            )
            .to_compile_error();
        }
    };

    // Collect (param_name, param_type) pairs, skipping any `self` receiver.
    let params: Vec<(syn::Ident, Box<Type>)> = input
        .sig
        .inputs
        .iter()
        .filter_map(|arg| {
            if let FnArg::Typed(PatType { pat, ty, .. }) = arg {
                if let Pat::Ident(pi) = pat.as_ref() {
                    return Some((pi.ident.clone(), ty.clone()));
                }
            }
            None
        })
        .collect();

    let body = &input.block;

    // --- code-gen fragments ---

    let struct_fields = params.iter().map(|(n, ty)| quote! { pub #n: #ty });

    let default_inits = params
        .iter()
        .map(|(n, _)| quote! { #n: ::core::default::Default::default() });

    let setters = params.iter().map(|(n, ty)| {
        quote! {
            pub fn #n(mut self, v: #ty) -> Self {
                self.#n = v;
                self
            }
        }
    });

    // Let-bindings injected at the top of build() so the original body can
    // reference its parameters by name unchanged.
    let bindings = params.iter().map(|(n, _)| quote! { let #n = self.#n; });

    quote! {
        #[allow(non_snake_case)]
        #vis struct #name {
            #(#struct_fields,)*
        }

        impl #name {
            /// Create a new instance with all fields set to their `Default`.
            pub fn new() -> Self {
                Self {
                    #(#default_inits,)*
                }
            }

            #(#setters)*

            /// Run the component function body and return its output.
            pub fn build(self) -> #ret_type {
                #(#bindings)*
                #body
            }
        }

        impl ::core::default::Default for #name {
            fn default() -> Self {
                Self::new()
            }
        }
    }
}
