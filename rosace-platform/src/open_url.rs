//! Hand a URL to the OS.
//!
//! Small, but it is the difference between a feature working and half of one:
//! the framework offers apps a way to add a "Search the web for…" item to the
//! text context menu, and without this there is no way to open the result.
//!
//! Deliberately the OS's default handler rather than an in-app browser — the
//! platform already knows which application owns `https:`, `mailto:` or a
//! custom scheme, and every platform's convention is to respect that.

/// Open `url` with the OS's default handler for its scheme.
///
/// Returns `false` when the platform has no handler, the URL is rejected, or
/// the target has no way to open one at all. Callers should treat a `false`
/// as "nothing happened" rather than an error worth interrupting the user
/// over — there is no meaningful recovery, and the common cause is a
/// malformed URL the app built.
///
/// Rejects anything that is not obviously a URL before handing it to a shell,
/// so a string that came from a text selection cannot smuggle arguments into
/// the command that opens it.
pub fn open_url(url: &str) -> bool {
    if !is_safe_url(url) {
        return false;
    }
    open_platform(url)
}

/// Only absolute URLs with a plausible scheme, and no characters that a shell
/// or argument parser could read as anything but part of the URL.
///
/// The strings reaching this come from user selections and app-built queries,
/// so "looks like a URL" is a security boundary, not a nicety: `open` and
/// `xdg-open` take a positional argument, and one starting with `-` becomes a
/// flag. Percent-encoding is the caller's job — see `encode_query`.
fn is_safe_url(url: &str) -> bool {
    let Some((scheme, rest)) = url.split_once(':') else { return false };
    if rest.is_empty() || scheme.is_empty() {
        return false;
    }
    if !scheme.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')) {
        return false;
    }
    // A scheme must start with a letter, which also rules out a leading `-`.
    if !scheme.starts_with(|c: char| c.is_ascii_alphabetic()) {
        return false;
    }
    // Control characters, spaces and quotes have no business in a URL that has
    // already been encoded, and are exactly what an injection would need.
    !url.chars().any(|c| c.is_control() || c.is_whitespace() || matches!(c, '"' | '\'' | '\\'))
}

/// Percent-encode `text` for use inside a query string.
///
/// Provided because the motivating case — building a search URL out of a text
/// selection — is otherwise an invitation for every app to hand-roll it and
/// get `&`, `#` or a space wrong.
pub fn encode_query(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(target_os = "macos")]
fn open_platform(url: &str) -> bool {
    std::process::Command::new("/usr/bin/open")
        .arg("--")
        .arg(url)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(target_os = "linux")]
fn open_platform(url: &str) -> bool {
    std::process::Command::new("xdg-open")
        .arg(url)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(target_os = "windows")]
fn open_platform(url: &str) -> bool {
    // `start` is a shell builtin, and its first quoted argument is the window
    // TITLE — hence the empty one before the URL.
    std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(target_arch = "wasm32")]
fn open_platform(url: &str) -> bool {
    // A popup blocker may refuse this when it is not driven by a user gesture,
    // which is reported honestly rather than swallowed.
    web_sys::window()
        .and_then(|w| w.open_with_url_and_target(url, "_blank").ok().flatten())
        .is_some()
}

/// iOS and Android open URLs through their own host application object, which
/// this crate has no handle on — the FFI host does. Reported as "not opened"
/// rather than silently succeeding.
#[cfg(not(any(
    target_os = "macos",
    target_os = "linux",
    target_os = "windows",
    target_arch = "wasm32",
)))]
fn open_platform(_url: &str) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_https_urls_are_accepted() {
        assert!(is_safe_url("https://example.com/search?q=hello"));
        assert!(is_safe_url("mailto:someone@example.com"));
        assert!(is_safe_url("myapp+scheme://open/thing"));
    }

    /// The strings reaching `open_url` come from text selections, so anything
    /// a shell could read as an argument has to be refused BEFORE it is
    /// handed to `open`/`xdg-open`.
    #[test]
    fn argument_injection_shapes_are_rejected() {
        assert!(!is_safe_url("-flag"), "a leading dash would become a flag");
        assert!(!is_safe_url("--version"));
        assert!(!is_safe_url("https://example.com/a b"), "unencoded space");
        assert!(!is_safe_url("https://example.com/\"x\""), "quote");
        assert!(!is_safe_url("https://example.com/\nx"), "control character");
        assert!(!is_safe_url("no-scheme-at-all"));
        assert!(!is_safe_url("https:"), "scheme with nothing after it");
        assert!(!is_safe_url(""));
    }

    #[test]
    fn queries_are_percent_encoded() {
        assert_eq!(encode_query("hello world"), "hello+world");
        assert_eq!(encode_query("a&b=c#d"), "a%26b%3Dc%23d");
        assert_eq!(encode_query("caf\u{e9}"), "caf%C3%A9");
        assert_eq!(encode_query("safe-._~"), "safe-._~");
    }

    /// The encoder's whole point is producing something the validator accepts.
    #[test]
    fn an_encoded_selection_survives_validation() {
        let url = format!(
            "https://www.google.com/search?q={}",
            encode_query("rust \"lifetime\" & borrow -check")
        );
        assert!(is_safe_url(&url), "{url}");
    }
}
