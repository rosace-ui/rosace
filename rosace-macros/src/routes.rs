//! `#[routes]` — gives an enum of screens a URL path form (D026).
//!
//! ```rust,ignore
//! #[routes]
//! #[derive(Debug, Clone, PartialEq)]
//! enum Screen {
//!     #[route("/")]             Home,
//!     #[route("/user/:id")]     User { id: u64 },
//!     #[route("/widget/:kind")] Widget(WidgetKind),
//! }
//! ```
//!
//! Generates `impl RoutePath`, so `to_path`/`from_path` are derived from ONE
//! declaration. Writing the two directions by hand is how they drift: the
//! formatter gains a segment the parser does not know about, and a link that
//! the app itself produced stops resolving.
//!
//! Parameters are typed. A `:name` segment binds to the field of that name
//! (or, for a tuple variant, to the next positional field), and is parsed with
//! `FromStr` — so `/user/seven` does not match `User { id: u64 }`. Formatting
//! uses `Display`. A route type is therefore only as good as its fields'
//! round-tripping, which is the app's business and stated in the error text.
//!
//! Variants are tried in DECLARATION ORDER and the first match wins, so a
//! catch-all belongs last. That is a deliberate, documented rule rather than a
//! specificity heuristic: specificity ranking is invisible in the source, and
//! a route table you cannot read top-to-bottom is a route table nobody can
//! reason about.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Fields, LitStr};

/// One `:param` or literal piece of a route pattern.
enum Seg {
    Literal(String),
    Param(String),
}

fn parse_pattern(pattern: &str) -> Vec<Seg> {
    pattern
        .split('/')
        .filter(|s| !s.is_empty())
        .map(|s| match s.strip_prefix(':') {
            Some(name) => Seg::Param(name.to_string()),
            None => Seg::Literal(s.to_string()),
        })
        .collect()
}

pub fn expand(input: DeriveInput) -> TokenStream {
    let ident = input.ident.clone();
    let (impl_g, ty_g, where_c) = input.generics.split_for_impl();

    let data = match &input.data {
        Data::Enum(e) => e,
        _ => {
            return syn::Error::new_spanned(
                &input.ident,
                "#[routes] applies to an enum — each variant is one screen, and \
                 that is what gives a route its exhaustive match",
            )
            .to_compile_error()
        }
    };

    let mut to_arms = Vec::new();
    let mut from_arms = Vec::new();

    for variant in &data.variants {
        let vname = &variant.ident;

        // The #[route("...")] attribute is required: a variant with no path
        // would silently never resolve, and silence is the failure mode this
        // macro exists to remove.
        let pattern = match variant.attrs.iter().find(|a| a.path().is_ident("route")) {
            Some(attr) => match attr.parse_args::<LitStr>() {
                Ok(lit) => lit.value(),
                Err(e) => return e.to_compile_error(),
            },
            None => {
                return syn::Error::new_spanned(
                    variant,
                    format!(
                        "`{vname}` has no #[route(\"/path\")]. Every variant needs one — \
                         a variant without a path can never be reached by a deep link \
                         and can never be written into a URL, which is a routing bug \
                         that shows up only in the field."
                    ),
                )
                .to_compile_error()
            }
        };

        let segs = parse_pattern(&pattern);
        let params: Vec<&String> = segs
            .iter()
            .filter_map(|s| match s {
                Seg::Param(n) => Some(n),
                _ => None,
            })
            .collect();

        // Bind each pattern parameter to a field.
        let (binders, field_count, is_named): (Vec<syn::Ident>, usize, bool) = match &variant.fields
        {
            Fields::Unit => (Vec::new(), 0, false),
            Fields::Named(f) => (
                f.named.iter().map(|x| x.ident.clone().unwrap()).collect(),
                f.named.len(),
                true,
            ),
            Fields::Unnamed(f) => (
                (0..f.unnamed.len()).map(|i| format_ident!("p{i}")).collect(),
                f.unnamed.len(),
                false,
            ),
        };

        if params.len() != field_count {
            return syn::Error::new_spanned(
                variant,
                format!(
                    "`{vname}` has {} field(s) but its route \"{pattern}\" declares {} \
                     parameter(s). They have to correspond — a field with no parameter \
                     could not survive a round trip through a URL, and a parameter with \
                     no field would be parsed and thrown away.",
                    field_count,
                    params.len()
                ),
            )
            .to_compile_error();
        }

        // For named variants the pattern's :names must be the field names, so
        // the mapping is readable at the declaration rather than positional.
        if is_named {
            for p in &params {
                if !binders.iter().any(|b| b == *p) {
                    return syn::Error::new_spanned(
                        variant,
                        format!(
                            "route \"{pattern}\" declares `:{p}` but `{vname}` has no field \
                             called `{p}`. For a struct variant the names must match — \
                             positional matching would make a field rename silently \
                             reorder the URL."
                        ),
                    )
                    .to_compile_error();
                }
            }
        }

        // ── to_path ────────────────────────────────────────────────────────
        let mut fmt = String::new();
        let mut fmt_args: Vec<syn::Ident> = Vec::new();
        let mut positional = 0usize;
        for seg in &segs {
            fmt.push('/');
            match seg {
                Seg::Literal(l) => fmt.push_str(l),
                Seg::Param(p) => {
                    fmt.push_str("{}");
                    if is_named {
                        fmt_args.push(format_ident!("{}", p));
                    } else {
                        fmt_args.push(binders[positional].clone());
                        positional += 1;
                    }
                }
            }
        }
        if fmt.is_empty() {
            fmt.push('/');
        }

        let destructure = match &variant.fields {
            Fields::Unit => quote! {},
            Fields::Named(_) => {
                let names = &binders;
                quote! { { #(#names),* } }
            }
            Fields::Unnamed(_) => {
                let names = &binders;
                quote! { ( #(#names),* ) }
            }
        };
        to_arms.push(quote! {
            Self::#vname #destructure => format!(#fmt #(, #fmt_args)*),
        });

        // ── from_path ──────────────────────────────────────────────────────
        let seg_count = segs.len();
        let mut checks = Vec::new();
        let mut parses = Vec::new();
        let mut positional = 0usize;
        for (i, seg) in segs.iter().enumerate() {
            match seg {
                Seg::Literal(l) => checks.push(quote! {
                    if segments[#i] != #l { return None; }
                }),
                Seg::Param(p) => {
                    let bind = if is_named {
                        format_ident!("{}", p)
                    } else {
                        let b = binders[positional].clone();
                        positional += 1;
                        b
                    };
                    parses.push(quote! {
                        let #bind = segments[#i].parse().ok()?;
                    });
                }
            }
        }
        let construct = match &variant.fields {
            Fields::Unit => quote! { Self::#vname },
            Fields::Named(_) => {
                let names = &binders;
                quote! { Self::#vname { #(#names),* } }
            }
            Fields::Unnamed(_) => {
                let names = &binders;
                quote! { Self::#vname( #(#names),* ) }
            }
        };
        from_arms.push(quote! {
            if segments.len() == #seg_count {
                // Each candidate runs in its own closure so a failed parameter
                // parse falls through to the NEXT variant instead of failing
                // the whole lookup — two patterns can share a shape and differ
                // only in what their parameters accept.
                let attempt = (|| {
                    #(#checks)*
                    #(#parses)*
                    Some(#construct)
                })();
                if attempt.is_some() { return attempt; }
            }
        });
    }

    // Re-emit the enum WITHOUT the `#[route(..)]` markers. They are input to
    // this macro, not real attributes, and leaving them on would fail to
    // compile with "cannot find attribute `route`".
    let mut stripped = input.clone();
    if let Data::Enum(e) = &mut stripped.data {
        for v in &mut e.variants {
            v.attrs.retain(|a| !a.path().is_ident("route"));
        }
    }

    quote! {
        #stripped

        // A routed enum IS a route; making every app write the marker impl
        // by hand adds a line whose only possible value is being forgotten.
        impl #impl_g ::rosace::nav::Route for #ident #ty_g #where_c {}

        impl #impl_g ::rosace::nav::RoutePath for #ident #ty_g #where_c {
            fn to_path(&self) -> String {
                match self {
                    #(#to_arms)*
                }
            }

            fn from_path(path: &str) -> Option<Self> {
                let query_stripped = path.split('?').next().unwrap_or(path);
                let segments: ::std::vec::Vec<&str> =
                    query_stripped.split('/').filter(|s| !s.is_empty()).collect();
                #(#from_arms)*
                None
            }
        }
    }
}
