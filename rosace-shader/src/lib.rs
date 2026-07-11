//! Shader pipeline types + registration queue for ROSACE's GPU-native
//! rendering (D109 / Phase 27).
//!
//! This crate is the *typed* face of the pipeline registry: apps and widget
//! code describe a pipeline as a [`ShaderSpec`] (WGSL source + blend mode)
//! keyed by a [`PipelineId`], and produce uniform bytes through the
//! [`ShaderUniforms`] trait — normally via `#[derive(ShaderUniforms)]`
//! (`rosace-macros`), which generates a WGSL-uniform-layout-correct
//! `to_bytes()` at compile time so nobody hand-packs a byte buffer.
//!
//! Deliberately **zero wgpu dependency**: `wgpu::RenderPipeline` compilation
//! and storage live in `rosace-compositor` (Layer 0, which cannot import
//! this crate's types — its registration API takes primitives; the platform
//! layer converts). Registration here only queues; the platform drains the
//! queue into the compositor eagerly at startup / next frame boundary,
//! never lazily on first paint (the Impeller lesson, see PHASE_27.md).

pub mod builtin;

use std::sync::Mutex;

/// Stable identity of a registered shader pipeline.
///
/// Ids below [`PipelineId::BUILTIN_MAX`] are reserved for ROSACE's built-in
/// shape pipelines (Phase 27 Step 3); app/custom pipelines must use
/// [`PipelineId::user`]. Re-registering an id replaces its pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PipelineId(u64);

impl PipelineId {
    /// Upper bound (exclusive) of the id range reserved for built-in
    /// pipelines. Registered by the framework itself, never by app code.
    pub const BUILTIN_MAX: u64 = 0x100;

    /// A user/custom pipeline id. Panics on ids inside the reserved
    /// built-in range — a compile-visible constant misuse, not a runtime
    /// data error, so a loud panic beats a silent collision.
    pub fn user(id: u64) -> Self {
        assert!(
            id >= Self::BUILTIN_MAX,
            "PipelineId::user({id}) collides with the reserved built-in range 0..{}",
            Self::BUILTIN_MAX
        );
        Self(id)
    }

    /// A built-in pipeline id (framework-internal — Phase 27 Step 3
    /// registers the built-in shape pipelines through this). Hidden from
    /// docs so app code reaches for [`PipelineId::user`] instead; the
    /// reserved-range assert there is the real collision guard.
    #[doc(hidden)]
    pub const fn builtin(id: u64) -> Self {
        Self(id)
    }

    /// The raw id — what `DrawCommand::ShaderFill` carries (`rosace-render`
    /// is Layer 4 and cannot depend on this Layer 5 crate) and what the
    /// compositor's primitives-only API accepts.
    pub const fn raw(self) -> u64 {
        self.0
    }
}

/// How a pipeline's output blends over what's already in the frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BlendMode {
    /// Standard premultiplied source-over — the default, matches how every
    /// built-in CPU path composites today.
    #[default]
    Alpha,
    /// Output replaces the destination (no blending). For fully opaque
    /// effects; cheaper than Alpha on tile-based GPUs.
    Opaque,
    /// Source added to destination — glows, particles.
    Additive,
}

/// A complete pipeline description: everything the compositor needs to
/// compile a `wgpu::RenderPipeline`, and nothing it doesn't.
///
/// The WGSL must define `@vertex fn vs_main` and `@fragment fn fs_main`
/// (naming contract enforced at compile/registration time by the
/// compositor, which fails loudly per D109's eager-compilation rule).
#[derive(Debug, Clone, PartialEq)]
pub struct ShaderSpec {
    pub wgsl_source: String,
    pub blend: BlendMode,
}

impl ShaderSpec {
    pub fn new(wgsl_source: impl Into<String>) -> Self {
        Self { wgsl_source: wgsl_source.into(), blend: BlendMode::Alpha }
    }

    pub fn blend(mut self, blend: BlendMode) -> Self {
        self.blend = blend;
        self
    }
}

/// Re-exported from `rosace-core` (the trait lives at Layer 2 so
/// `rosace-render`'s built-in shape conversions can produce uniform bytes
/// without an upward dependency — see `rosace_render::gpu_shapes`).
pub use rosace_core::shader::ShaderUniforms;

/// Pending registrations, queued by [`register_shader`] and drained by the
/// platform layer via [`take_pending_shaders`] — compiled into real
/// `wgpu::RenderPipeline`s by the compositor at the next frame boundary
/// (eager per D109: before any paint references the pipeline, never
/// on-demand at first use).
static PENDING: Mutex<Vec<(PipelineId, ShaderSpec)>> = Mutex::new(Vec::new());

/// Queue a shader pipeline for eager compilation. Call at app startup (or
/// any time before the first frame that draws with it). Re-registering an
/// id replaces the previous pipeline.
pub fn register_shader(id: PipelineId, spec: ShaderSpec) {
    rosace_trace::trace!(rosace_trace::event::RosaceTrace::ShaderRegister {
        pipeline: id.raw(),
        wgsl_len: spec.wgsl_source.len(),
    });
    PENDING.lock().unwrap_or_else(|e| e.into_inner()).push((id, spec));
}

/// Drain everything queued since the last call. Platform-layer only.
pub fn take_pending_shaders() -> Vec<(PipelineId, ShaderSpec)> {
    std::mem::take(&mut *PENDING.lock().unwrap_or_else(|e| e.into_inner()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_macros::ShaderUniforms;

    // ── Layout tests: each asserts the EXACT byte layout the derive
    // produces, against WGSL uniform-address-space rules computed by hand.

    #[derive(ShaderUniforms)]
    struct ScalarThenVec4 {
        a: f32,
        b: [f32; 4],
    }

    #[test]
    fn scalar_then_vec4_pads_to_vec4_alignment() {
        let v = ScalarThenVec4 { a: 1.5, b: [1.0, 2.0, 3.0, 4.0] };
        let bytes = v.to_bytes();
        // a at 0..4, pad 4..16 (vec4 aligns to 16), b at 16..32.
        assert_eq!(bytes.len(), 32);
        assert_eq!(&bytes[0..4], &1.5f32.to_le_bytes());
        assert_eq!(&bytes[4..16], &[0u8; 12]);
        assert_eq!(&bytes[16..20], &1.0f32.to_le_bytes());
        assert_eq!(&bytes[28..32], &4.0f32.to_le_bytes());
    }

    #[derive(ShaderUniforms)]
    struct ScalarsOnly {
        a: f32,
        b: u32,
        c: i32,
    }

    #[test]
    fn scalars_pack_tightly_then_round_to_sixteen() {
        let v = ScalarsOnly { a: 0.5, b: 7, c: -3 };
        let bytes = v.to_bytes();
        // 3 × 4 bytes = 12, rounded up to 16.
        assert_eq!(bytes.len(), 16);
        assert_eq!(&bytes[0..4], &0.5f32.to_le_bytes());
        assert_eq!(&bytes[4..8], &7u32.to_le_bytes());
        assert_eq!(&bytes[8..12], &(-3i32).to_le_bytes());
        assert_eq!(&bytes[12..16], &[0u8; 4]);
    }

    #[derive(ShaderUniforms)]
    struct Vec3ThenScalar {
        v: [f32; 3],
        s: f32,
    }

    #[test]
    fn scalar_fits_in_vec3_tail_padding() {
        let u = Vec3ThenScalar { v: [1.0, 2.0, 3.0], s: 9.0 };
        let bytes = u.to_bytes();
        // vec3 at 0..12 (align 16, size 12); f32 (align 4) fits at 12..16 —
        // the classic std140/WGSL vec3-tail case. Total exactly 16.
        assert_eq!(bytes.len(), 16);
        assert_eq!(&bytes[8..12], &3.0f32.to_le_bytes());
        assert_eq!(&bytes[12..16], &9.0f32.to_le_bytes());
    }

    #[derive(ShaderUniforms)]
    struct Vec2Pair {
        a: [f32; 2],
        b: f32,
        c: [f32; 2],
    }

    #[test]
    fn vec2_alignment_inserts_padding_after_odd_scalar() {
        let u = Vec2Pair { a: [1.0, 2.0], b: 3.0, c: [4.0, 5.0] };
        let bytes = u.to_bytes();
        // a at 0..8; b at 8..12; c needs align 8 → pad 12..16, c at 16..24;
        // total rounds to 32.
        assert_eq!(bytes.len(), 32);
        assert_eq!(&bytes[8..12], &3.0f32.to_le_bytes());
        assert_eq!(&bytes[12..16], &[0u8; 4]);
        assert_eq!(&bytes[16..20], &4.0f32.to_le_bytes());
        assert_eq!(&bytes[24..32], &[0u8; 8]);
    }

    #[derive(ShaderUniforms)]
    struct Mat4Uniform {
        m: [[f32; 4]; 4],
    }

    #[test]
    fn mat4_is_sixty_four_column_major_order_preserved() {
        let mut m = [[0.0f32; 4]; 4];
        m[0][0] = 1.0;
        m[3][3] = 2.0;
        let bytes = Mat4Uniform { m }.to_bytes();
        assert_eq!(bytes.len(), 64);
        assert_eq!(&bytes[0..4], &1.0f32.to_le_bytes());
        assert_eq!(&bytes[60..64], &2.0f32.to_le_bytes());
    }

    // ── Registry queue behavior ─────────────────────────────────────────

    #[test]
    fn user_pipeline_id_rejects_reserved_builtin_range() {
        let result = std::panic::catch_unwind(|| PipelineId::user(3));
        assert!(result.is_err(), "ids below BUILTIN_MAX must panic in user()");
        assert_eq!(PipelineId::user(0x100).raw(), 0x100);
    }

    #[test]
    fn register_then_take_drains_in_order_and_empties() {
        // Serialize against any other test touching the global queue.
        let _ = take_pending_shaders();
        register_shader(PipelineId::user(0x200), ShaderSpec::new("// a"));
        register_shader(
            PipelineId::user(0x201),
            ShaderSpec::new("// b").blend(BlendMode::Additive),
        );
        let drained = take_pending_shaders();
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].0.raw(), 0x200);
        assert_eq!(drained[1].0.raw(), 0x201);
        assert_eq!(drained[1].1.blend, BlendMode::Additive);
        assert!(take_pending_shaders().is_empty(), "second take must be empty");
    }
}
