/// Which rendering backend is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RendererBackend {
    /// tiny-skia (pure Rust, current MVP, WASM-compatible).
    TinySkia,
    /// skia-safe (C++ Skia bindings, planned for v1.0 GPU support).
    SkiaSafe,
}

impl RendererBackend {
    pub fn is_tiny_skia(&self) -> bool {
        *self == RendererBackend::TinySkia
    }

    pub fn is_skia_safe(&self) -> bool {
        *self == RendererBackend::SkiaSafe
    }

    pub fn description(&self) -> &'static str {
        match self {
            RendererBackend::TinySkia => "tiny-skia — pure Rust, CPU, WASM-compatible",
            RendererBackend::SkiaSafe => "skia-safe — C++ Skia bindings, GPU acceleration",
        }
    }
}

impl Default for RendererBackend {
    fn default() -> Self {
        RendererBackend::TinySkia
    }
}

impl std::fmt::Display for RendererBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.description())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_default_is_tiny_skia() {
        assert_eq!(RendererBackend::default(), RendererBackend::TinySkia);
    }

    #[test]
    fn backend_is_tiny_skia() {
        let b = RendererBackend::TinySkia;
        assert!(b.is_tiny_skia());
        assert!(!b.is_skia_safe());
    }

    #[test]
    fn backend_is_skia_safe() {
        let b = RendererBackend::SkiaSafe;
        assert!(b.is_skia_safe());
        assert!(!b.is_tiny_skia());
    }

    #[test]
    fn backend_description_tiny_skia() {
        let b = RendererBackend::TinySkia;
        assert_eq!(b.description(), "tiny-skia — pure Rust, CPU, WASM-compatible");
    }

    #[test]
    fn backend_description_skia_safe() {
        let b = RendererBackend::SkiaSafe;
        assert_eq!(b.description(), "skia-safe — C++ Skia bindings, GPU acceleration");
    }

    #[test]
    fn backend_display() {
        let b = RendererBackend::TinySkia;
        let s = format!("{}", b);
        assert_eq!(s, "tiny-skia — pure Rust, CPU, WASM-compatible");
    }

    #[test]
    fn backend_clone_copy() {
        let b = RendererBackend::TinySkia;
        let c = b; // Copy
        let d = b.clone(); // Clone
        assert_eq!(b, c);
        assert_eq!(b, d);
    }

    #[test]
    fn backend_eq() {
        assert_eq!(RendererBackend::TinySkia, RendererBackend::TinySkia);
        assert_eq!(RendererBackend::SkiaSafe, RendererBackend::SkiaSafe);
        assert_ne!(RendererBackend::TinySkia, RendererBackend::SkiaSafe);
    }
}
