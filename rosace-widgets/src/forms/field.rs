use std::sync::{Arc, Mutex};
use crate::forms::validator::Validator;
use crate::forms::error::FieldError;

/// A single form field: a name, a shared string value, and validation
/// rules. Cloning a `FormField` is cheap and shares state (D116 Phase 28
/// Step 8) — every clone reads/writes the SAME underlying atoms, the same
/// "clone shares identity" convention `EditController`/`ScrollController`
/// already use in this codebase. This is what lets `TextInput::field(f)`,
/// `Form::field(f)`, and the app's own submit-button closure all see the
/// same live value/touched/errors without any manual synchronization.
/// The mutable half of a field, shared by every clone.
///
/// This was three `Atom`s, each subscribing the owning COMPONENT — and there
/// is exactly one component in the tree, so every keystroke dirtied the root,
/// re-ran `build()`, and made the frame structural, which disables every
/// per-node cache in the framework. Typing is about as continuous as
/// interaction gets, so that was the worst possible place for it.
#[derive(Default)]
struct FieldState {
    value: String,
    /// Last validation errors (populated by `validate()`).
    errors: Vec<FieldError>,
    /// Whether the field has been interacted with (touched = show errors).
    touched: bool,
    /// Repaint hook, installed by [`FormField::bind`] — marks the owning
    /// NODE, not a component.
    on_invalidate: Option<Arc<dyn Fn() + Send + Sync>>,
    /// App-facing notification, installed by [`FormField::on_change`].
    on_change: Option<Arc<dyn Fn() + Send + Sync>>,
}

#[derive(Clone)]
pub struct FormField {
    pub name: String,
    validators: Vec<Arc<dyn Validator>>,
    state: Arc<Mutex<FieldState>>,
}

impl FormField {
    /// A field with no repaint hook attached yet — writing to it changes the
    /// value but schedules nothing. Bound to a widget by [`Self::bind`],
    /// which `TextInput::field` does for you.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            validators: Vec::new(),
            state: Arc::new(Mutex::new(FieldState::default())),
        }
    }

    fn s(&self) -> std::sync::MutexGuard<'_, FieldState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Called whenever this field's value, touched flag or errors change.
    ///
    /// For dependencies that cross widget boundaries — a submit button that
    /// gates on `form.is_valid()` lives somewhere else in the tree, and
    /// marking the FIELD's node cannot repaint it. The field reports; the app
    /// decides what that means (typically writing its own `ctx.state`, which
    /// rebuilds).
    ///
    /// Field-local rendering does not need this: the input repaints itself
    /// through [`Self::bind`].
    pub fn on_change(&self, f: impl Fn() + Send + Sync + 'static) {
        self.s().on_change = Some(Arc::new(f));
    }

    /// Repaint this NODE when the field changes.
    ///
    /// Replaces subscribing the owning component: a field write now marks one
    /// node and the frame stays targeted, instead of rebuilding the app on
    /// every keystroke.
    pub fn bind(&self, f: impl Fn() + Send + Sync + 'static) {
        self.s().on_invalidate = Some(Arc::new(f));
    }

    fn invalidate(&self) {
        // Both cloned out and called OUTSIDE the lock — a handler that reads
        // the field back would otherwise deadlock on its own notification.
        let (repaint, notify) = {
            let st = self.s();
            (st.on_invalidate.clone(), st.on_change.clone())
        };
        if let Some(f) = repaint { f(); }
        if let Some(f) = notify { f(); }
    }

    pub fn with_value(self, v: impl Into<String>) -> Self {
        { self.s().value = v.into(); self.invalidate(); }
        self
    }

    pub fn rule(mut self, v: impl Validator) -> Self {
        self.validators.push(Arc::new(v));
        self
    }

    /// Current string value.
    pub fn get(&self) -> String { self.s().value.clone() }

    /// Set the string value and mark the field touched.
    pub fn set(&self, v: impl Into<String>) {
        { self.s().touched = true; self.invalidate(); }
        { self.s().value = v.into(); self.invalidate(); }
    }

    /// Run all validators against the current value, publish the result,
    /// and return whether it passed. `&self`, not `&mut self` — every
    /// clone of this field shares the same underlying atoms, so any
    /// clone can validate and every other clone (and the app's own
    /// `Form`) sees the result immediately.
    pub fn validate(&self) -> bool {
        let val = self.s().value.clone();
        let errs: Vec<FieldError> = self.validators.iter()
            .filter_map(|v| v.validate(&val).map(|msg| FieldError::new(&self.name, msg)))
            .collect();
        let ok = errs.is_empty();
        { self.s().errors = errs; self.invalidate(); }
        ok
    }

    /// Current validation errors (from the last `validate()` call).
    pub fn errors(&self) -> Vec<FieldError> { self.s().errors.clone() }

    /// True if the field has no validation errors after the last
    /// `validate()` call. Defaults to `true` before the first
    /// `validate()` — an unvalidated field isn't KNOWN invalid; callers
    /// that need "definitely passes all rules" should call `validate()`
    /// (or rely on Step 8's live-validating `.field()` binding, which
    /// validates on every edit) before trusting this for gating.
    pub fn is_valid(&self) -> bool { self.s().errors.clone().is_empty() }

    /// True if the field has been interacted with (`set()` called at
    /// least once) — the standard "don't show errors until touched"
    /// convention, so a blank required field doesn't show red before the
    /// user has even had a chance to fill it in.
    pub fn is_touched(&self) -> bool { self.s().touched.clone() }

    /// Reset value, errors, and touched state.
    pub fn reset(&self) {
        { self.s().value = String::new(); self.invalidate(); }
        { self.s().errors = Vec::new(); self.invalidate(); }
        { self.s().touched = false; self.invalidate(); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forms::validator::{Required, MinLength};

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
