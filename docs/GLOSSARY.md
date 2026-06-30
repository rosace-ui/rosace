# TEZZERA — GLOSSARY
> Every term defined precisely.
> When in doubt, check here first.

---

## A

**Atom**
TEZZERA's core state primitive. A reactive value of type T.
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
Idle, Loading, Success(T), Error(TezzeraError), Refreshing(T).

**AxisBound**
Describes the constraint on one axis: Bounded(f32) for exact max,
Unbounded for scroll axes, Shrink to take only needed space.

---

## B

**Batch**
A group of atom changes that triggers only one rebuild.
Automatic within sync blocks. Manual via batch() function.

**BiDi**
Bidirectional text. Mixed left-to-right and right-to-left
text in the same paragraph. Handled automatically by cosmic-text.

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
The build context passed to TezzeraComponent::build().
Provides access to local state, lifecycle hooks, and services.

**cosmic-text**
The Rust text layout library used by TEZZERA.
Handles BiDi, font fallback, and line breaking.
Used in tezzera-layout.

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
to render. Created by TezzeraComponent::build(). Cheap to create.
The virtual representation before layout and paint.

**ErrorBoundary**
A widget that catches panics and TezzeraErrors from its subtree
and shows a fallback UI instead of crashing.

---

## F

**Flexure**
The name of TEZZERA's constraint-based layout engine.
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

**HarfBuzz**
Industry-standard text shaping library. Used via harfbuzz-rs.
Same shaper used by Chrome and Firefox. Converts Unicode to
positioned glyph IDs.

---

## I

**IntrinsicHeight / IntrinsicWidth / IntrinsicSize**
Widgets that force a two-pass layout to measure children before
sizing themselves. Explicit opt-in — zero cost when not used.

---

## J

**JIT (dev mode)**
TEZZERA approximates JIT in dev mode via WASM hot-swap.
Component code changes → WASM module swapped → UI updates instantly.
Not true JIT — fast incremental recompile + hot-swap.

---

## K

**Key**
An optional identifier attached to a component with .key(value).
Tells TEZZERA to track this component by key rather than position.
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
The layer below Element. Handles layout (sizing), painting (Skia),
and hit testing. Created from Element during reconciliation.

**RingBufferSubscriber**
A TraceSubscriber that keeps the last N TezzeraTrace events in memory.
Enables time travel debugging. Default capacity: 1000 events.

**RTL**
Right-to-left text direction. Used by Arabic, Hebrew, Persian.
TEZZERA handles RTL automatically when locale is set.

---

## S

**SemanticNode**
The accessibility tree node. Created from RenderObject.
Bridges to platform accessibility APIs (UIAccessibility, ARIA etc.).

**SharedMemory**
A memory region shared between TEZZERA and native FFI code.
Used for the synchronous platform bridge hot path.

**Skia**
The 2D graphics library used by TEZZERA for rendering.
Same engine used by Flutter and Chrome. Provides pixel-perfect,
identical output across all platforms.

---

## T

**TezzeraApp**
The root builder for a TEZZERA application.
Configures theme, plugins, locale, and starts the event loop.

**TezzeraComponent**
The core trait. Implement this to create a component.
Must implement build() → Element.

**TezzeraError**
The standard error type used throughout TEZZERA.

**TezzeraRenderer**
A trait for custom render pipelines (Level 5 customization).
Allows bypassing Skia for game engines, 3D, WebGL etc.

**TezzeraResult<T>**
Result<T, TezzeraError>. Used for expected failures in components.

**TezzeraTheme**
A derive macro for defining exhaustive theme token sets.
Partial theme (missing any token) = compile error.

**TezzeraTrace**
The unified event enum emitted by all TEZZERA systems.
Zero cost in production via #[cfg(debug_assertions)].

**TracingBus**
The central hub that receives TezzeraTrace events and dispatches
to all registered TraceSubscribers. Global singleton.

**TraceSubscriber**
A trait for receiving TezzeraTrace events.
Implementations: RingBuffer, Console, File, DevTools, IDE.

**tzr**
The TEZZERA CLI binary. Short for TEZZERA.
Commands: tzr dev, tzr build, tzr test, tzr analyze, tzr snapshot.

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
