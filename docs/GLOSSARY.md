# ROSACE — GLOSSARY
> Every term defined precisely. When in doubt, check here first.

This glossary has two parts:

1. **ROSACE terms (A–Z below)** — names invented by this framework: `Atom`,
   `Flexure`, `RefreshEngine`, and so on.
2. **[Graphics, GPU & Rendering — a plain-language primer](#graphics-gpu--rendering--a-plain-language-primer)**
   at the end — the *general* computer-graphics vocabulary the architecture
   book leans on (UV mapping, LRU cache, SDF, blend modes, gamma…). These are
   not ROSACE inventions; they're industry terms, explained here from scratch
   so no chapter has to assume you already know them. Each has a linkable
   heading, so prose elsewhere can write "the layer samples at a
   [UV offset](GLOSSARY.md#uv-mapping)" instead of dropping the jargon raw.

> **Accuracy note (2026-07-24):** some older entries described an *intended*
> stack that was never built (e.g. Skia/cosmic-text/HarfBuzz). Those have been
> corrected to the real dependencies (`tiny-skia`, `fontdue`, `wgpu`). If an
> entry and the code ever disagree, the code wins — file the entry as a bug.

---

> ### ⚠️ Audit banner — entries not verified against the current build (2026-07-24)
>
> A code-grounded sweep found these A–Z entries describe APIs with **no
> matching symbol anywhere in the source** — they're Phase-1-era aspirational
> names, not shipped types. Kept (not deleted) in case they're roadmap items,
> but **do not treat them as real until verified**:
>
> - **`use_async`**, **`use_before_leave`** — no such hooks (the
>   [state chapter](architecture/state-and-reactivity.md) explicitly says
>   `use_async` is unimplemented; `AsyncState` is just the data enum).
> - **`RosaceRenderer`**, **`WidgetOverride`**, **`WidgetScope`** + the whole
>   "**Level 1–5 customization**" framing — that phrase appears *only* in this
>   glossary, never in code or `DECISIONS.md`.
> - **`ErrorBoundary`**, **`FocusScope`**, **`AtomProvider`/`Provider`**,
>   **`ForeignBox`**, **`IntrinsicHeight`/`IntrinsicWidth`** (the *concept* —
>   D016 opt-in intrinsic sizing — exists, but not under these widget names),
>   **`Derived Atom`** — no matching symbol found.
> - **`RosaceApp`** — the real root builder is **`App`** (`App::new().title().size(w,h).launch(Component)`).
>
> Verified-real entries (`Atom`, `use_atom`, `AsyncState`, `GlobalAtom`,
> `NavigationDecision`, `KeepAlive`, `SemanticNode`, `RingBufferSubscriber`,
> `TracingBus`, `RosaceTheme`, `RosaceError`, `Flexure`, …) are unaffected.

## A

**Atom**
ROSACE's core state primitive. A reactive value of type T.
When changed via .set() or .update(), all subscriber components
are scheduled for rebuild. The smallest unit of state.

**AtomId**
A unique identifier for an atom instance. Used by the
refresh engine and tracing system.

**AtomProvider**
A widget that makes a scoped atom available to its subtree.
Multiple providers can exist for the same atom — each is isolated.

**AsyncState<T>**
The five states of an async atom:
Idle, Loading, Success(T), Error(RosaceError), Refreshing(T).

**AxisBound**
Describes the constraint on one axis: Bounded(f32) for exact max,
Unbounded for scroll axes, Shrink to take only needed space.

---

## B

**Batch**
A group of atom changes that triggers only one rebuild.
Automatic within sync blocks. Manual via batch() function.

**BiDi**
Bidirectional text — mixed left-to-right and right-to-left
text in the same paragraph (e.g. an English word inside an Arabic
sentence). ROSACE's handling lives in `rosace-bidi`; full
complex-script shaping is deferred to v1.0 (see **HarfBuzz** below).

---

## C

**ChildContainer**
A trait implemented by multi-child widgets (Column, Row, Stack etc.)
providing .child(), .children(), .builder(), .child_if(), .prepend(),
.append(), .append_many() methods.

**ComponentId**
A unique identifier for a component instance in the tree.
Used for identity tracking, key resolution, and tracing.

**Constraints**
The layout instruction passed from parent to child:
min_width, max_width (AxisBound), min_height, max_height (AxisBound).

**Context**
The build context passed to RosaceComponent::build().
Provides access to local state, lifecycle hooks, and services.

**cosmic-text** — *NOT used (historical).*
An early design named `cosmic-text` as the text-layout library, but it
was never adopted. The real stack is `fontdue` (glyph rasterization) +
`ttf-parser` (font-file parsing) with ROSACE's own glyph-placement walk
([`layout_glyphs`](../rosace-render/src/font.rs)) and a first-fit
`FallbackShaper` (one glyph per character — full shaping deferred to
v1.0, D014). Kept here only so old references resolve to the truth.

---

## D

**Derived Atom**
An atom whose value is computed from other atoms.
Lazily recomputed — only when read. Auto-invalidated when
source atoms change.

**Dirty**
A component or screen region that needs to be rebuilt or repainted.
Marked dirty by the refresh engine when a subscribed atom changes.

**DFS Timestamp**
Depth-first search entry/exit timestamps used by the refresh engine's
tree index for O(1) ancestor lookup.

---

## E

**Element**
A lightweight, immutable description of what a component wants
to render. Created by RosaceComponent::build(). Cheap to create.
The virtual representation before layout and paint.

**ErrorBoundary**
A widget that catches panics and RosaceErrors from its subtree
and shows a fallback UI instead of crashing.

---

## F

**Flexure**
The name of ROSACE's constraint-based layout engine.
Implements three-pass layout: Measure, Place, Paint.

**ForeignBox**
A RAII wrapper for memory allocated by external C code.
Automatically calls the provided free function on drop.

**FocusScope**
A widget that manages keyboard focus. Can auto-focus first child
and trap focus within its subtree (for dialogs).

---

## G

**GlobalAtom**
An atom with app-wide scope. Accessible from any component
without a provider. Use sparingly — only for truly global state.

**Glyph Cache**
A GPU texture atlas of rasterized glyphs. Prevents rasterizing
the same glyph twice. LRU eviction when full.

---

## H

**HarfBuzz** — *NOT used (deferred).*
The industry-standard text *shaping* library (used by Chrome/Firefox):
it turns a run of Unicode characters into correctly positioned glyphs,
handling ligatures, kerning, and complex scripts (Arabic joining,
Indic reordering). ROSACE does **not** depend on it. Today it uses a
first-fit `FallbackShaper` (one glyph per character, no ligatures/
complex scripts); real shaping is deferred to v1.0 (D014). Shaping =
"which glyphs, in what order, where"; see also **kerning** in the
graphics primer.

---

## I

**IntrinsicHeight / IntrinsicWidth / IntrinsicSize**
Widgets that force a two-pass layout to measure children before
sizing themselves. Explicit opt-in — zero cost when not used.

---

## J

**JIT (dev mode)**
ROSACE approximates JIT in dev mode via WASM hot-swap.
Component code changes → WASM module swapped → UI updates instantly.
Not true JIT — fast incremental recompile + hot-swap.

---

## K

**Key**
An optional identifier attached to a component with .key(value).
Tells ROSACE to track this component by key rather than position.
Required for dynamic lists where order can change.

**KeepAlive**
A widget that preserves its child's state even when removed from
the active tree (e.g. tab switching). Memory budget enforced via LRU.

---

## L

**Layout Cache**
A cache of text layout results keyed by string + style + width.
Invalidated when any input changes. Prevents re-measuring unchanged text.

**LifecycleState**
The four states of app lifecycle: Active, Inactive, Background, Suspended.
Available as a GlobalAtom.

**Logical Sides**
Padding/margin values that respect text direction:
.padding_start() = left in LTR, right in RTL.
.padding_end() = right in LTR, left in RTL.

---

## N

**NavigationDecision**
The result of a navigation guard: Allow, Block, or RedirectTo(route).

---

## O

**on_mount**
Lifecycle hook. Fires once when component is added to the tree.
Return a cleanup function to run on unmount.

**on_unmount**
Lifecycle hook. Fires once when component is removed from tree.

**on_update**
Lifecycle hook. Fires when component's own props change.
Receives previous props.

---

## P

**Physical Sides**
Padding/margin values that never mirror with RTL:
.padding_left() always = left. .padding_right() always = right.

**Priority**
Batching priority for atom updates:
Immediate (bypasses batch), Normal (default, batched), Background (deferred).

**Provider**
See AtomProvider. Makes a scoped atom available to a subtree.

---

## R

**RefreshEngine**
The system that minimizes component rebuilds. Collects dirty
components, prunes descendants, rebuilds from roots only.
Guarantees each component rebuilds at most once per frame.

**RenderObject**
The layer below Element. Handles layout (sizing), painting (by emitting
[`DrawCommand`](../rosace-render/src/draw_command.rs)s — never touching
pixels directly, see [**rasterization**](#rasterization)), and hit
testing. Created from Element during reconciliation.

**RingBufferSubscriber**
A TraceSubscriber that keeps the last N RosaceTrace events in memory.
Enables time travel debugging. Default capacity: 1000 events.

**RTL**
Right-to-left text direction. Used by Arabic, Hebrew, Persian.
ROSACE handles RTL automatically when locale is set.

---

## S

**SemanticNode**
The accessibility tree node. Created from RenderObject.
Bridges to platform accessibility APIs (UIAccessibility, ARIA etc.).

**SharedMemory**
A memory region shared between ROSACE and native FFI code.
Used for the synchronous platform bridge hot path.

**Skia / tiny-skia**
"Skia" is Google's C++ 2D graphics library (Flutter, Chrome). ROSACE
does **not** use it. Decision D032 said "Skia" but the code actually
ships [`tiny-skia`](https://github.com/RazrFalcon/tiny-skia) — a
pure-Rust re-implementation of Skia's *software* (CPU) backend: no
C++, no GPU. It rasterizes each [`DrawCommand`](../rosace-render/src/draw_command.rs)
into an RGBA [**pixmap**](#pixmap--rgba-buffer) on the CPU. As of
Phase 27 (D109) shapes and text are migrating to the GPU
([**wgpu**](#wgpu)); `tiny-skia` stays as the CPU/web fallback. See
[render-pipeline.md](architecture/render-pipeline.md) for the two modes.

---

## T

**RosaceApp** — *renamed; the real type is `App`.*
The root builder for a ROSACE application is
[`App`](../rosace/src/lib.rs): `App::new().title(..).size(w, h).launch(Component)`.
Configures theme/window and starts the event loop. ("RosaceApp" was the
old planned name and does not exist as a type.)

**RosaceComponent**
The core trait. Implement this to create a component.
Must implement build() → Element.

**RosaceError**
The standard error type used throughout ROSACE.

**RosaceRenderer**
A trait for custom render pipelines (Level 5 customization).
Allows bypassing Skia for game engines, 3D, WebGL etc.

**RosaceResult<T>**
Result<T, RosaceError>. Used for expected failures in components.

**RosaceTheme**
A derive macro for defining exhaustive theme token sets.
Partial theme (missing any token) = compile error.

**RosaceTrace**
The unified event enum emitted by all ROSACE systems.
Zero cost in production via #[cfg(debug_assertions)].

**TracingBus**
The central hub that receives RosaceTrace events and dispatches
to all registered TraceSubscribers. Global singleton.

**TraceSubscriber**
A trait for receiving RosaceTrace events.
Implementations: RingBuffer, Console, File, DevTools, IDE.

**rsc**
The ROSACE CLI binary. Short for ROSACE.
Commands: rsc dev, rsc build, rsc test, rsc analyze, rsc snapshot.

---

## U

**use_async**
A hook that fetches data asynchronously on mount.
Returns AsyncState<T>. Cancels automatically on unmount.

**use_atom**
A hook that reads an atom and subscribes to its changes.
The component rebuilds when the atom changes.

**use_before_leave**
A hook that registers an async guard before navigation.
Returns NavigationDecision.

---

## W

**WidgetOverride**
A trait for replacing a built-in widget globally (Level 3 customization).
Receives original props and returns custom Element.

**WidgetScope**
A widget that applies widget overrides to its subtree only.
Inside: uses overridden widget. Outside: uses original.

---
---

# Graphics, GPU & Rendering — a plain-language primer

Everything above is a ROSACE invention. Everything below is *general*
computer-graphics vocabulary that [render-pipeline.md](architecture/render-pipeline.md)
and other chapters use. None of it is unique to ROSACE — but a textbook
shouldn't assume you already know it, so each term is explained from
scratch, then tied to where ROSACE actually uses it. Every entry has its
own heading so prose can link straight to it.

> **Wiki-style linking convention.** Each entry ends with a **📖 Wikipedia**
> link to the canonical, deeper treatment. The rule the docs follow: prose
> links a term to *our* entry here (the short, ROSACE-flavoured explanation),
> and our entry links onward to Wikipedia for the full theory. So a reader
> gets our plain-language version first and the authoritative source one click
> further — never a raw undefined acronym.

## The pixel foundation

### Rasterization
Turning a *description* of a shape (e.g. "a red rounded rectangle from
(10,10) to (200,60)") into the actual grid of coloured pixels that a
screen displays. The description is resolution-independent; the pixels
are not. In ROSACE, widgets never rasterize — they only emit
[`DrawCommand`](../rosace-render/src/draw_command.rs) descriptions, and a
*separate* stage rasterizes them: on the CPU via **tiny-skia** (see the
**Skia / tiny-skia** entry in the A–Z section above), or on the GPU via
[shaders](#shader--fragment-shader). Keeping "describe" and "rasterize" apart is
what lets ROSACE replay a recorded frame without re-running any widget.
*📖 [Wikipedia — Rasterisation](https://en.wikipedia.org/wiki/Rasterisation).*

### Pixmap / RGBA buffer
A plain block of memory holding one colour per pixel, row by row. **RGBA**
= four bytes per pixel: Red, Green, Blue, and Alpha (opacity). A
1920×1080 RGBA buffer is ~8 MB. `tiny-skia`'s `Pixmap` is exactly this;
ROSACE's CPU renderer draws into one, then hands it to the compositor to
upload to the GPU as a [**texture**](#texture).
*📖 [Wikipedia — RGBA color model](https://en.wikipedia.org/wiki/RGBA_color_model).*

### Anti-aliasing (AA)
Smoothing the jagged "staircase" edges you get when a smooth shape meets
a square pixel grid, by making edge pixels *partially* coloured instead
of fully on/off. A diagonal line without AA looks like stairs; with AA
the step pixels are dimmed proportionally to how much of the pixel the
line covers (see [**coverage mask**](#coverage-mask)). ROSACE's rounded
rects, text, and shadows are all anti-aliased.
*📖 [Wikipedia — Spatial anti-aliasing](https://en.wikipedia.org/wiki/Spatial_anti-aliasing).*

### Coverage mask
A grayscale image where each pixel's value (0.0–1.0) says *how much of
that pixel a shape covers* — the raw ingredient of [**anti-aliasing**](#anti-aliasing-aa).
A glyph rasterized by `fontdue` is a coverage mask: the letter's body is
1.0, its soft edges are fractional, everything else is 0.0. To draw it,
you multiply the mask by the text colour. Color-emoji glyphs skip the
mask (they're already full RGBA), which is why the render pipeline treats
them separately.
*📖 [Wikipedia — Alpha compositing](https://en.wikipedia.org/wiki/Alpha_compositing).*

## Resolution & colour

### Device pixel ratio / HiDPI / scale factor
On a "Retina" (HiDPI = high-dots-per-inch) display, one *logical* pixel
your code talks about maps to 2 (or 3) *physical* pixels on the glass.
The **scale factor** is that multiplier. ROSACE records every
`DrawCommand` in **logical** pixels and multiplies by the scale factor
only at the last moment (`play_picture`), so the same recorded frame is
crisp on a 1× or 2× screen with no per-widget math. See render-pipeline
step 6.
*📖 [Wikipedia — Pixel density (DPR / PPI)](https://en.wikipedia.org/wiki/Pixel_density).*

### Gamma & sRGB
Screens don't display brightness linearly, and human eyes aren't linear
either, so colour values are stored in a *non-linear* encoding called
**sRGB** (the standard for web/UI colour). The catch: math on colours
(blending, blurring, anti-aliasing) is only correct in **linear** space,
so the GPU must *decode* sRGB→linear before blending and *re-encode*
after. Getting this wrong doubles the curve and everything comes out too
dark — exactly the "double-gamma bug" fixed in the compositor (a
`#2B2D30` gray was rendering as `#606266`). The fix: tag the texture
`Rgba8UnormSrgb` so the GPU linearizes on read automatically.
*📖 [Wikipedia — sRGB](https://en.wikipedia.org/wiki/SRGB) · [Gamma correction](https://en.wikipedia.org/wiki/Gamma_correction).*

## The GPU model (wgpu)

### wgpu
The Rust library ROSACE uses to talk to the GPU
([wgpu.rs](https://wgpu.rs)). It's a thin, safe, cross-platform wrapper
over the OS's real GPU API — Metal on macOS/iOS, Vulkan on
Linux/Android, Direct3D on Windows — so ROSACE writes GPU code once and
the right backend is chosen at runtime (D072). The compositor and the
Phase 27 shape/text GPU paths are all wgpu.
*📖 [Wikipedia — WebGPU](https://en.wikipedia.org/wiki/WebGPU) (wgpu implements the WebGPU standard).*

### Texture
An image living in GPU memory that shaders can read. Uploading a CPU
[**pixmap**](#pixmap--rgba-buffer) to the GPU makes it a texture. Textures
are the compositor's currency: each on-screen layer is a texture, and a
clean (unchanged) layer keeps its texture across frames with *no*
re-upload — a big part of why an idle ROSACE app costs almost nothing
(D089).
*📖 [Wikipedia — Texture mapping](https://en.wikipedia.org/wiki/Texture_mapping).*

### Shader / fragment shader
A small program that runs *on the GPU*, in parallel, once per pixel (or
per vertex). A **fragment shader** computes the final colour of one
pixel. Instead of the CPU deciding each pixel's colour one at a time, the
GPU runs the shader on thousands of pixels simultaneously. ROSACE's
Phase 27 migration replaces CPU shape-drawing with fragment shaders (one
per shape kind) — see [**SDF**](#signed-distance-field-sdf).
*📖 [Wikipedia — Shader](https://en.wikipedia.org/wiki/Shader).*

### GPU pipeline (render pipeline)
The GPU's fixed recipe for one *kind* of draw: which shaders to run, what
vertex layout to expect, how to blend the result. Switching pipelines has
a cost, so ROSACE registers one pipeline per built-in shape and batches
all draws of that shape together. (`wgpu::RenderPipeline`.)
*📖 [Wikipedia — Graphics pipeline](https://en.wikipedia.org/wiki/Graphics_pipeline).*

### Uniform / uniform buffer
A **uniform** is a value that's the *same* for every pixel a shader
processes in one draw — e.g. "corner radius = 8" or "zoom = 1.5". A
**uniform buffer** is the little block of GPU memory holding those
values. Because it's separate from the geometry, you can change a uniform
(e.g. a scroll offset) with a tiny cheap write instead of re-uploading
anything. ROSACE passes shape parameters to its shaders as uniforms; the
compositor is a strict Layer 0 crate, so it receives them as raw
`Vec<u8>` rather than typed structs.
*📖 [Wikipedia — Shader § Uniforms](https://en.wikipedia.org/wiki/Shader).*

### Bind group
wgpu's bundle that tells a shader *where its inputs live* — "the texture
is here, the uniform buffer is there." You build it once and re-bind it
each frame. The compositor keeps a persistent bind group per layer
alongside its [**texture**](#texture), so a clean layer re-binds without
rebuilding anything.
*📖 [WebGPU spec — GPUBindGroup](https://www.w3.org/TR/webgpu/#gpubindgroup) (wgpu concept, no Wikipedia article).*

### Surface / swapchain / present
The **surface** is the actual window region the GPU draws into. Behind it
is a **swapchain**: a small set of textures the GPU cycles through — draw
into one while the screen shows another, then swap. **Present** = "this
frame is done, show it." ROSACE's frame loop ends each painted frame by
presenting the composited surface; a fully idle frame skips present
entirely.
*📖 [Wikipedia — Swap chain](https://en.wikipedia.org/wiki/Swap_chain).*

### UV mapping
The coordinate system for *sampling a texture*. A texture's own axes are
called **U** (horizontal) and **V** (vertical), each running 0.0→1.0
regardless of the texture's pixel size, so `(0,0)` is one corner and
`(1,1)` the opposite. To draw part of a texture you sample a **UV
window** — a sub-rectangle in UV space. A **UV offset** shifts that
window. This is the trick behind ROSACE's zero-cost scrolling: the
scrolled content is one big unchanging texture, and scrolling just moves
the UV offset the on-screen quad samples at — no repaint, no re-upload
(D090). Zooming works the same way: sample a *smaller* UV window of a
bigger texture and the GPU magnifies it crisply.
*📖 [Wikipedia — UV mapping](https://en.wikipedia.org/wiki/UV_mapping).*

### Texture atlas
One big texture packed with many small images, so the GPU can draw all of
them without the expense of switching textures between each. The classic
use is text: see [**glyph atlas**](#glyph-atlas).
*📖 [Wikipedia — Texture atlas](https://en.wikipedia.org/wiki/Texture_atlas).*

## Compositing

### Blend mode (Porter-Duff)
The rule for combining a new pixel with what's already underneath.
**Porter-Duff** is the standard family of these rules. ROSACE uses two:
**REPLACE** (the new pixel overwrites — used for the opaque bottom
layer) and **ALPHA_BLENDING** / *source-over* (the new pixel is laid over
the old one weighted by its [alpha](#pixmap--rgba-buffer) — used for every
overlay on top, e.g. a dialog over the page). Compositing bottom-to-top
with these two rules is how N layers become one image (D076).
*📖 [Wikipedia — Alpha compositing (Porter–Duff)](https://en.wikipedia.org/wiki/Alpha_compositing) · [Blend modes](https://en.wikipedia.org/wiki/Blend_modes).*

### Damage rectangle (damage-rect)
The smallest rectangle enclosing everything that *changed* since the last
frame. If only a button's hover state changed, you only need to re-draw
and re-upload that little rectangle, not the whole screen. ROSACE
accumulates the damage rect from every repainted widget (inflated a few
px to catch shadows/focus rings) and clears/replays only that region — a
CPU-buffer economy. Note it's switched *off* in GPU-shapes mode, where
the frame is an ordered item list with no partial-texture to replay.
*📖 (No single canonical article — the technique is "incremental / dirty-rectangle rendering"; see [Wikipedia — Repaint](https://en.wikipedia.org/wiki/Repaint) and graphics-engine literature.)*

## Vector shapes

### Signed distance field (SDF)
A way to describe a shape mathematically instead of as pixels: for any
point, a function returns its *distance* to the shape's edge — negative
inside, positive outside, zero exactly on the edge. A fragment
[**shader**](#shader--fragment-shader) can evaluate this per-pixel and get
a perfectly [anti-aliased](#anti-aliasing-aa), resolution-independent
edge for free (the distance *is* the coverage). ROSACE's Phase 27 GPU
shapes are SDF-based: a rounded rectangle is a handful of math ops in a
shader, not a rasterized mesh — crisp at any zoom, cheap to animate.
*📖 [Wikipedia — Signed distance function](https://en.wikipedia.org/wiki/Signed_distance_function).*

### Tessellation
The *other* way to put a vector shape on a GPU: chop it into lots of tiny
triangles (the only thing GPUs draw natively) and upload those. It's the
traditional approach (and what a headless crate like `lyon` produces).
ROSACE's Phase 27 prefers [**SDF**](#signed-distance-field-sdf) over
tessellation for its built-in shapes, but tessellation is the natural
adapter path for importing arbitrary vector art (charts, maps).
*📖 [Wikipedia — Tessellation (computer graphics)](https://en.wikipedia.org/wiki/Tessellation_(computer_graphics)).*

## Text & typography

### Glyph
The actual drawn shape of a character in a specific font — the letter "a"
in Helvetica Bold at 16px is one glyph. A character (the abstract "a") is
not a glyph; the font maps characters to glyphs. ROSACE rasterizes each
distinct glyph once with `fontdue` and caches it (see below).
*📖 [Wikipedia — Glyph](https://en.wikipedia.org/wiki/Glyph).*

### Glyph atlas
A [**texture atlas**](#texture-atlas) specialized for text: every glyph
the app has drawn is packed into one GPU texture, so a paragraph is drawn
as many small quads all sampling that one atlas — one pipeline, one
texture, thousands of letters. Each glyph is keyed so it's rasterized and
uploaded only once no matter how many times it appears. [**LRU**](#lru-cache)
eviction reclaims space when the atlas fills.
*📖 [Wikipedia — Texture atlas](https://en.wikipedia.org/wiki/Texture_atlas).*

### Baseline, bearing, kerning
The geometry of placing glyphs in a row. The **baseline** is the line
letters sit on (the tail of "y" hangs below it). **Bearing** is a glyph's
offset from its own start point to where its ink actually begins (so
letters don't jam together). **Kerning** is a per-pair nudge for
better-looking spacing (the "A" and "V" in "AV" tuck closer). ROSACE
computes all three once in [`layout_glyphs`](../rosace-render/src/font.rs),
shared by both the CPU and GPU text paths so they can't drift apart.
*📖 [Wikipedia — Kerning](https://en.wikipedia.org/wiki/Kerning) · [Baseline (typography)](https://en.wikipedia.org/wiki/Baseline_(typography)).*

## Caching

### LRU cache
**Least-Recently-Used** cache: a fixed-size store that, when full, evicts
whatever hasn't been touched in the longest time to make room for a new
entry — betting that recently-used things will be used again soon. It's
the standard policy for caches with a memory budget. ROSACE uses LRU for
the [**glyph atlas**](#glyph-atlas), the text [layout cache](#l), and
`KeepAlive`'d subtrees — each keeps hot items and drops cold ones instead
of growing without bound.
*📖 [Wikipedia — Cache replacement policies § LRU](https://en.wikipedia.org/wiki/Cache_replacement_policies#Least_recently_used_(LRU)).*
