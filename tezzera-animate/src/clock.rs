use std::sync::atomic::{AtomicU32, Ordering};

// Default: 1/60 s as IEEE 754 bits
static FRAME_DT: AtomicU32 = AtomicU32::new(0x3C888889);

pub fn set_frame_dt(dt: f32) {
    let clamped = dt.clamp(0.001, 0.1);
    FRAME_DT.store(clamped.to_bits(), Ordering::Release);
}

pub fn frame_dt() -> f32 {
    f32::from_bits(FRAME_DT.load(Ordering::Acquire))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        set_frame_dt(0.032);
        let v = frame_dt();
        assert!((v - 0.032).abs() < 1e-6, "expected 0.032, got {v}");
    }

    #[test]
    fn clamps_low() {
        set_frame_dt(0.0);
        assert!(frame_dt() >= 0.001);
    }

    #[test]
    fn clamps_high() {
        set_frame_dt(10.0);
        assert!(frame_dt() <= 0.1);
    }

    #[test]
    fn default_is_sixty_hz() {
        // Reset to default by writing 1/60 manually
        set_frame_dt(1.0 / 60.0);
        let v = frame_dt();
        assert!((v - 1.0 / 60.0).abs() < 1e-5);
    }
}
