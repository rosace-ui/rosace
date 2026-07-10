use std::time::{Duration, Instant};

/// Tracks frame render times and computes FPS statistics.
pub struct FrameProfiler {
    frame_times: Vec<Duration>,
    last_frame: Option<Instant>,
    max_samples: usize,
}

impl FrameProfiler {
    pub fn new() -> Self {
        Self { frame_times: Vec::new(), last_frame: None, max_samples: 120 }
    }

    pub fn max_samples(mut self, n: usize) -> Self {
        self.max_samples = n;
        self
    }

    /// Call at the start of each frame.
    pub fn begin_frame(&mut self) {
        let now = Instant::now();
        if let Some(last) = self.last_frame {
            let dt = now.duration_since(last);
            if self.frame_times.len() >= self.max_samples {
                self.frame_times.remove(0);
            }
            self.frame_times.push(dt);
        }
        self.last_frame = Some(now);
    }

    /// Average FPS over sampled frames.
    pub fn avg_fps(&self) -> f32 {
        if self.frame_times.is_empty() {
            return 0.0;
        }
        let avg = self.frame_times.iter().map(|d| d.as_secs_f32()).sum::<f32>()
            / self.frame_times.len() as f32;
        if avg == 0.0 { 0.0 } else { 1.0 / avg }
    }

    /// Minimum FPS (worst frame). Returns 0.0 when no samples are recorded.
    pub fn min_fps(&self) -> f32 {
        if self.frame_times.is_empty() {
            return 0.0;
        }
        self.frame_times
            .iter()
            .map(|d| if d.as_secs_f32() == 0.0 { 0.0 } else { 1.0 / d.as_secs_f32() })
            .fold(f32::INFINITY, f32::min)
            .max(0.0)
    }

    /// Maximum FPS (best frame).
    pub fn max_fps(&self) -> f32 {
        self.frame_times
            .iter()
            .map(|d| if d.as_secs_f32() == 0.0 { 0.0 } else { 1.0 / d.as_secs_f32() })
            .fold(0.0_f32, f32::max)
    }

    /// Number of frames sampled.
    pub fn sample_count(&self) -> usize {
        self.frame_times.len()
    }

    /// Render an ASCII FPS bar with sparkline.
    pub fn render(&self) -> String {
        let avg = self.avg_fps();
        let min = self.min_fps();
        let max = self.max_fps();
        let n = self.sample_count();

        // ASCII sparkline using ▁▂▃▄▅▆▇█
        let bars = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
        let sparkline: String = self.frame_times.iter().rev().take(20).rev()
            .map(|d| {
                let fps =
                    if d.as_secs_f32() == 0.0 { 0.0 } else { 1.0 / d.as_secs_f32() };
                let normalized = (fps / 60.0).min(1.0);
                bars[(normalized * 7.0) as usize]
            })
            .collect();

        format!(
            "┌─ FPS ({} samples) ───────────────────────────────\n│  avg {:5.1}  min {:5.1}  max {:5.1}\n│  {}\n└─────────────────────────────────────────────────\n",
            n, avg, min, max, sparkline
        )
    }

    pub fn clear(&mut self) {
        self.frame_times.clear();
        self.last_frame = None;
    }
}

impl Default for FrameProfiler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::thread;

    use super::*;

    #[test]
    fn frame_profiler_new_zero_fps() {
        let profiler = FrameProfiler::new();
        assert_eq!(profiler.avg_fps(), 0.0);
        assert_eq!(profiler.sample_count(), 0);
    }

    #[test]
    fn frame_profiler_begin_frame_increments_samples() {
        let mut profiler = FrameProfiler::new();
        // First begin_frame sets the baseline, second produces the first sample.
        profiler.begin_frame();
        assert_eq!(profiler.sample_count(), 0);
        thread::sleep(Duration::from_millis(5));
        profiler.begin_frame();
        assert_eq!(profiler.sample_count(), 1);
    }

    #[test]
    fn frame_profiler_avg_fps_reasonable() {
        let mut profiler = FrameProfiler::new();
        // Inject two synthetic frame times of ~16ms each (≈60fps).
        profiler.frame_times.push(Duration::from_millis(16));
        profiler.frame_times.push(Duration::from_millis(16));
        let fps = profiler.avg_fps();
        // Should be around 62.5 fps; accept a wide range.
        assert!(fps > 50.0 && fps < 80.0, "unexpected avg fps: {}", fps);
    }

    #[test]
    fn frame_profiler_min_max_fps() {
        let mut profiler = FrameProfiler::new();
        profiler.frame_times.push(Duration::from_millis(8));  // 125 fps (max)
        profiler.frame_times.push(Duration::from_millis(33)); // ~30 fps (min)
        let min = profiler.min_fps();
        let max = profiler.max_fps();
        assert!(min < max, "min should be less than max");
        assert!(min > 25.0 && min < 35.0, "unexpected min fps: {}", min);
        assert!(max > 100.0 && max < 150.0, "unexpected max fps: {}", max);
    }

    #[test]
    fn frame_profiler_sparkline_in_render() {
        let mut profiler = FrameProfiler::new();
        profiler.frame_times.push(Duration::from_millis(16));
        profiler.frame_times.push(Duration::from_millis(16));
        let rendered = profiler.render();
        // Sparkline chars should be present.
        let has_bar = rendered.chars().any(|c| "▁▂▃▄▅▆▇█".contains(c));
        assert!(has_bar, "sparkline missing from render output");
    }

    #[test]
    fn frame_profiler_clear_resets() {
        let mut profiler = FrameProfiler::new();
        profiler.frame_times.push(Duration::from_millis(16));
        profiler.last_frame = Some(Instant::now());
        profiler.clear();
        assert_eq!(profiler.sample_count(), 0);
        assert_eq!(profiler.avg_fps(), 0.0);
    }

    #[test]
    fn frame_profiler_max_samples_evicts_old() {
        let mut profiler = FrameProfiler::new().max_samples(3);
        // Manually push 5 frame times.
        for _ in 0..5 {
            if profiler.frame_times.len() >= profiler.max_samples {
                profiler.frame_times.remove(0);
            }
            profiler.frame_times.push(Duration::from_millis(16));
        }
        assert_eq!(profiler.sample_count(), 3);
    }

    #[test]
    fn frame_profiler_min_fps_empty_returns_zero() {
        let profiler = FrameProfiler::new();
        // min_fps with no samples returns 0.0 (fold on empty => INFINITY, .max(0.0))
        assert_eq!(profiler.min_fps(), 0.0);
    }
}
