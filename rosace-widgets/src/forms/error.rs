/// A validation error on a named field.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldError {
    pub field: String,
    pub message: String,
}

impl FieldError {
    pub fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self { field: field.into(), message: message.into() }
    }
}

impl std::fmt::Display for FieldError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.field, self.message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_error_new() {
        let e = FieldError::new("email", "invalid");
        assert_eq!(e.field, "email");
        assert_eq!(e.message, "invalid");
    }

    #[test]
    fn field_error_display() {
        let e = FieldError::new("name", "required");
        assert_eq!(format!("{}", e), "name: required");
    }

    #[test]
    fn field_error_clone() {
        let e = FieldError::new("age", "too young");
        let c = e.clone();
        assert_eq!(e, c);
    }
}
