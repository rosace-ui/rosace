use crate::error::MediaError;
use crate::format::AudioFormat;

/// A loaded audio resource (stub — no actual audio data).
#[derive(Debug, Clone)]
pub struct AudioHandle {
    pub id: u64,
    pub format: AudioFormat,
    pub duration_secs: f32,
    pub playing: bool,
    pub volume: f32,
    pub looping: bool,
}

impl AudioHandle {
    pub fn new(id: u64, format: AudioFormat, duration_secs: f32) -> Self {
        Self { id, format, duration_secs, playing: false, volume: 1.0, looping: false }
    }

    pub fn play(&mut self)  { self.playing = true; }
    pub fn pause(&mut self) { self.playing = false; }
    pub fn stop(&mut self)  { self.playing = false; }

    pub fn volume(mut self, v: f32) -> Self {
        self.volume = v.clamp(0.0, 1.0);
        self
    }

    pub fn looping(mut self, l: bool) -> Self { self.looping = l; self }
}

/// Audio playback controller (stub — all operations return PlatformUnavailable).
#[derive(Debug, Default)]
pub struct AudioPlayer {
    next_id: u64,
}

impl AudioPlayer {
    pub fn new() -> Self { Self { next_id: 1 } }

    /// Load an audio file. Always returns `Err(PlatformUnavailable)` in this stub.
    pub fn load(&mut self, path: &str) -> Result<AudioHandle, MediaError> {
        // Detect format from extension
        let ext = std::path::Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        let format = crate::format::AudioFormat::from_extension(ext)
            .ok_or(MediaError::Unsupported)?;
        let _ = format;
        Err(MediaError::PlatformUnavailable)
    }

    /// Number of loads attempted.
    pub fn load_count(&self) -> u64 { self.next_id - 1 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_handle_new() {
        let h = AudioHandle::new(1, AudioFormat::Mp3, 120.0);
        assert_eq!(h.id, 1);
        assert_eq!(h.format, AudioFormat::Mp3);
        assert_eq!(h.duration_secs, 120.0);
        assert!(!h.playing);
        assert_eq!(h.volume, 1.0);
        assert!(!h.looping);
    }

    #[test]
    fn audio_handle_play_sets_playing() {
        let mut h = AudioHandle::new(2, AudioFormat::Wav, 30.0);
        h.play();
        assert!(h.playing);
    }

    #[test]
    fn audio_handle_pause_stops_playing() {
        let mut h = AudioHandle::new(3, AudioFormat::Wav, 30.0);
        h.play();
        h.pause();
        assert!(!h.playing);
    }

    #[test]
    fn audio_handle_volume_clamps() {
        let h = AudioHandle::new(4, AudioFormat::Ogg, 10.0).volume(1.5);
        assert_eq!(h.volume, 1.0);
        let h2 = AudioHandle::new(5, AudioFormat::Ogg, 10.0).volume(-0.5);
        assert_eq!(h2.volume, 0.0);
    }

    #[test]
    fn audio_handle_looping() {
        let h = AudioHandle::new(6, AudioFormat::Flac, 60.0).looping(true);
        assert!(h.looping);
    }

    #[test]
    fn audio_player_new() {
        let p = AudioPlayer::new();
        assert_eq!(p.load_count(), 0);
    }

    #[test]
    fn audio_player_load_returns_unavailable() {
        let mut p = AudioPlayer::new();
        let result = p.load("music.mp3");
        assert_eq!(result.unwrap_err(), MediaError::PlatformUnavailable);
    }

    #[test]
    fn audio_player_load_unknown_ext_returns_unsupported() {
        let mut p = AudioPlayer::new();
        let result = p.load("music.xyz");
        assert_eq!(result.unwrap_err(), MediaError::Unsupported);
    }
}
