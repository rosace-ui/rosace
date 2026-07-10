use crate::error::MediaError;
use crate::format::VideoFormat;

/// A single decoded video frame (RGBA).
#[derive(Debug, Clone)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    /// RGBA pixel data, row-major.
    pub data: Vec<u8>,
    pub timestamp_ms: u64,
    pub frame_index: u64,
}

impl VideoFrame {
    pub fn new(width: u32, height: u32, timestamp_ms: u64) -> Self {
        Self {
            width,
            height,
            data: vec![0u8; (width * height * 4) as usize],
            timestamp_ms,
            frame_index: 0,
        }
    }

    pub fn pixel_count(&self) -> usize { (self.width * self.height) as usize }
    pub fn byte_count(&self) -> usize { self.data.len() }
}

/// Video stream decoder (stub — always returns PlatformUnavailable).
#[derive(Debug)]
pub struct VideoDecoder {
    pub format: VideoFormat,
    pub width: u32,
    pub height: u32,
    pub fps: f32,
    frame_count: u64,
}

impl VideoDecoder {
    /// Open a video file. Always returns `Err(PlatformUnavailable)` in this stub.
    pub fn open(path: &str) -> Result<Self, MediaError> {
        let ext = std::path::Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        let _format = VideoFormat::from_extension(ext)
            .ok_or(MediaError::Unsupported)?;
        Err(MediaError::PlatformUnavailable)
    }

    /// Decode the next frame. Always returns `None` in this stub.
    pub fn next_frame(&mut self) -> Option<VideoFrame> {
        None
    }

    pub fn frames_decoded(&self) -> u64 { self.frame_count }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_frame_new() {
        let f = VideoFrame::new(4, 4, 0);
        assert_eq!(f.width, 4);
        assert_eq!(f.height, 4);
        assert_eq!(f.frame_index, 0);
    }

    #[test]
    fn video_frame_pixel_count() {
        let f = VideoFrame::new(10, 5, 0);
        assert_eq!(f.pixel_count(), 50);
    }

    #[test]
    fn video_frame_byte_count() {
        let f = VideoFrame::new(10, 5, 0);
        assert_eq!(f.byte_count(), 200); // 10 * 5 * 4
    }

    #[test]
    fn video_decoder_open_returns_unavailable() {
        let result = VideoDecoder::open("clip.mp4");
        assert_eq!(result.unwrap_err(), MediaError::PlatformUnavailable);
    }

    #[test]
    fn video_decoder_open_unknown_ext_returns_unsupported() {
        let result = VideoDecoder::open("clip.mkv");
        assert_eq!(result.unwrap_err(), MediaError::Unsupported);
    }

    #[test]
    fn video_frame_data_size_correct() {
        let f = VideoFrame::new(8, 8, 0);
        assert_eq!(f.data.len(), 8 * 8 * 4);
        assert!(f.data.iter().all(|&b| b == 0));
    }

    #[test]
    fn video_frame_timestamp() {
        let f = VideoFrame::new(2, 2, 1000);
        assert_eq!(f.timestamp_ms, 1000);
    }

    #[test]
    fn video_frame_width_height() {
        let f = VideoFrame::new(1920, 1080, 0);
        assert_eq!(f.width, 1920);
        assert_eq!(f.height, 1080);
    }
}
