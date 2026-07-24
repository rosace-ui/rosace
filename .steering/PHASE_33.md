# Phase 33 — Custom Shader Materials: `ShaderPaint`, the Material Cascade, Backdrop Sampling (D124)

> Status: PLANNED (scoped 2026-07-18, not started).
> Decision: **D124** — read DECISIONS.md first; it holds the constraints.
> Prereqs: D109 (shader registry + `ShaderFill`, SHIPPED), D105 (theme
> extensions, SHIPPED), D-DEF-012 (BackdropBlur scene capture, SHIPPED).
> Everything here is ADDITIVE — zero structural change to the render
> object, the widget protocol, or the compositor's existing contracts.

## Why This Phase

D109 built a general shader-pipeline registry and deliberately noted that a
`ShaderPaint` widget "falls out of the core-rendering mechanism for free" —
then the widget itself was never built (the recurring built-but-never-wired
pattern: forms, RichText, hooks, now this). Meanwhile the user's actual goal
is bigger than one widget: **apps building 2026-era UI (Liquid-Glass-style
materials, per-surface custom looks) on top of ROSACE**, with materials
controllable per-widget AND app-wide from one registration point.

Research note (how Apple does Liquid Glass, which shaped this scope):
- It is **backdrop-based** — glass samples/refracts the scene behind it.
  A shader that only sees its own uniforms structurally cannot do glass.
- Developers get a **declarative style** (`.glassEffect()`), not raw
  shader authorship; the system renders it.
- `GlassEffectContainer` merges adjacent glass shapes into one continuous
  material — cross-widget coordination, explicitly deferred here.

## The Material Model (the core design)

One value type used at EVERY level of the system:

```rust
// rosace-shader (Layer 5)
#[derive(Clone, PartialEq)]
pub struct ShaderMaterial {
    pub pipeline: PipelineId,
    pub uniforms: Vec<u8>,          // via #[derive(ShaderUniforms)] to_bytes()
    pub fallback: Option<Color>,    // honest CPU/web degradation (see below)
}
```

**The cascade (user-decided): instance → theme-global → none.**

```rust
// rosace-widgets — ONE resolver, used by every material-capable widget.
// Mirrors resolve_physics (scroll_view.rs:81) EXACTLY — the codebase's
// existing precedent for explicit → theme.ext → default resolution:
pub fn resolve_material<K: MaterialKey>(
    theme: &ThemeData,
    explicit: Option<&ShaderMaterial>,
) -> Option<ShaderMaterial> {
    explicit.cloned().or_else(|| theme.ext::<K>().map(|m| m.0.clone()))
}
```

**"Global" is scoped per widget KIND via type-keyed theme extensions
(D105's type-map), NOT one shader for every surface.** Each material-capable
widget defines its own key newtype (`CardMaterial(ShaderMaterial)`,
`ContainerMaterial(...)`) — a Card can be frosted glass while Buttons stay
solid. Register once on the theme → every Card app-wide picks it up; theme
switching (already live, theme is a GlobalAtom) swaps materials for free.
Type-keying (not an enum) is what lets THIRD-PARTY widgets define their own
material slots without editing rosace — same extensibility bar as D115's
icon registry.

**Fallback discipline (honest degradation, BackdropBlur's own precedent):**
`ShaderFill` renders through the GPU compositor; the true no-GPU paths
(softbuffer fallback, web's putImageData) drop shader quads entirely. A
material therefore carries an optional `fallback` color painted by the
normal fill path when no GPU compositor exists. No fallback set → the
widget's ordinary default rendering (as if no material). Never a silent
black hole.

## Steps

### Step 1 — `ShaderMaterial` + resolver + starter material library
- `ShaderMaterial` in `rosace-shader`; `MaterialKey` marker trait +
  `resolve_material` in `rosace-widgets` (one function, unit-tested).
- Seed `rosace_shader::materials` with 2-3 REAL ready-made pipelines
  (animated gradient, noise/grain, glow) — the "curated set" layer Apple
  ships; apps get good defaults without writing WGSL, and the library
  doubles as reference WGSL for authors who do.
- Time-driven animation convention: a documented standard `time: f32`
  uniform slot + a `TimeUniforms` helper; widgets using an animated
  material call `ctx.request_animation()` (EVENT-driven frame requests —
  the D123 rule; no free-running loops).

Exit: resolver unit tests (explicit beats theme beats none); a registered
starter material renders in a headless paint test.

### Step 2 — The `ShaderPaint` widget
- Leaf widget, D094 vocabulary: `ShaderPaint::new(material)`,
  `.size(w,h)/.width/.height`, `.radius(f32)` (rounded clip via the
  pipeline's uniform, matching built-in rrect convention), `.animated()`
  (opt-in per-frame time uniform + request_animation).
- Decorative by default: no semantics entry, no hit region (it is paint,
  not a control) — documented, deliberate.
- Registration follows WIDGET_RECIPE_PHASE32.md (mod.rs/lib.rs/prelude).

Exit: gallery section renders a custom app-authored WGSL material live +
one starter-library material, screenshotted.

### Step 3 — Wire the cascade into the first surface widgets
- `Container.material(ShaderMaterial)` + `Card.material(...)` — instance
  slot + `ContainerMaterial`/`CardMaterial` theme keys + resolver call in
  paint(): material (if any) replaces the background fill; everything else
  (border, radius, child, padding) unchanged.
- Prove the "register once" story: gallery/theming demo registers a
  CardMaterial on the theme → every card in the app changes, no per-card
  code; one card overrides per-instance → cascade visibly correct.

Exit: live demo of both cascade levels + tests (instance-over-theme,
theme-when-no-instance, none-renders-as-today).

### Step 4 — Backdrop sampling (the glass enabler)
- `ShaderSpec::with_backdrop()` — a pipeline DECLARES it samples the
  scene. Compositor generalizes D-DEF-012's existing scene-texture
  indirection: for flagged pipelines, bind the captured behind-this-rect
  scene texture + its UV window alongside the user uniforms (one new bind
  group layout for backdrop pipelines; `BackdropBlur`'s own dedicated
  fast path stays untouched — it is performance-tuned and shipping).
- Documented uniform/binding contract for backdrop shaders (scene texture
  @binding(N), uv rect uniform) so third parties can target it.
- Build `glass_material_demo`: a Liquid-Glass-STYLE material (refraction
  offset + specular edge + tint) through the GENERIC path — the proof the
  registry can express glass, not just self-contained effects.

Exit: glass demo live over scrolling content (backdrop visibly refracts as
content moves behind it); BackdropBlur regression-checked (glass_demo
still renders identically).

### Step 5 — Surface-widget rollout + third-party extensibility proof
- Extend `.material()` to the remaining SURFACE widgets: Dialog, Sheet,
  Drawer, AppBar, BottomNavigationBar (surfaces where a material makes
  sense; NOT every widget — a Checkbox has no material surface).
- Extensibility exit bar (mirrors D115 Step 2's icon bar): a separate test
  crate registers its OWN pipeline (PipelineId::user range), its OWN
  MaterialKey, on its OWN widget — without editing rosace-* crates.

Exit: themed-material sweep across surface widgets live in the gallery;
third-party crate proof compiles + renders.

## Future-Proofing (named now, wired later)

- **Interaction-reactive materials** (Apple's glass responds to
  touch/pointer): the hooks already exist (`ctx.hovered()`,
  `ctx.pressed()`, press position) — a later step can feed them as
  standard uniforms. Named, not in scope.
- **GlassEffectContainer-style shape merging** (adjacent glass surfaces
  blend into one continuous material): real cross-widget coordination,
  deferred to its own future phase after this ships and gets used.
- **Shader hot-reload** (edit WGSL, see it live): rides D102 hot-reload
  when that lands — `register_shader` already replaces on re-register, so
  the seam is ready.
- **wasm/WebGPU**: shader materials on web are gated on the deferred
  WebGPU presenter (PHASE_27 Out of Scope) — fallback colors carry web
  until then.
- **SDF text effects / material-on-text**: out of scope; text stays on
  the glyph atlas path.

## Performance Guardrails (from D109's own lessons)

- Pipelines compile EAGERLY at registration (the Impeller lesson) — never
  lazily on first paint.
- Per-frame cost of an animated material = one uniform write + the quad's
  per-pixel fragment cost. Budget guidance documented on `ShaderPaint`:
  a full-screen shader pays per-pixel across the whole surface.
- Animated materials MUST drive frames via `request_animation` (skip-
  present/D089 stays effective for static materials — a static shader
  quad with unchanged uniforms skips like any other unchanged frame).
- Backdrop pipelines force the scene-capture pass for their rect (same
  cost class as BackdropBlur today) — documented so authors reach for
  self-contained materials when they don't need the backdrop.

## Migration Rule

All additive. No existing widget changes appearance unless a material is
explicitly set (instance) or registered (theme). `BackdropBlur` and every
built-in shape pipeline are untouched. Apps that never use materials see
zero behavior and zero performance change.


## Post-phase: GPU animation fast path (D109 maturity, 2026-07-19)

User verdict on the 110%-CPU animated-aurora loop: "the aurora should be on
GPU, that is why we developed the gpu pattern — this is not matured." Fixed:

- `DrawCommand::ShaderFill` gained `animate_time`; `ShaderPaint::animated()`
  records its quad ONCE (no per-frame repaint, no `request_animation`
  loop); the PLATFORM patches the live clock into the retained quad's
  first 4 uniform bytes each present (`time`-first convention) and keeps
  the loop alive through the `FrameRequest` proxy (an in-handler
  `request_redraw` is coalesced on macOS — found live as a frozen loop),
  throttled to ~30fps for ambient animation.
- Overlay canvas clear gated on `has_drawn()` — an unconditional
  full-window tiny-skia fill per present was ~40% of a debug core.
- Measured (debug, liquid_glass_app): 110% → 33% CPU with the aurora
  still animating and the ENGINE fully idle (3 paints / 6 s). Remaining
  ~33% = per-present scene re-render for the backdrop glass (its input
  really changes every frame) + Metal drawable acquire; collapses to a
  few % in release.

Named follow-ups (not built):
- Animated CPU widgets (CircularProgress spinner, caret blink) still
  force engine repaints every frame — animated-widget damage granularity
  is the next maturity gap. The liquid demo swapped its spinner for a
  GPU glow pulse to stay on the fast path.
- Animated quads INSIDE GPU scroll layers re-render that layer's whole
  offscreen content per frame (works, heavy); per-quad patching there is
  the refinement.
- Overlay-pass shader quads (glass popups/menus) still unsupported —
  Menu ships a translucent rounded tint instead.
