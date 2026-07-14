use std::sync::Arc;
use rosace_state::use_atom;
use crate::validator::Validator;
use crate::error::FieldError;

/// A single form field: a name, a shared string value, and validation
/// rules. Cloning a `FormField` is cheap and shares state (D116 Phase 28
/// Step 8) — every clone reads/writes the SAME underlying atoms, the same
/// "clone shares identity" convention `EditController`/`ScrollController`
/// already use in this codebase. This is what lets `TextInput::field(f)`,
/// `Form::field(f)`, and the app's own submit-button closure all see the
/// same live value/touched/errors without any manual synchronization.
#[derive(Clone)]
pub struct FormField {
    pub name: String,
    value: rosace_state::Atom<String>,
    validators: Vec<Arc<dyn Validator>>,
    /// Last validation errors (populated by `validate()`).
    errors: rosace_state::Atom<Vec<FieldError>>,
    /// Whether the field has been interacted with (touched = show errors).
    touched: rosace_state::Atom<bool>,
}

impl FormField {
    /// Plain constructor — the atoms it creates (`use_atom`) are NOT tied
    /// to any component, so nothing rebuilds when they change. Call this
    /// directly only when you don't need live UI updates (e.g. a
    /// throwaway validation check); for a real form field bound to a
    /// widget, use [`FormField::for_ctx`] instead.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: use_atom(String::new()),
            validators: Vec::new(),
            errors: use_atom(Vec::new()),
            touched: use_atom(false),
        }
    }

    /// Create (or retrieve) a field persisted in component state — the
    /// value/touched/errors survive rebuilds AND writing to them re-dirties
    /// the owning component (so a submit button's disabled state and an
    /// inline error message actually refresh live). Follows the same hook
    /// rules as `ctx.state`/`ScrollController::for_ctx`: call
    /// unconditionally in `build()`, stable order.
    pub fn for_ctx(ctx: &mut rosace_core::Context, name: impl Into<String>) -> Self {
        let field = ctx.state(Self::new(name)).get();
        // The inner atoms are framework-created (`use_atom`) — nothing
        // subscribes to them by default, so a `.set()`/`.validate()` would
        // request a frame that repaints nothing (cache-hit). Subscribing
        // the owning component makes field writes dirty it like `ctx.state`
        // atoms do (duplicate subscribes are ignored).
        let id = ctx.component_id();
        field.value.subscribe(id);
        field.touched.subscribe(id);
        field.errors.subscribe(id);
        field
    }

    pub fn with_value(self, v: impl Into<String>) -> Self {
        self.value.set(v.into());
        self
    }

    pub fn rule(mut self, v: impl Validator) -> Self {
        self.validators.push(Arc::new(v));
        self
    }

    /// Current string value.
    pub fn get(&self) -> String { self.value.get() }

    /// Set the string value and mark the field touched.
    pub fn set(&self, v: impl Into<String>) {
        self.touched.set(true);
        self.value.set(v.into());
    }

    /// Run all validators against the current value, publish the result,
    /// and return whether it passed. `&self`, not `&mut self` — every
    /// clone of this field shares the same underlying atoms, so any
    /// clone can validate and every other clone (and the app's own
    /// `Form`) sees the result immediately.
    pub fn validate(&self) -> bool {
        let val = self.value.get();
        let errs: Vec<FieldError> = self.validators.iter()
            .filter_map(|v| v.validate(&val).map(|msg| FieldError::new(&self.name, msg)))
            .collect();
        let ok = errs.is_empty();
        self.errors.set(errs);
        ok
    }

    /// Current validation errors (from the last `validate()` call).
    pub fn errors(&self) -> Vec<FieldError> { self.errors.get() }

    /// True if the field has no validation errors after the last
    /// `validate()` call. Defaults to `true` before the first
    /// `validate()` — an unvalidated field isn't KNOWN invalid; callers
    /// that need "definitely passes all rules" should call `validate()`
    /// (or rely on Step 8's live-validating `.field()` binding, which
    /// validates on every edit) before trusting this for gating.
    pub fn is_valid(&self) -> bool { self.errors.get().is_empty() }

    /// True if the field has been interacted with (`set()` called at
    /// least once) — the standard "don't show errors until touched"
    /// convention, so a blank required field doesn't show red before the
    /// user has even had a chance to fill it in.
    pub fn is_touched(&self) -> bool { self.touched.get() }

    /// Reset value, errors, and touched state.
    pub fn reset(&self) {
        self.value.set(String::new());
        self.errors.set(Vec::new());
        self.touched.set(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validator::{Required, MinLength};

    #[test]
    fn form_field_new_empty() {
        let f = FormField::new("username");
        assert_eq!(f.name, "username");
        assert_eq!(f.get(), "");
        assert!(!f.is_touched());
        assert!(f.errors().is_empty());
    }

    #[test]
    fn form_field_set_marks_touched() {
        let f = FormField::new("username");
        f.set("alice");
        assert!(f.is_touched());
        assert_eq!(f.get(), "alice");
    }

    #[test]
    fn form_field_validate_no_rules_passes() {
        let f = FormField::new("bio");
        assert!(f.validate());
        assert!(f.is_valid());
    }

    #[test]
    fn form_field_validate_required_fails_empty() {
        let f = FormField::new("name").rule(Required);
        assert!(!f.validate());
        assert!(!f.is_valid());
    }

    #[test]
    fn form_field_validate_passes_with_value() {
        let f = FormField::new("name").rule(Required);
        f.set("alice");
        assert!(f.validate());
        assert!(f.is_valid());
    }

    #[test]
    fn form_field_multiple_rules_all_checked() {
        let f = FormField::new("name").rule(Required).rule(MinLength(5));
        f.set("ab");
        assert!(!f.validate());
        // Only MinLength fails (Required passes since "ab" is non-empty)
        assert_eq!(f.errors().len(), 1);
        assert!(f.errors()[0].message.contains("5 characters"));
    }

    #[test]
    fn form_field_errors_after_validate() {
        let f = FormField::new("email").rule(Required);
        f.validate();
        assert!(!f.errors().is_empty());
        assert_eq!(f.errors()[0].field, "email");
    }

    #[test]
    fn form_field_reset_clears() {
        let f = FormField::new("name").rule(Required);
        f.set("alice");
        f.validate();
        f.reset();
        assert_eq!(f.get(), "");
        assert!(!f.is_touched());
        assert!(f.errors().is_empty());
    }

    #[test]
    fn form_field_with_value() {
        let f = FormField::new("city").with_value("London");
        assert_eq!(f.get(), "London");
    }

    #[test]
    fn cloning_a_field_shares_the_same_live_state() {
        // The whole point of the atom-backed redesign (D116 Step 8): a
        // clone handed to a widget and the original kept by the app must
        // see each other's writes.
        let original = FormField::new("name").rule(Required);
        let widget_copy = original.clone();
        widget_copy.set("alice");
        assert_eq!(original.get(), "alice", "a clone's write must be visible through the original handle");
        assert!(original.is_touched());
        original.validate();
        assert!(widget_copy.is_valid(), "a clone must see validation results run through a DIFFERENT clone");
    }
}
