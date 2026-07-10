/// Audio container/codec format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AudioFormat {
    Wav,
    Mp3,
    Ogg,
    Aac,
    Flac,
}

impl AudioFormat {
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "wav"  => Some(AudioFormat::Wav),
            "mp3"  => Some(AudioFormat::Mp3),
            "ogg"  => Some(AudioFormat::Ogg),
            "aac"  => Some(AudioFormat::Aac),
            "flac" => Some(AudioFormat::Flac),
            _      => None,
        }
    }

    pub fn mime_type(&self) -> &'static str {
        match self {
            AudioFormat::Wav  => "audio/wav",
            AudioFormat::Mp3  => "audio/mpeg",
            AudioFormat::Ogg  => "audio/ogg",
            AudioFormat::Aac  => "audio/aac",
            AudioFormat::Flac => "audio/flac",
        }
    }
}

/// Video container format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VideoFormat {
    Mp4,
    Webm,
    Gif,
    Avi,
}

impl VideoFormat {
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "mp4"  => Some(VideoFormat::Mp4),
            "webm" => Some(VideoFormat::Webm),
            "gif"  => Some(VideoFormat::Gif),
            "avi"  => Some(VideoFormat::Avi),
            _      => None,
        }
    }

    pub fn mime_type(&self) -> &'static str {
        match self {
            VideoFormat::Mp4  => "video/mp4",
            VideoFormat::Webm => "video/webm",
            VideoFormat::Gif  => "image/gif",
            VideoFormat::Avi  => "video/avi",
        }
    }

    pub fn supports_transparency(&self) -> bool {
        matches!(self, VideoFormat::Webm | VideoFormat::Gif)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_format_from_extension_wav() {
        assert_eq!(AudioFormat::from_extension("wav"), Some(AudioFormat::Wav));
    }

    #[test]
    fn audio_format_from_extension_mp3() {
        assert_eq!(AudioFormat::from_extension("mp3"), Some(AudioFormat::Mp3));
    }

    #[test]
    fn audio_format_from_extension_unknown() {
        assert_eq!(AudioFormat::from_extension("xyz"), None);
    }

    #[test]
    fn audio_format_mime_wav() {
        assert_eq!(AudioFormat::Wav.mime_type(), "audio/wav");
    }

    #[test]
    fn audio_format_mime_mp3() {
        assert_eq!(AudioFormat::Mp3.mime_type(), "audio/mpeg");
    }

    #[test]
    fn video_format_from_extension_mp4() {
        assert_eq!(VideoFormat::from_extension("mp4"), Some(VideoFormat::Mp4));
    }

    #[test]
    fn video_format_from_extension_gif() {
        assert_eq!(VideoFormat::from_extension("gif"), Some(VideoFormat::Gif));
    }

    #[test]
    fn video_format_from_extension_unknown() {
        assert_eq!(VideoFormat::from_extension("mkv"), None);
    }

    #[test]
    fn video_format_supports_transparency_webm() {
        assert!(VideoFormat::Webm.supports_transparency());
        assert!(VideoFormat::Gif.supports_transparency());
    }

    #[test]
    fn video_format_no_transparency_mp4() {
        assert!(!VideoFormat::Mp4.supports_transparency());
        assert!(!VideoFormat::Avi.supports_transparency());
    }
}
