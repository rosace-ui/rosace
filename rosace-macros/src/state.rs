use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    parse::Parse,
    parse::ParseStream,
    parse2,
    Expr, Ident, Token, Type, Visibility,
    Result as SynResult,
};

/// Parsed form of `[pub] name: Type = default_expr`
struct StateField {
    vis:     Visibility,
    name:    Ident,
    ty:      Type,
    default: Expr,
}

impl Parse for StateField {
    fn parse(input: ParseStream) -> SynResult<Self> {
        let vis: Visibility = input.parse()?;
        let name: Ident     = input.parse()?;
        input.parse::<Token![:]>()?;
        let ty: Type        = input.parse()?;
        input.parse::<Token![=]>()?;
        let default: Expr   = input.parse()?;
        // optional trailing semicolon
        let _ = input.parse::<Token![;]>();
        Ok(Self { vis, name, ty, default })
    }
}

/// Expand `#[state]` on a field declaration.
///
/// Input:  `pub count: i32 = 0`
/// Output: `pub count: rosace_state::Atom<i32> = rosace_state::Atom::new(0)`
pub fn expand(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let field: StateField = match parse2(item.clone()) {
        Ok(f)  => f,
        Err(_) => {
            return quote! {
                compile_error!(
                    "#[state] expects: [pub] name: Type = default_expr"
                );
            };
        }
    };

    let StateField { vis, name, ty, default } = field;

    quote! {
        #vis #name: rosace_state::Atom<#ty> = rosace_state::Atom::new(#default);
    }
}
