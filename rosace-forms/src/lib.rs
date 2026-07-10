//! Form state management for ROSACE.
//!
//! # Example
//! ```rust,ignore
//! use rosace_forms::{Form, FormField, Required, Email};
//!
//! let mut form = Form::new()
//!     .field(FormField::new("email").rule(Required).rule(Email))
//!     .field(FormField::new("name").rule(Required).rule(MinLength(2)));
//!
//! form.field_named_mut("email").unwrap().set("not-an-email");
//! assert!(!form.validate_all());
//! assert!(!form.errors().is_empty());
//! ```

pub mod error;
pub mod field;
pub mod form;
pub mod validator;

pub use error::FieldError;
pub use field::FormField;
pub use form::Form;
pub use validator::{Contains, Email, MaxLength, MinLength, Range, Required, Validator};
