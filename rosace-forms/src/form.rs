use crate::error::FieldError;
use crate::field::FormField;

/// A form that aggregates multiple `FormField`s. Since `FormField` clones
/// share their underlying atoms (D116 Phase 28 Step 8), `Form` itself is
/// just a `Vec<FormField>` — no separate reactive plumbing needed at the
/// `Form` level; every method here is `&self` because the fields it holds
/// are shared handles, not owned data.
#[derive(Clone)]
pub struct Form {
    fields: Vec<FormField>,
}

impl Form {
    pub fn new() -> Self { Self { fields: Vec::new() } }

    pub fn field(mut self, f: FormField) -> Self { self.fields.push(f); self }

    pub fn add_field(&mut self, f: FormField) { self.fields.push(f); }

    /// Run validate() on all fields. Returns true only if ALL pass.
    pub fn validate_all(&self) -> bool {
        // Collect into Vec to prevent `all()` from short-circuiting — every
        // field must run its validators so errors are populated for all fields.
        let results: Vec<bool> = self.fields.iter().map(|f| f.validate()).collect();
        results.iter().all(|&v| v)
    }

    /// Collect all errors from all fields.
    pub fn errors(&self) -> Vec<FieldError> {
        self.fields.iter().flat_map(|f| f.errors()).collect()
    }

    /// Get a field by name.
    pub fn field_named(&self, name: &str) -> Option<&FormField> {
        self.fields.iter().find(|f| f.name == name)
    }

    /// True if all fields pass validation (must call validate_all first,
    /// or rely on a live-validating `.field()` binding — see `TextInput`/
    /// `TextArea`'s Step 8 seam — to have kept this current).
    pub fn is_valid(&self) -> bool {
        self.fields.iter().all(|f| f.is_valid())
    }

    /// Reset all fields.
    pub fn reset(&self) {
        self.fields.iter().for_each(|f| f.reset());
    }

    /// Number of fields.
    pub fn len(&self) -> usize { self.fields.len() }
    pub fn is_empty(&self) -> bool { self.fields.is_empty() }

    /// Validate every field; if all pass, run `on_valid` and return
    /// `true`. The natural body of a submit button's `on_press` (D116
    /// Phase 28 Step 8) — `Button::new("Submit").on_press(move || {
    /// form.submit(|| { ... }); })`. Marks every field touched (via
    /// `validate_all`'s own `validate()` calls reading the CURRENT
    /// value — touched status itself comes from `set()`, unaffected
    /// here), so an untouched-but-invalid field's error becomes visible
    /// immediately after a failed submit attempt, not just after the
    /// user happens to edit it.
    pub fn submit(&self, on_valid: impl FnOnce()) -> bool {
        let valid = self.validate_all();
        if valid {
            on_valid();
        }
        valid
    }
}

impl Default for Form { fn default() -> Self { Self::new() } }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::FormField;
    use crate::validator::Required;

    #[test]
    fn form_new_empty() {
        let f = Form::new();
        assert!(f.is_empty());
        assert_eq!(f.len(), 0);
    }

    #[test]
    fn form_add_field() {
        let mut f = Form::new();
        f.add_field(FormField::new("email"));
        assert_eq!(f.len(), 1);
    }

    #[test]
    fn form_validate_all_passes() {
        let form = Form::new()
            .field(FormField::new("name").rule(Required));
        form.field_named("name").unwrap().set("alice");
        assert!(form.validate_all());
    }

    #[test]
    fn form_validate_all_fails() {
        let form = Form::new()
            .field(FormField::new("name").rule(Required));
        assert!(!form.validate_all());
    }

    #[test]
    fn form_errors_returns_all() {
        let form = Form::new()
            .field(FormField::new("name").rule(Required))
            .field(FormField::new("email").rule(Required));
        form.validate_all();
        assert_eq!(form.errors().len(), 2);
    }

    #[test]
    fn form_field_named() {
        let form = Form::new()
            .field(FormField::new("username"));
        assert!(form.field_named("username").is_some());
        assert!(form.field_named("missing").is_none());
    }

    #[test]
    fn form_reset_clears_all() {
        let form = Form::new()
            .field(FormField::new("name").rule(Required));
        form.field_named("name").unwrap().set("alice");
        form.validate_all();
        form.reset();
        assert_eq!(form.field_named("name").unwrap().get(), "");
        assert!(form.errors().is_empty());
    }

    #[test]
    fn form_is_valid_after_validate() {
        let form = Form::new()
            .field(FormField::new("name").rule(Required));
        form.field_named("name").unwrap().set("bob");
        form.validate_all();
        assert!(form.is_valid());
    }

    #[test]
    fn submit_runs_the_callback_only_when_valid() {
        let form = Form::new().field(FormField::new("name").rule(Required));
        let mut ran = false;
        assert!(!form.submit(|| ran = true), "submit must return false when a required field is empty");
        assert!(!ran, "the callback must not run on a failed submit");

        form.field_named("name").unwrap().set("alice");
        let mut ran2 = false;
        assert!(form.submit(|| ran2 = true));
        assert!(ran2, "the callback must run once validation passes");
    }

    #[test]
    fn cloning_a_form_shares_the_same_fields() {
        let form = Form::new().field(FormField::new("name").rule(Required));
        let clone = form.clone();
        clone.field_named("name").unwrap().set("alice");
        assert!(form.field_named("name").unwrap().is_touched(), "a clone's write must be visible through the original Form");
    }
}
