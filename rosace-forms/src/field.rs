use std::sync::Arc;
use rosace_state::use_atom;
use crate::validator::Validator;
use crate::error::FieldError;

/// A single form field with a name, a string value atom, and validation rules.
pub struct FormField {
    pub name: String,
    value: rosace_state::Atom<String>,
    validators: Vec<Arc<dyn Validator>>,
    /// Last validation errors (populated after validate() is called).
    errors: Vec<FieldError>,
    /// Whether the field has been interacted with (touched = show errors).
    pub touched: bool,
}

impl FormField {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: use_atom(String::new()),
            validators: Vec::new(),
            errors: Vec::new(),
            touched: false,
        }
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

    /// Set the string value (marks field as touched).
    pub fn set(&mut self, v: impl Into<String>) {
        self.touched = true;
        self.value.set(v.into());
    }

    /// Run all validators. Returns true if valid.
    pub fn validate(&mut self) -> bool {
        let val = self.value.get();
        self.errors = self.validators.iter()
            .filter_map(|v| v.validate(&val).map(|msg| FieldError::new(&self.name, msg)))
            .collect();
        self.errors.is_empty()
    }

    /// Current validation errors.
    pub fn errors(&self) -> &[FieldError] { &self.errors }

    /// True if field has no validation errors after last validate() call.
    pub fn is_valid(&self) -> bool { self.errors.is_empty() }

    /// True if the field has been interacted with.
    pub fn is_touched(&self) -> bool { self.touched }

    /// Reset value and errors.
    pub fn reset(&mut self) {
        self.value.set(String::new());
        self.errors.clear();
        self.touched = false;
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
        let mut f = FormField::new("username");
        f.set("alice");
        assert!(f.is_touched());
        assert_eq!(f.get(), "alice");
    }

    #[test]
    fn form_field_validate_no_rules_passes() {
        let mut f = FormField::new("bio");
        assert!(f.validate());
        assert!(f.is_valid());
    }

    #[test]
    fn form_field_validate_required_fails_empty() {
        let mut f = FormField::new("name").rule(Required);
        assert!(!f.validate());
        assert!(!f.is_valid());
    }

    #[test]
    fn form_field_validate_passes_with_value() {
        let mut f = FormField::new("name").rule(Required);
        f.set("alice");
        assert!(f.validate());
        assert!(f.is_valid());
    }

    #[test]
    fn form_field_multiple_rules_all_checked() {
        let mut f = FormField::new("name").rule(Required).rule(MinLength(5));
        f.set("ab");
        assert!(!f.validate());
        // Only MinLength fails (Required passes since "ab" is non-empty)
        assert_eq!(f.errors().len(), 1);
        assert!(f.errors()[0].message.contains("5 characters"));
    }

    #[test]
    fn form_field_errors_after_validate() {
        let mut f = FormField::new("email").rule(Required);
        f.validate();
        assert!(!f.errors().is_empty());
        assert_eq!(f.errors()[0].field, "email");
    }

    #[test]
    fn form_field_reset_clears() {
        let mut f = FormField::new("name").rule(Required);
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
}
