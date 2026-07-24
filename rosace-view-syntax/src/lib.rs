//! The **single** `view!` grammar (D103 / D102 Tier 1).
//!
//! Both the compile-time `view!` proc-macro (`rosace-macros`) and the runtime
//! hot-reload parser (`rosace-widgets::template::parse`) parse `view!` through
//! THIS crate — so they can never disagree about what a `view!` means. A
//! divergence between the two parsers would make hot reload silently build the
//! wrong tree; sharing the grammar removes that class of bug by construction.
//!
//! This crate only *parses* into an AST ([`ViewElement`]); it is deliberately
//! free of widget/template types so both consumers can depend on it without a
//! cycle. Each consumer converts the AST to what it needs:
//! - the macro → builder + descriptor **tokens**;
//! - the runtime parser → a `Template` **value**.
//!
//! # Grammar
//! ```text
//! element  := Ident '{' items '}'
//! items    := ( prop | element )*
//! prop     := Ident ':' expr [',']
//! ```
//! A `prop` whose value is a literal (`"x"`, `12`, `1.5`, `true`) is **data**
//! ([`ViewProp::literal`] is `Some`); anything else is a **dynamic** hole
//! ([`ViewProp::literal`] is `None`), whose `expr` the macro emits and the
//! runtime parser counts as a positional slot.

use proc_macro2::{Delimiter, TokenStream, TokenTree};
use syn::{
    braced, parenthesized,
    parse::{Parse, ParseStream, Result as ParseResult},
    punctuated::Punctuated,
    token, Expr, ExprLit, Ident, Lit, Token,
};

/// A prop literal value — the data-travelable subset. Kept independent of
/// `rosace_widgets::StaticValue` so this crate has no widget dependency; the
/// widgets side converts.
#[derive(Clone, Debug, PartialEq)]
pub enum ViewLiteral {
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
}

/// One `key: value` prop. `expr` is always the original expression (so the
/// macro can emit it verbatim — preserving untyped-literal coercion for the
/// builder); `literal` is `Some` when the value is a literal (data), `None`
/// when it is a dynamic hole.
pub struct ViewProp {
    pub name: Ident,
    pub expr: Expr,
    pub literal: Option<ViewLiteral>,
}

impl ViewProp {
    /// The prop name as a string (registry/descriptor key).
    pub fn name_str(&self) -> String {
        self.name.to_string()
    }
    /// Whether this prop is a dynamic hole (vs static data).
    pub fn is_hole(&self) -> bool {
        self.literal.is_none()
    }
}

/// One positional **constructor argument** — the `("Hi")` in `Text("Hi")` or
/// the `"Save"` in `Button("Save", …)`. Goes to `Widget::new(...)`. Like a
/// prop, `expr` is the original tokens and `literal` is `Some` for a data
/// literal, `None` for a dynamic hole.
pub struct ViewArg {
    pub expr: Expr,
    pub literal: Option<ViewLiteral>,
}

impl ViewArg {
    pub fn is_hole(&self) -> bool {
        self.literal.is_none()
    }
}

/// One element node: a widget name, its positional constructor args, its named
/// props (source order), and its children.
///
/// Syntax: `Name`, `Name(arg, arg)`, `Name { props/children }`, or
/// `Name(args) { props/children }` — parens are constructor args, braces are
/// named props + children. Both are optional.
pub struct ViewElement {
    pub name: Ident,
    pub args: Vec<ViewArg>,
    pub props: Vec<ViewProp>,
    pub children: Vec<ViewElement>,
}

impl ViewElement {
    /// The widget name as a string (registry key).
    pub fn name_str(&self) -> String {
        self.name.to_string()
    }
}

/// Parse the optional `(args)` + optional `{ items }` that follow an element's
/// name (`name` already consumed). A bare name (neither) is a zero-config
/// element → `Name::new()`.
fn parse_element_tail(name: Ident, input: ParseStream) -> ParseResult<ViewElement> {
    let mut args = Vec::new();
    if input.peek(token::Paren) {
        let content;
        parenthesized!(content in input);
        let exprs: Punctuated<Expr, Token![,]> = content.parse_terminated(Expr::parse, Token![,])?;
        for expr in exprs {
            let literal = literal_of(&expr);
            args.push(ViewArg { expr, literal });
        }
    }

    let mut props = Vec::new();
    let mut children = Vec::new();
    if input.peek(token::Brace) {
        let content;
        braced!(content in input);
        let (p, c) = parse_items(&content)?;
        props = p;
        children = c;
    }

    Ok(ViewElement { name, args, props, children })
}

/// Classify a parsed expression as a literal (data) or dynamic (hole),
/// returning the literal value when applicable. The `expr` is retained by the
/// caller regardless.
fn literal_of(e: &Expr) -> Option<ViewLiteral> {
    if let Expr::Lit(ExprLit { lit, .. }) = e {
        match lit {
            Lit::Str(s) => Some(ViewLiteral::Str(s.value())),
            Lit::Int(i) => i.base10_parse::<i64>().ok().map(ViewLiteral::Int),
            Lit::Float(f) => f.base10_parse::<f64>().ok().map(ViewLiteral::Float),
            Lit::Bool(b) => Some(ViewLiteral::Bool(b.value)),
            _ => None,
        }
    } else {
        None
    }
}

/// Parse the interior of an element's braces: a mixed list of `key: value`
/// props and `Name { … }` child elements.
///
/// Disambiguation: `ident :` → prop; `ident {` → child element. Struct-literal
/// prop values are excluded (parsed `without_eager_brace`), matching the
/// original `view!` behaviour.
fn parse_items(input: ParseStream) -> ParseResult<(Vec<ViewProp>, Vec<ViewElement>)> {
    let mut props = Vec::new();
    let mut children = Vec::new();

    while !input.is_empty() {
        let ident: Ident = input.parse()?;

        if input.peek(Token![:]) {
            input.parse::<Token![:]>()?;
            let expr: Expr = input.call(Expr::parse_without_eager_brace)?;
            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
            let literal = literal_of(&expr);
            props.push(ViewProp { name: ident, expr, literal });
        } else {
            // A child element: `Name`, `Name(args)`, `Name { … }`, or a mix.
            children.push(parse_element_tail(ident, input)?);
        }
    }

    Ok((props, children))
}

impl Parse for ViewElement {
    fn parse(input: ParseStream) -> ParseResult<Self> {
        let name: Ident = input.parse()?;
        parse_element_tail(name, input)
    }
}

/// Parse a `view!` body (the tokens inside `view! { … }`) into an AST. Used by
/// the proc-macro with the macro's input token stream.
pub fn parse_tokens(tokens: proc_macro2::TokenStream) -> ParseResult<ViewElement> {
    syn::parse2(tokens)
}

/// Parse a `view!` body from source text into an AST. Used by the runtime
/// hot-reload parser reading an edited file — same grammar, no compiler.
pub fn parse_str(src: &str) -> ParseResult<ViewElement> {
    syn::parse_str(src)
}

// ---------------------------------------------------------------------------
// File scanning (runtime hot-reload): find every `view!` in a source file
// ---------------------------------------------------------------------------

/// One `view!` invocation found in a source file: its parsed AST plus the
/// source location of the `view` token (so it can be keyed like the macro's
/// `TemplateKey`). `line` is 1-based; `column` is 0-based (proc-macro2's
/// convention) — match primarily on `line`.
pub struct ViewSite {
    pub line: usize,
    pub column: usize,
    pub element: ViewElement,
}

/// Why scanning a file failed.
#[derive(Debug, Clone)]
pub enum ScanError {
    /// The file text could not even be tokenised.
    Lex(String),
    /// A `view!` body was found but did not parse (message + location).
    Parse { line: usize, column: usize, msg: String },
}

impl std::fmt::Display for ScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScanError::Lex(m) => write!(f, "lex error: {m}"),
            ScanError::Parse { line, column, msg } => {
                write!(f, "view! at {line}:{column}: {msg}")
            }
        }
    }
}
impl std::error::Error for ScanError {}

/// Find every `view! { … }` invocation in a source file (lexed, not fully
/// parsed — so it works even if the rest of the file is mid-edit) and parse
/// each body. Recurses into nested groups, so `view!`s inside functions,
/// closures, or other macros are all found.
pub fn scan_file(src: &str) -> Result<Vec<ViewSite>, ScanError> {
    let tokens: TokenStream = src.parse().map_err(|e: proc_macro2::LexError| ScanError::Lex(e.to_string()))?;
    let mut bodies: Vec<(usize, usize, TokenStream)> = Vec::new();
    collect_view_bodies(tokens, &mut bodies);

    let mut sites = Vec::with_capacity(bodies.len());
    for (line, column, body) in bodies {
        let element = parse_tokens(body).map_err(|e| ScanError::Parse { line, column, msg: e.to_string() })?;
        sites.push(ViewSite { line, column, element });
    }
    Ok(sites)
}

/// Walk a token stream collecting each `view ! { body }` as (line, col, body),
/// recursing into every group (including the matched body, for nested `view!`).
fn collect_view_bodies(tokens: TokenStream, out: &mut Vec<(usize, usize, TokenStream)>) {
    let tts: Vec<TokenTree> = tokens.into_iter().collect();
    let mut i = 0;
    while i < tts.len() {
        if let TokenTree::Ident(id) = &tts[i] {
            let is_view = id.to_string() == "view";
            let bang = matches!(tts.get(i + 1), Some(TokenTree::Punct(p)) if p.as_char() == '!');
            if is_view && bang {
                if let Some(TokenTree::Group(g)) = tts.get(i + 2) {
                    if g.delimiter() == Delimiter::Brace {
                        let start = id.span().start();
                        out.push((start.line, start.column, g.stream()));
                        // Do NOT recurse into the matched body: its contents are
                        // widget grammar (`Name { }`), not arbitrary code — a
                        // `view!` cannot legally nest inside a `view!` body.
                        i += 3;
                        continue;
                    }
                }
            }
        }
        if let TokenTree::Group(g) = &tts[i] {
            collect_view_bodies(g.stream(), out);
        }
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_leaf_with_a_literal_prop() {
        let el = parse_str("Text { content: \"Hi\" }").unwrap();
        assert_eq!(el.name_str(), "Text");
        assert_eq!(el.props.len(), 1);
        assert_eq!(el.props[0].name_str(), "content");
        assert_eq!(el.props[0].literal, Some(ViewLiteral::Str("Hi".into())));
        assert!(!el.props[0].is_hole());
    }

    #[test]
    fn classifies_each_literal_kind() {
        let el = parse_str("W { a: 12, b: 1.5, c: true, d: \"s\" }").unwrap();
        let lits: Vec<_> = el.props.iter().map(|p| p.literal.clone()).collect();
        assert_eq!(
            lits,
            vec![
                Some(ViewLiteral::Int(12)),
                Some(ViewLiteral::Float(1.5)),
                Some(ViewLiteral::Bool(true)),
                Some(ViewLiteral::Str("s".into())),
            ]
        );
    }

    #[test]
    fn parses_positional_constructor_args() {
        // Text("Hi") — a literal positional arg, no braces.
        let el = parse_str("Text(\"Hi\")").unwrap();
        assert_eq!(el.name_str(), "Text");
        assert_eq!(el.args.len(), 1);
        assert_eq!(el.args[0].literal, Some(ViewLiteral::Str("Hi".into())));
        assert!(el.props.is_empty() && el.children.is_empty());
    }

    #[test]
    fn parses_positional_arg_plus_named_props() {
        // Button("Save") { width: 90.0 } — positional arg + a named prop.
        let el = parse_str("Button(\"Save\") { width: 90.0 }").unwrap();
        assert_eq!(el.name_str(), "Button");
        assert_eq!(el.args.len(), 1);
        assert_eq!(el.args[0].literal, Some(ViewLiteral::Str("Save".into())));
        assert_eq!(el.props.len(), 1);
        assert_eq!(el.props[0].name_str(), "width");
    }

    #[test]
    fn a_positional_arg_can_be_a_hole() {
        // Text(title) — a variable arg is a dynamic hole.
        let el = parse_str("Text(title)").unwrap();
        assert_eq!(el.args.len(), 1);
        assert!(el.args[0].is_hole());
    }

    #[test]
    fn a_bare_name_is_a_zero_config_element() {
        let el = parse_str("Divider").unwrap();
        assert_eq!(el.name_str(), "Divider");
        assert!(el.args.is_empty() && el.props.is_empty() && el.children.is_empty());
    }

    #[test]
    fn positional_args_work_on_children_too() {
        let el = parse_str("Column { Text(\"a\") Text(b) }").unwrap();
        assert_eq!(el.children.len(), 2);
        assert_eq!(el.children[0].args[0].literal, Some(ViewLiteral::Str("a".into())));
        assert!(el.children[1].args[0].is_hole());
    }

    #[test]
    fn a_non_literal_is_a_hole() {
        let el = parse_str("Column { spacing: my_gap }").unwrap();
        assert!(el.props[0].is_hole(), "a variable must be a dynamic hole");
        assert!(el.props[0].literal.is_none());
    }

    #[test]
    fn parses_nested_children_in_order() {
        let el = parse_str("Column { Text { content: \"a\" } Text { content: \"b\" } }").unwrap();
        assert!(el.props.is_empty());
        assert_eq!(el.children.len(), 2);
        assert_eq!(el.children[0].name_str(), "Text");
        assert_eq!(el.children[1].props[0].literal, Some(ViewLiteral::Str("b".into())));
    }

    #[test]
    fn scan_finds_a_view_with_its_line() {
        let src = "fn f() {\n    let x = view! { Column { spacing: 4.0 } };\n}\n";
        let sites = scan_file(src).unwrap();
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].line, 2, "view! is on line 2");
        assert_eq!(sites[0].element.name_str(), "Column");
    }

    #[test]
    fn scan_finds_multiple_views_across_functions() {
        let src = "\
fn a() { view! { Row { } } }
fn b() {
    let c = view! { Column { Text { content: \"x\" } } };
}
";
        let sites = scan_file(src).unwrap();
        // Two `view!` sites: Row (line 1) and Column (line 3). `Text` is a
        // child of Column, not a separate site.
        assert_eq!(sites.len(), 2);
        let names: Vec<_> = sites.iter().map(|s| s.element.name_str()).collect();
        assert!(names.contains(&"Row".to_string()));
        assert!(names.contains(&"Column".to_string()));
        assert_eq!(sites.iter().find(|s| s.element.name_str() == "Row").unwrap().line, 1);
        assert_eq!(sites.iter().find(|s| s.element.name_str() == "Column").unwrap().line, 3);
    }

    #[test]
    fn scan_ignores_non_view_macros() {
        let src = "fn f() { println!(\"view!\"); let v = vec![1, 2, 3]; }";
        assert!(scan_file(src).unwrap().is_empty());
    }

    #[test]
    fn parse_str_and_parse_tokens_agree() {
        let src = "Column { spacing: 4.0 Text { content: name } }";
        let a = parse_str(src).unwrap();
        let toks: proc_macro2::TokenStream = src.parse().unwrap();
        let b = parse_tokens(toks).unwrap();
        assert_eq!(a.name_str(), b.name_str());
        assert_eq!(a.props.len(), b.props.len());
        assert_eq!(a.children.len(), b.children.len());
        assert_eq!(a.children[0].props[0].is_hole(), b.children[0].props[0].is_hole());
    }
}
