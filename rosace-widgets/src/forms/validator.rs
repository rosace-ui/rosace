/// A validation rule that can be applied to a string value.
pub trait Validator: Send + Sync + 'static {
    /// Returns `None` if valid, or an error message string if invalid.
    fn validate(&self, value: &str) -> Option<String>;

    /// Human-readable name for this rule (used in error messages).
    fn name(&self) -> &'static str;
}

/// Field value must be non-empty (after trimming).
pub struct Required;
impl Validator for Required {
    fn validate(&self, value: &str) -> Option<String> {
        if value.trim().is_empty() { Some("This field is required.".into()) } else { None }
    }
    fn name(&self) -> &'static str { "Required" }
}

/// Minimum character length.
pub struct MinLength(pub usize);
impl Validator for MinLength {
    fn validate(&self, value: &str) -> Option<String> {
        if value.len() < self.0 { Some(format!("Must be at least {} characters.", self.0)) } else { None }
    }
    fn name(&self) -> &'static str { "MinLength" }
}

/// Maximum character length.
pub struct MaxLength(pub usize);
impl Validator for MaxLength {
    fn validate(&self, value: &str) -> Option<String> {
        if value.len() > self.0 { Some(format!("Must be no more than {} characters.", self.0)) } else { None }
    }
    fn name(&self) -> &'static str { "MaxLength" }
}

/// Simple pattern check (contains substring — no regex dep needed).
pub struct Contains(pub &'static str);
impl Validator for Contains {
    fn validate(&self, value: &str) -> Option<String> {
        if !value.contains(self.0) { Some(format!("Must contain '{}'.", self.0)) } else { None }
    }
    fn name(&self) -> &'static str { "Contains" }
}

/// Email-like validation: must contain @ and a dot after @.
pub struct Email;
impl Validator for Email {
    fn validate(&self, value: &str) -> Option<String> {
        let valid = value.contains('@') && value.split('@').nth(1).map(|d| d.contains('.')).unwrap_or(false);
        if valid { None } else { Some("Must be a valid email address.".into()) }
    }
    fn name(&self) -> &'static str { "Email" }
}

/// Numeric range validator for string-encoded numbers.
pub struct Range {
    pub min: f64,
    pub max: f64,
}
impl Range {
    pub fn new(min: f64, max: f64) -> Self { Self { min, max } }
}
impl Validator for Range {
    fn validate(&self, value: &str) -> Option<String> {
        match value.trim().parse::<f64>() {
            Err(_) => Some("Must be a number.".into()),
            Ok(n) if n < self.min => Some(format!("Must be at least {}.", self.min)),
            Ok(n) if n > self.max => Some(format!("Must be no more than {}.", self.max)),
            Ok(_) => None,
        }
    }
    fn name(&self) -> &'static str { "Range" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_rejects_empty() {
        assert!(Required.validate("").is_some());
    }

    #[test]
    fn required_rejects_whitespace() {
        assert!(Required.validate("   ").is_some());
    }

    #[test]
    fn required_accepts_non_empty() {
        assert!(Required.validate("hello").is_none());
    }

    #[test]
    fn min_length_rejects_short() {
        assert!(MinLength(5).validate("abc").is_some());
    }

    #[test]
    fn min_length_accepts_exact() {
        assert!(MinLength(3).validate("abc").is_none());
    }

    #[test]
    fn max_length_rejects_long() {
        assert!(MaxLength(3).validate("abcd").is_some());
    }

    #[test]
    fn max_length_accepts_exact() {
        assert!(MaxLength(3).validate("abc").is_none());
    }

    #[test]
    fn contains_rejects_missing() {
        assert!(Contains("@").validate("nodomain").is_some());
    }

    #[test]
    fn contains_accepts_present() {
        assert!(Contains("@").validate("user@example.com").is_none());
    }

    #[test]
    fn email_rejects_no_at() {
        assert!(Email.validate("nodomain.com").is_some());
    }

    #[test]
    fn email_rejects_no_dot_after_at() {
        assert!(Email.validate("user@nodot").is_some());
    }

    #[test]
    fn email_accepts_valid() {
        assert!(Email.validate("user@example.com").is_none());
    }

    #[test]
    fn range_rejects_below() {
        assert!(Range::new(1.0, 10.0).validate("0").is_some());
    }

    #[test]
    fn range_rejects_above() {
        assert!(Range::new(1.0, 10.0).validate("11").is_some());
    }

    #[test]
    fn range_rejects_non_numeric() {
        assert!(Range::new(1.0, 10.0).validate("abc").is_some());
    }

    #[test]
    fn range_accepts_within() {
        assert!(Range::new(1.0, 10.0).validate("5").is_none());
    }
}
