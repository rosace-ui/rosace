use std::sync::Mutex;
use crate::bundle::MessageBundle;
use crate::locale::Locale;

static CURRENT_BUNDLE: Mutex<Option<MessageBundle>> = Mutex::new(None);

/// Set the active message bundle.
pub fn set_locale(bundle: MessageBundle) {
    *CURRENT_BUNDLE.lock().unwrap() = Some(bundle);
}

/// Get a translated string for `key` from the active bundle.
/// Falls back to returning `key` if no bundle is set or key is missing.
pub fn t(key: &str) -> String {
    let guard = CURRENT_BUNDLE.lock().unwrap();
    match &*guard {
        Some(bundle) => bundle.get(key).to_string(),
        None => key.to_string(),
    }
}

/// Current locale, or None if no bundle is loaded.
pub fn current_locale() -> Option<Locale> {
    CURRENT_BUNDLE.lock().unwrap().as_ref().map(|b| b.locale.clone())
}

/// Clear the active bundle (reset to no translation).
pub fn clear() {
    *CURRENT_BUNDLE.lock().unwrap() = None;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
// NOTE: Run with --test-threads=1 to avoid global-state races between tests.

#[cfg(test)]
mod tests {
    /// The provider stores the locale in a process-global — parallel
    /// test threads race on it. Every test takes this lock first.
    static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    use super::*;
    use crate::bundle::MessageBundle;
    use crate::locale::Locale;

    fn make_bundle_en() -> MessageBundle {
        MessageBundle::from_str(
            Locale::english(),
            "prov_greeting = Hello\nprov_farewell = Goodbye\nprov_extra = Extra",
        )
    }

    fn make_bundle_fr() -> MessageBundle {
        MessageBundle::from_str(
            Locale::french(),
            "prov_greeting = Bonjour\nprov_farewell = Au revoir",
        )
    }

    #[test]
    fn t_returns_key_when_no_bundle() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear();
        assert_eq!(t("prov_no_bundle_key"), "prov_no_bundle_key");
    }

    #[test]
    fn t_returns_translation_when_bundle_set() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear();
        set_locale(make_bundle_en());
        assert_eq!(t("prov_greeting"), "Hello");
        clear();
    }

    #[test]
    fn t_falls_back_to_key_when_missing() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear();
        set_locale(make_bundle_en());
        assert_eq!(t("prov_unknown_xyz"), "prov_unknown_xyz");
        clear();
    }

    #[test]
    fn set_locale_replaces_bundle() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear();
        set_locale(make_bundle_en());
        assert_eq!(t("prov_greeting"), "Hello");
        set_locale(make_bundle_fr());
        assert_eq!(t("prov_greeting"), "Bonjour");
        clear();
    }

    #[test]
    fn current_locale_none_when_no_bundle() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear();
        assert!(current_locale().is_none());
    }

    #[test]
    fn current_locale_returns_locale_when_set() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear();
        set_locale(make_bundle_en());
        let loc = current_locale();
        assert!(loc.is_some());
        assert_eq!(loc.unwrap().language, "en");
        clear();
    }

    #[test]
    fn clear_removes_bundle() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_locale(make_bundle_en());
        clear();
        assert!(current_locale().is_none());
    }

    #[test]
    fn t_after_clear_returns_key() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_locale(make_bundle_en());
        clear();
        assert_eq!(t("prov_greeting"), "prov_greeting");
    }

    #[test]
    fn bundle_locale_stored() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear();
        set_locale(make_bundle_fr());
        let loc = current_locale().unwrap();
        assert_eq!(loc.language, "fr");
        clear();
    }

    #[test]
    fn t_multiple_keys() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear();
        set_locale(make_bundle_en());
        assert_eq!(t("prov_greeting"), "Hello");
        assert_eq!(t("prov_farewell"), "Goodbye");
        assert_eq!(t("prov_extra"), "Extra");
        clear();
    }
}
