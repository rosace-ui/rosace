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

### Atom
ROSACE's core state primitive — a reactive value of type `T`
([`atom.rs`](../rosace-state/src/atom.rs)). Changing it via `.set()` or
`.update()` schedules every subscriber [component](#component) for
rebuild. The smallest unit of state; read it with [`use_atom`](#use_atom).
See [state-and-reactivity.md](architecture/state-and-reactivity.md).

### AtomId
A unique identifier for an atom instance, used by the
[RefreshEngine](#refreshengine) and the [tracing](#tracingbus) system.

### AtomProvider
*No matching symbol in the source — treat as aspirational.* The intended
idea: a widget that makes a scoped [atom](#atom) available to its subtree,
with multiple isolated providers per atom. See also [Provider](#provider).

### AsyncState
The five states an async value moves through
([`async_state.rs`](../rosace-state/src/async_state.rs), D009): `Idle`,
`Loading`, `Success(T)`, `Error(`[`RosaceError`](#rosaceerror)`)`,
`Refreshing(T)`. Note: this is a **data enum only** — there is no
`use_async` hook that drives it (see [use_async](#use_async)).

### AxisBound
The constraint on one layout axis
([`render_object.rs`](../rosace-core/src/render_object.rs)):
`Bounded(f32)` (exact max), `Unbounded` (scroll axes), or `Shrink` (take
only needed space). The building block of [Constraints](#constraints).

---

## B

### Batch
A group of [atom](#atom) changes that triggers only one rebuild —
automatic within a sync block, manual via `batch()`. See [Priority](#priority).

### BiDi
Bidirectional text — mixed left-to-right and right-to-left in one
paragraph (e.g. an English word inside an Arabic sentence). ROSACE's
handling lives in `rosace-bidi`; full complex-script shaping is deferred
to v1.0 (see [HarfBuzz](#harfbuzz)).
*📖 [Wikipedia — Bidirectional text](https://en.wikipedia.org/wiki/Bidirectional_text).*

---

## C

### ChildContainer
A trait implemented by multi-child widgets (`Column`, `Row`, `Stack`…)
providing `.child()`, `.children()`, `.builder()`, `.child_if()`,
`.prepend()`, `.append()`, `.append_many()`.

### Component
The core trait every UI type implements
([`component.rs`](../rosace-core/src/component.rs)): `trait Component:
Send + Sync + 'static { fn build(&self, ctx: &mut `[`Context`](#context)`)
-> `[`Element`](#element)` }`. (Older docs called this `RosaceComponent`
— that name does not exist; the real trait is `Component`.) See
[core.md](architecture/core.md).

### ComponentId
A unique identifier for a component instance in the tree
([defined in `rosace-trace`](../rosace-trace/src/event.rs)) — its DFS
position (D001). Used for identity tracking, [Key](#key) resolution,
[tracing](#tracingbus), and to key [atoms](#atom) so state survives
rebuilds and hot-reloads.

### Constraints
The layout instruction passed parent→child
([`constraints.rs`](../rosace-layout/src/constraints.rs)): `min/max_width`
and `min/max_height`, each an [AxisBound](#axisbound). Consumed by
[Flexure](#flexure); see [render-pipeline.md](architecture/render-pipeline.md).

### Context
The build context passed to [`Component::build()`](#component)
([`context.rs`](../rosace-core/src/context.rs)) — access to local state
(`ctx.state(default)`), [lifecycle hooks](#on_mount), and services.

### cosmic-text
*NOT used (historical).* An early design named `cosmic-text` as the
text-layout library; it was never adopted. The real stack is `fontdue`
(glyph rasterization) + `ttf-parser` (font parsing) with ROSACE's own
[glyph](#glyph)-placement walk
([`layout_glyphs`](../rosace-render/src/font.rs)) and a first-fit
`FallbackShaper` (one glyph per character — full shaping deferred, D014).
Kept so old references resolve to the truth. See [Skia / tiny-skia](#skia--tiny-skia).

---

## D

### Derived Atom
*No matching symbol in the source — treat as aspirational.* Intended: an
[atom](#atom) computed from other atoms, lazily recomputed on read and
auto-invalidated when its sources change.

### Dirty
A component or screen region that needs rebuilding or repainting. Marked
dirty by the [RefreshEngine](#refreshengine) when a subscribed
[atom](#atom) changes; drives frame-skip (see
[render-pipeline.md](architecture/render-pipeline.md)).

### DFS Timestamp
Depth-first-search entry/exit timestamps in the [RefreshEngine](#refreshengine)'s
tree index, giving O(1) ancestor lookup when pruning [dirty](#dirty) subtrees.
*📖 [Wikipedia — Depth-first search](https://en.wikipedia.org/wiki/Depth-first_search).*

---

## E

### Element
A lightweight, immutable description of what a [component](#component)
wants to render, produced by `build()`. Cheap to create — the virtual
representation before layout and paint. See [core.md](architecture/core.md).

### ErrorBoundary
*No matching symbol in the source — treat as aspirational.* Intended: a
widget that catches panics/[`RosaceError`](#rosaceerror)s from its subtree
and shows a fallback UI instead of crashing.

---

## F

### Flexure
ROSACE's constraint-based layout engine
([`flexure.rs`](../rosace-layout/src/flexure.rs)) — three-pass layout
(measure → place → paint, D013/D014). Consumes [Constraints](#constraints),
produces sizes + child positions. See [render-pipeline.md](architecture/render-pipeline.md).

### ForeignBox
*No matching symbol found — verify before use.* Intended: an RAII wrapper
for memory allocated by external C code, calling the provided free
function on drop. (The FFI layer exists — see [SharedMemory](#sharedmemory)
— but this exact type wasn't found.)
*📖 [Wikipedia — RAII](https://en.wikipedia.org/wiki/Resource_acquisition_is_initialization).*

### FocusScope
*No matching symbol in the source — treat as aspirational.* Intended: a
widget managing keyboard focus — auto-focus first child, trap focus in a
subtree (for dialogs). (Focus itself exists in the engine; this wrapper
widget wasn't found.)

---

## G

### GlobalAtom
An [atom](#atom) with app-wide scope, reachable from any component
without a provider (used e.g. for [LifecycleState](#lifecyclestate)). Use
sparingly — only for truly global state.

### Glyph Cache
The cache of rasterized [glyphs](#glyph) — on GPU it's a
[glyph atlas](GLOSSARY.md#glyph-atlas) with [LRU](GLOSSARY.md#lru-cache)
eviction; each distinct glyph is rasterized once (`fontdue`). See the
graphics primer for the full mechanism.

---

## H

### HarfBuzz
*NOT used (deferred).* The industry-standard text *shaping* library
(Chrome/Firefox): turns Unicode characters into correctly positioned
glyphs, handling ligatures, [kerning](GLOSSARY.md#baseline-bearing-kerning),
and complex scripts (Arabic joining, Indic reordering). ROSACE does
**not** depend on it — today it uses a first-fit `FallbackShaper` (one
glyph per character); real shaping is deferred to v1.0 (D014).
*📖 [Wikipedia — HarfBuzz](https://en.wikipedia.org/wiki/HarfBuzz).*

---

## I

### IntrinsicHeight / IntrinsicWidth / IntrinsicSize
*The concept is real (D016 opt-in intrinsic sizing) but these widget
names weren't found in the source — verify before use.* Intended: force
a two-pass layout to measure children before sizing self; explicit
opt-in, zero cost when unused.

---

## J

### JIT (dev mode)
*Misleading — corrected.* ROSACE dev-mode reload is **not** WASM/JIT. It's
a three-tier system (data-swap → dylib-swap → hot-restart) — see
[hot-reload.md](architecture/hot-reload.md). No WASM module swapping is
involved on native; the web path rebuilds + rehydrates.
*📖 [Wikipedia — JIT compilation](https://en.wikipedia.org/wiki/Just-in-time_compilation) (for the general term).*

---

## K

### Key
An optional identifier attached to a component with `.key(value)`. Tells
ROSACE to track a component by key rather than tree position — required
for dynamic lists where order changes. See [ComponentId](#componentid).

### KeepAlive
Preserves a child's state even when removed from the active tree (e.g.
tab switching) — [`rosace-nav`](../rosace-nav/src/lib.rs). Memory budget
enforced via [LRU](GLOSSARY.md#lru-cache).

---

## L

### Layout Cache
A cache of text-layout results keyed by string + style + width;
invalidated when any input changes, so unchanged text isn't re-measured.
[LRU](GLOSSARY.md#lru-cache)-bounded.

### LifecycleState
The app-lifecycle states — `Active`, `Inactive`, `Background`,
`Suspended` (D110). Exposed as a [GlobalAtom](#globalatom). See
[platform-and-app-loop.md](architecture/platform-and-app-loop.md).

### Logical Sides
Padding/margin that respects text direction: `.padding_start()` = left in
LTR / right in [RTL](#rtl); `.padding_end()` the reverse. Contrast
[Physical Sides](#physical-sides).

---

## N

### NavigationDecision
The result of a navigation guard
([`route.rs`](../rosace-nav/src/route.rs)): `Allow`, `Block`, or
`RedirectTo(route)`.

---

## O

### on_mount
Lifecycle hook — fires once when a component enters the tree; return a
cleanup closure to run on unmount. See [Context](#context).

### on_unmount
Lifecycle hook — fires once when a component leaves the tree.

### on_update
Lifecycle hook — fires when a component's own props change; receives the
previous props.

---

## P

### Physical Sides
Padding/margin that never mirrors with [RTL](#rtl): `.padding_left()` is
always left, `.padding_right()` always right. Contrast
[Logical Sides](#logical-sides).

### Priority
[Batching](#batch) priority for atom updates: `Immediate` (bypasses the
batch), `Normal` (default, batched), `Background` (deferred).

### Provider
*See [AtomProvider](#atomprovider) — aspirational, no matching symbol.*

---

## R

### RefreshEngine
The system that minimizes rebuilds: collects [dirty](#dirty) components,
prunes descendants (via [DFS timestamps](#dfs-timestamp)), and rebuilds
from roots only — each component rebuilds at most once per frame. See
[state-and-reactivity.md](architecture/state-and-reactivity.md).

### RenderObject
The layer below [Element](#element): handles layout (sizing), painting
(by emitting [`DrawCommand`](../rosace-render/src/draw_command.rs)s —
never touching pixels directly, see
[rasterization](GLOSSARY.md#rasterization)), and hit testing. Created from
`Element` during reconciliation.

### RingBufferSubscriber
A [TraceSubscriber](#tracesubscriber) that keeps the last N
[RosaceTrace](#rosacetrace) events in memory
([`rosace-trace`](../rosace-trace/src/subscribers/ring_buffer.rs)) —
enables time-travel debugging. Default capacity 1000.

### RTL
Right-to-left text direction (Arabic, Hebrew, Persian). ROSACE mirrors
[Logical Sides](#logical-sides) when the locale is RTL.
*📖 [Wikipedia — Right-to-left script](https://en.wikipedia.org/wiki/Right-to-left_script).*

---

## S

### SemanticNode
The accessibility-tree node created from a [RenderObject](#renderobject),
bridging to platform a11y APIs (UIAccessibility, ARIA…). See `rosace-a11y`.

### SharedMemory
A memory region shared between ROSACE and native FFI code — the
synchronous platform-bridge hot path (D106). See `rosace-ffi` and
[platform-and-app-loop.md](architecture/platform-and-app-loop.md).

### Skia / tiny-skia
"Skia" is Google's C++ 2D graphics library (Flutter, Chrome). ROSACE
does **not** use it. D032 said "Skia" but the code ships
[`tiny-skia`](https://github.com/RazrFalcon/tiny-skia) — a pure-Rust
re-implementation of Skia's *software* (CPU) backend: no C++, no GPU. It
[rasterizes](GLOSSARY.md#rasterization) each
[`DrawCommand`](../rosace-render/src/draw_command.rs) into an RGBA
[pixmap](GLOSSARY.md#pixmap--rgba-buffer). Phase 27 (D109) migrates shapes
and text to the GPU ([wgpu](GLOSSARY.md#wgpu)); `tiny-skia` stays the
CPU/web fallback. See [render-pipeline.md](architecture/render-pipeline.md).

---

## T

### App
The root builder for a ROSACE application
([`rosace/src/lib.rs`](../rosace/src/lib.rs)):
`App::new().title(..).size(w, h).launch(root)`. Configures theme/window
and starts the frame loop. *(Older docs called this `RosaceApp`, which
does not exist as a type.)*

### RosaceComponent
*Renamed → see [Component](#component).* The core trait is named
`Component`, not `RosaceComponent`; this entry exists only so the old
name resolves.

### RosaceError
The standard error type ([`error.rs`](../rosace-core/src/error.rs)) used
throughout ROSACE. Paired with [RosaceResult](#rosaceresult).

### RosaceRenderer
*No matching symbol in the source — treat as aspirational.* Intended: a
trait for custom render pipelines ("Level 5 customization") to bypass the
renderer for game engines / 3D. Note: the "Level 1–5 customization"
framing appears only in this glossary, not in code or `DECISIONS.md`.

### RosaceResult
`Result<T, `[`RosaceError`](#rosaceerror)`>`
([`error.rs`](../rosace-core/src/error.rs)) — for expected failures.

### RosaceTheme
The theme-definition trait
([`theme.rs`](../rosace-theme/src/theme.rs)) for exhaustive theme token
sets — a partial theme (missing a token) is a compile error. See
[theming.md](guide/theming.md).

### RosaceTrace
The unified event enum emitted by every ROSACE system
([`rosace-trace`](../rosace-trace/src/lib.rs)); near-zero cost in release.
Dispatched by the [TracingBus](#tracingbus).

### TracingBus
The central hub that receives [RosaceTrace](#rosacetrace) events and
dispatches them to every registered [TraceSubscriber](#tracesubscriber)
([`rosace-trace`](../rosace-trace/src/lib.rs)) — a global singleton.

### TraceSubscriber
A trait for receiving [RosaceTrace](#rosacetrace) events.
Implementations: [RingBuffer](#ringbuffersubscriber), Console, File,
DevTools.

### rsc
The ROSACE CLI binary ([`rosace-cli`](../rosace-cli/src/main.rs)).
Real subcommands: `new`, `dev`, `run`, `build`, `package`, `test`,
`check`, `fmt`, `lint`, `analyze`, `doctor`, `devices`, `bundle-id`,
`snapshot`. See [cli.md](architecture/cli.md).

---

## U

### use_async
*No such hook exists — treat as aspirational.* Intended: a hook that
fetches data on mount, returning [AsyncState](#asyncstate), cancelling on
unmount. Today `AsyncState` is just the data enum with no driving hook
(confirmed in [state-and-reactivity.md](architecture/state-and-reactivity.md)).
The real async-fetch hook that *does* exist is `use_query` (`rosace-net`).

### use_atom
A hook that reads an [atom](#atom) and subscribes to its changes
([`use_atom`](../rosace-state/src/lib.rs)) — the component rebuilds when
the atom changes.

### use_before_leave
*No such hook exists — treat as aspirational.* Intended: register an async
navigation guard returning a [NavigationDecision](#navigationdecision).

---

## W

### WidgetOverride
*No matching symbol in the source — treat as aspirational.* Intended: a
trait to replace a built-in widget globally ("Level 3 customization").
The "Level 1–5" framing is glossary-only, not in code.

### WidgetScope
*No matching symbol in the source — treat as aspirational.* Intended: a
widget applying [WidgetOverride](#widgetoverride)s to its subtree only.

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
