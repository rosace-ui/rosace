//! `#[derive(ShaderUniforms)]` — generates a WGSL-uniform-layout-correct
//! `to_bytes()` (D109/Phase 27 Step 1).
//!
//! The whole point of this derive is that uniform packing is decided at
//! macro-expansion time, per WGSL *uniform address space* rules, and the
//! only thing that runs at paint time is a straight sequence of
//! `extend_from_slice` calls — no reflection, no runtime layout math, and
//! no way for a widget author to hand-pack a misaligned buffer.
//!
//! Supported field types and their WGSL uniform layout:
//!
//! | Rust field       | WGSL         | align | size |
//! |------------------|--------------|-------|------|
//! | `f32`/`u32`/`i32`| scalar       | 4     | 4    |
//! | `[f32; 2]`       | `vec2<f32>`  | 8     | 8    |
//! | `[f32; 3]`       | `vec3<f32>`  | 16    | 12   |
//! | `[f32; 4]`       | `vec4<f32>`  | 16    | 16   |
//! | `[[f32; 4]; 4]`  | `mat4x4<f32>`| 16    | 64   |
//!
//! Anything else is a compile error naming the field — better a build
//! failure than garbage uniforms with no error at any stage. Total size is
//! rounded up to 16 (safe over-padding for uniform buffer bindings).

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, Type};

/// (WGSL alignment, WGSL size, tokens writing this field's bytes LE).
struct FieldLayout {
    align: usize,
    size: usize,
    write: TokenStream,
}

/// Classify a field's type syntactically. Returns `None` for unsupported
/// types — the caller turns that into a compile error naming the field.
fn classify(ident: &syn::Ident, ty: &Type) -> Option<FieldLayout> {
    // Scalars: f32 / u32 / i32.
    if let Type::Path(p) = ty {
        if p.qself.is_none() && p.path.segments.len() == 1 {
            let name = p.path.segments[0].ident.to_string();
            if matches!(name.as_str(), "f32" | "u32" | "i32") {
                return Some(FieldLayout {
                    align: 4,
                    size: 4,
                    write: quote! { buf.extend_from_slice(&self.#ident.to_le_bytes()); },
                });
            }
        }
        return None;
    }

    // Arrays: [f32; 2|3|4] and [[f32; 4]; 4].
    let Type::Array(outer) = ty else { return None };
    let outer_len = array_len(outer)?;

    match &*outer.elem {
        // [f32; N]
        Type::Path(p)
            if p.qself.is_none()
                && p.path.segments.len() == 1
                && p.path.segments[0].ident == "f32" =>
        {
            let (align, size) = match outer_len {
                2 => (8, 8),
                3 => (16, 12),
                4 => (16, 16),
                _ => return None,
            };
            Some(FieldLayout {
                align,
                size,
                write: quote! {
                    for component in self.#ident.iter() {
                        buf.extend_from_slice(&component.to_le_bytes());
                    }
                },
            })
        }
        // [[f32; 4]; 4]
        Type::Array(inner) => {
            let inner_len = array_len(inner)?;
            let inner_is_f32 = matches!(
                &*inner.elem,
                Type::Path(p) if p.qself.is_none()
                    && p.path.segments.len() == 1
                    && p.path.segments[0].ident == "f32"
            );
            if inner_is_f32 && inner_len == 4 && outer_len == 4 {
                Some(FieldLayout {
                    align: 16,
                    size: 64,
                    write: quote! {
                        for column in self.#ident.iter() {
                            for component in column.iter() {
                                buf.extend_from_slice(&component.to_le_bytes());
                            }
                        }
                    },
                })
            } else {
                None
            }
        }
        _ => None,
    }
}

fn array_len(arr: &syn::TypeArray) -> Option<usize> {
    if let syn::Expr::Lit(lit) = &arr.len {
        if let syn::Lit::Int(int) = &lit.lit {
            return int.base10_parse::<usize>().ok();
        }
    }
    None
}

pub fn expand(input: DeriveInput) -> TokenStream {
    let name = &input.ident;

    let fields = match &input.data {
        Data::Struct(s) => match &s.fields {
            Fields::Named(named) => &named.named,
            _ => {
                return syn::Error::new_spanned(
                    &input.ident,
                    "#[derive(ShaderUniforms)] requires named fields — tuple/unit structs \
                     have no field order a WGSL struct can mirror by name",
                )
                .to_compile_error()
            }
        },
        _ => {
            return syn::Error::new_spanned(
                &input.ident,
                "#[derive(ShaderUniforms)] only supports structs",
            )
            .to_compile_error()
        }
    };

    if fields.is_empty() {
        return syn::Error::new_spanned(
            &input.ident,
            "#[derive(ShaderUniforms)] on an empty struct — a zero-byte uniform \
             buffer is invalid in wgpu; remove the derive or add fields",
        )
        .to_compile_error();
    }

    // Walk fields in declaration order, computing WGSL offsets at
    // macro-expansion time and emitting padding + write calls.
    let mut writes: Vec<TokenStream> = Vec::new();
    let mut cursor: usize = 0;

    for field in fields {
        let ident = field.ident.as_ref().expect("named fields checked above");
        let Some(layout) = classify(ident, &field.ty) else {
            return syn::Error::new_spanned(
                &field.ty,
                format!(
                    "unsupported uniform field type on `{ident}` — supported: \
                     f32, u32, i32, [f32; 2], [f32; 3], [f32; 4], [[f32; 4]; 4] \
                     (WGSL scalar/vec2/vec3/vec4/mat4x4)"
                ),
            )
            .to_compile_error();
        };

        let offset = cursor.div_ceil(layout.align) * layout.align;
        let pad = offset - cursor;
        if pad > 0 {
            writes.push(quote! { buf.extend_from_slice(&[0u8; #pad]); });
        }
        writes.push(layout.write);
        cursor = offset + layout.size;
    }

    // Round the total up to 16 — safe over-padding for uniform bindings.
    let total = cursor.div_ceil(16) * 16;
    let tail_pad = total - cursor;
    if tail_pad > 0 {
        writes.push(quote! { buf.extend_from_slice(&[0u8; #tail_pad]); });
    }

    quote! {
        impl ::rosace_shader::ShaderUniforms for #name {
            fn to_bytes(&self) -> ::std::vec::Vec<u8> {
                let mut buf = ::std::vec::Vec::with_capacity(#total);
                #(#writes)*
                debug_assert_eq!(buf.len(), #total);
                buf
            }
        }
    }
}
