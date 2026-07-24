//! The template registry (D103 / D102 Tier 1) — a process-global map from a
//! `view!` site's [`TemplateKey`] to the [`Template`] currently compiled into
//! the running binary.
//!
//! The `view!` macro registers each site's descriptor here in dev builds (under
//! the `rsc-hot` feature); the dev watcher reads it to diff an edited template
//! against what is actually running, and the interpreter (step 3) reads it to
//! inflate a subtree. Dev-only by construction — release builds never register
//! (the macro emits pure builder calls), so this map stays empty and unused.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use super::{Template, TemplateKey};

fn registry() -> &'static Mutex<HashMap<TemplateKey, Template>> {
    static REGISTRY: OnceLock<Mutex<HashMap<TemplateKey, Template>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Register (or replace) the template for its site key. Re-registering the same
/// key overwrites — a `view!` site has exactly one compiled-in shape at a time,
/// and a hot-reload push replaces it.
pub fn register(template: Template) {
    let mut map = registry().lock().unwrap_or_else(|e| e.into_inner());
    map.insert(template.key.clone(), template);
}

/// The template currently registered for `key`, if any.
pub fn get(key: &TemplateKey) -> Option<Template> {
    let map = registry().lock().unwrap_or_else(|e| e.into_inner());
    map.get(key).cloned()
}

/// Find the running template for a `view!` site by `(file, line)`, tolerating
/// path-form and column differences between the runtime file scanner and the
/// macro's `file!()`/`column!()` key. Matching is line + path-suffix: the
/// watcher's absolute path and the macro's package-relative `file!()` agree on
/// their trailing segments. Used by the reload runtime (exact key `get` fails
/// here because the two producers compute file/column differently).
pub fn find_running(file: &str, line: u32) -> Option<Template> {
    let map = registry().lock().unwrap_or_else(|e| e.into_inner());
    map.values()
        .find(|t| t.key.line == line && files_match(&t.key.file, file))
        .cloned()
}

/// Whether two path strings name the same file by comparing their trailing
/// path segments (so `src/app.rs` matches `/abs/pkg/src/app.rs`). Segment-wise
/// (not raw `ends_with`) so `app.rs` never matches `map.rs`.
fn files_match(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let a: Vec<&str> = a.split('/').filter(|s| !s.is_empty()).collect();
    let b: Vec<&str> = b.split('/').filter(|s| !s.is_empty()).collect();
    let n = a.len().min(b.len());
    n > 0 && a[a.len() - n..] == b[b.len() - n..]
}

/// Number of registered sites (diagnostics / tests).
pub fn len() -> usize {
    registry().lock().unwrap_or_else(|e| e.into_inner()).len()
}

/// A copy of every registered template (diagnostics / tests). Order is
/// unspecified — the caller should match by key or shape.
pub fn snapshot() -> Vec<Template> {
    registry().lock().unwrap_or_else(|e| e.into_inner()).values().cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::{StaticValue, TemplateNode};

    fn tmpl(file: &str, line: u32, label: &str) -> Template {
        Template::new(
            TemplateKey::new(file, line, 1),
            TemplateNode::new("Text").with_static("content", StaticValue::Str(label.into())),
        )
    }

    #[test]
    fn register_then_get_round_trips() {
        let t = tmpl("src/reg_a.rs", 10, "hello");
        register(t.clone());
        assert_eq!(get(&t.key), Some(t));
    }

    #[test]
    fn get_unknown_key_is_none() {
        assert_eq!(get(&TemplateKey::new("src/never_registered.rs", 999, 1)), None);
    }

    #[test]
    fn re_registering_a_key_replaces_the_shape() {
        let key = TemplateKey::new("src/reg_b.rs", 20, 1);
        register(Template::new(key.clone(), TemplateNode::new("Text").with_static("content", StaticValue::Str("v1".into()))));
        register(Template::new(key.clone(), TemplateNode::new("Text").with_static("content", StaticValue::Str("v2".into()))));
        let got = get(&key).unwrap();
        assert_eq!(got.root.props[0].1, PropValueStr("v2"));
    }

    // Small local helper so the assertion above reads clearly.
    #[allow(non_snake_case)]
    fn PropValueStr(s: &str) -> crate::template::PropValue {
        crate::template::PropValue::Static(StaticValue::Str(s.into()))
    }
}
