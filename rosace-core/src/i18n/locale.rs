/// A locale identifier like "en", "en-US", "fr-CA", "ja".
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Locale {
    pub language: String,
    pub region: Option<String>,
}

impl Locale {
    pub fn new(language: impl Into<String>) -> Self {
        Self { language: language.into(), region: None }
    }

    pub fn with_region(mut self, region: impl Into<String>) -> Self {
        self.region = Some(region.into());
        self
    }

    /// Parse "en-US" into Locale { language: "en", region: Some("US") }.
    pub fn parse(s: &str) -> Self {
        if let Some((lang, region)) = s.split_once('-') {
            Self { language: lang.to_string(), region: Some(region.to_string()) }
        } else {
            Self { language: s.to_string(), region: None }
        }
    }

    /// BCP 47 string representation ("en" or "en-US").
    #[allow(clippy::inherent_to_string)]
    pub fn to_string(&self) -> String {
        match &self.region {
            Some(r) => format!("{}-{}", self.language, r),
            None    => self.language.clone(),
        }
    }

    pub fn english() -> Self { Self::new("en") }
    pub fn french()  -> Self { Self::new("fr") }
    pub fn spanish() -> Self { Self::new("es") }
    pub fn japanese()-> Self { Self::new("ja") }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_new() {
        let l = Locale::new("en");
        assert_eq!(l.language, "en");
        assert!(l.region.is_none());
    }

    #[test]
    fn locale_with_region() {
        let l = Locale::new("en").with_region("US");
        assert_eq!(l.language, "en");
        assert_eq!(l.region.as_deref(), Some("US"));
    }

    #[test]
    fn locale_parse_lang_only() {
        let l = Locale::parse("ja");
        assert_eq!(l.language, "ja");
        assert!(l.region.is_none());
    }

    #[test]
    fn locale_parse_lang_region() {
        let l = Locale::parse("fr-CA");
        assert_eq!(l.language, "fr");
        assert_eq!(l.region.as_deref(), Some("CA"));
    }

    #[test]
    fn locale_to_string_no_region() {
        let l = Locale::new("es");
        assert_eq!(l.to_string(), "es");
    }

    #[test]
    fn locale_to_string_with_region() {
        let l = Locale::new("en").with_region("GB");
        assert_eq!(l.to_string(), "en-GB");
    }

    #[test]
    fn locale_english() {
        let l = Locale::english();
        assert_eq!(l.language, "en");
        assert!(l.region.is_none());
    }

    #[test]
    fn locale_eq() {
        let a = Locale::parse("fr-CA");
        let b = Locale::new("fr").with_region("CA");
        assert_eq!(a, b);
    }
}
