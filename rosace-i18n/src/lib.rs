//! Localization (i18n) support for ROSACE.
//!
//! # Example
//! ```rust,ignore
//! use rosace_i18n::{MessageBundle, Locale, set_locale, t};
//!
//! let bundle = MessageBundle::from_str(Locale::french(), "
//! greeting = Bonjour
//! farewell = Au revoir
//! ");
//! set_locale(bundle);
//! assert_eq!(t("greeting"), "Bonjour");
//! assert_eq!(t("missing_key"), "missing_key"); // graceful fallback
//! ```

pub mod bundle;
pub mod locale;
pub mod provider;

pub use bundle::MessageBundle;
pub use locale::Locale;
pub use provider::{clear, current_locale, set_locale, t};
