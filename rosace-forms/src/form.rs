use crate::error::FieldError;
use crate::field::FormField;

/// A form that aggregates multiple FormFields.
pub struct Form {
    fields: Vec<FormField>,
}

impl Form {
    pub fn new() -> Self { Self { fields: Vec::new() } }

    pub fn field(mut self, f: FormField) -> Self { self.fields.push(f); self }

    pub fn add_field(&mut self, f: FormField) { self.fields.push(f); }

    /// Run validate() on all fields. Returns true only if ALL pass.
    pub fn validate_all(&mut self) -> bool {
        // Collect into Vec to prevent `all()` from short-circuiting — every
        // field must run its validators so errors are populated for all fields.
        let results: Vec<bool> = self.fields.iter_mut().map(|f| f.validate()).collect();
        results.iter().all(|&v| v)
    }

    /// Collect all errors from all fields.
    pub fn errors(&self) -> Vec<&FieldError> {
        self.fields.iter().flat_map(|f| f.errors()).collect()
    }

    /// Get a field by name.
    pub fn field_named(&self, name: &str) -> Option<&FormField> {
        self.fields.iter().find(|f| f.name == name)
    }

    pub fn field_named_mut(&mut self, name: &str) -> Option<&mut FormField> {
        self.fields.iter_mut().find(|f| f.name == name)
    }

    /// True if all fields pass validation (must call validate_all first).
    pub fn is_valid(&self) -> bool {
        self.fields.iter().all(|f| f.is_valid())
    }

    /// Reset all fields.
    pub fn reset(&mut self) {
        self.fields.iter_mut().for_each(|f| f.reset());
    }

    /// Number of fields.
    pub fn len(&self) -> usize { self.fields.len() }
    pub fn is_empty(&self) -> bool { self.fields.is_empty() }
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
        let mut form = Form::new()
            .field(FormField::new("name").rule(Required));
        form.field_named_mut("name").unwrap().set("alice");
        assert!(form.validate_all());
    }

    #[test]
    fn form_validate_all_fails() {
        let mut form = Form::new()
            .field(FormField::new("name").rule(Required));
        assert!(!form.validate_all());
    }

    #[test]
    fn form_errors_returns_all() {
        let mut form = Form::new()
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
        let mut form = Form::new()
            .field(FormField::new("name").rule(Required));
        form.field_named_mut("name").unwrap().set("alice");
        form.validate_all();
        form.reset();
        assert_eq!(form.field_named("name").unwrap().get(), "");
        assert!(form.errors().is_empty());
    }

    #[test]
    fn form_is_valid_after_validate() {
        let mut form = Form::new()
            .field(FormField::new("name").rule(Required));
        form.field_named_mut("name").unwrap().set("bob");
        form.validate_all();
        assert!(form.is_valid());
    }
}
