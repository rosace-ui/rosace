#[derive(Debug, Clone, PartialEq)]
pub enum MediaError {
    /// Feature not yet implemented (v1.0).
    PlatformUnavailable,
    /// File or stream not found.
    NotFound(String),
    /// Decode failed.
    DecodeFailed(String),
    /// Format not supported.
    Unsupported,
    /// Invalid data.
    InvalidData(String),
}

impl std::fmt::Display for MediaError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            MediaError::PlatformUnavailable   => write!(f, "media: platform unavailable (v1.0)"),
            MediaError::NotFound(p)           => write!(f, "media: not found: {p}"),
            MediaError::DecodeFailed(r)       => write!(f, "media: decode failed: {r}"),
            MediaError::Unsupported           => write!(f, "media: format unsupported"),
            MediaError::InvalidData(d)        => write!(f, "media: invalid data: {d}"),
        }
    }
}

impl std::error::Error for MediaError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    #[test]
    fn media_error_display_unavailable() {
        let e = MediaError::PlatformUnavailable;
        assert_eq!(e.to_string(), "media: platform unavailable (v1.0)");
    }

    #[test]
    fn media_error_display_not_found() {
        let e = MediaError::NotFound("music.mp3".to_string());
        assert_eq!(e.to_string(), "media: not found: music.mp3");
    }

    #[test]
    fn media_error_display_decode_failed() {
        let e = MediaError::DecodeFailed("corrupt header".to_string());
        assert_eq!(e.to_string(), "media: decode failed: corrupt header");
    }

    #[test]
    fn media_error_display_unsupported() {
        let e = MediaError::Unsupported;
        assert_eq!(e.to_string(), "media: format unsupported");
    }

    #[test]
    fn media_error_clone_eq() {
        let a = MediaError::NotFound("x".to_string());
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn media_error_is_error_trait() {
        let e: &dyn Error = &MediaError::PlatformUnavailable;
        assert_eq!(e.to_string(), "media: platform unavailable (v1.0)");
    }
}
