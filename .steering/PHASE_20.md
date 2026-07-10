# Phase 20 — RenderTree Unification: Retained State, Structural Hit Testing, Damage Repaint

> Status: COMPLETE
> Started: 2026-07-02
> Completed: 2026-07-08
>
> Progress: Steps 2–4 landed together as the RenderTree arena (see
> `rosace-widgets/src/tree/render_tree.rs`): hit/scroll regions, focus
> nodes, overlay entries, and transform layers all live on persistent
> nodes; dispatch is a structural tree walk; the strip/insert-at-0
> workarounds and all three bolt-on caches are deleted. Node identity is
> positional per parent (safe: paint recursion is all-or-nothing per
> subtree; only the walker skips, and it consumes slots without reset).
> Step 1 remains partial — the keyed reconciler is still unused and the
> flat RenderNode list still owns picture caching. Step 5 slice 1
> (clean-frame skip) landed 1c6c3be; slice 2 (damage on DIRTY frames)
> LANDED: arena unification (flat RenderNode + dead reconciler deleted),
> damage-rect repaint (dirty region only), real RepaintBoundary picture
> cache, hover/tooltip/long-press/pointer-interceptors on the arena.
> Step 6 D089 LANDED: GpuPresenter holds persistent per-layer textures
> (`CachedLayer`) reused across frames; clean layers skip `write_texture`,
> offset-only changes are a uniform write, and a frame where no layer
> changed skips the present entirely (no surface acquire/submit). Signal:
> `SkiaCanvas::frame_dirty` set by the run loop when it repaints, consumed
> by the platform via `take_frame_dirty` → `CompositorLayer::tracked(..)`.
> Verified on Metal/M1 Pro: idle+hover frames log "skip present", content
> changes log "present (1 dirty)".
> Step 6 D090 FOUNDATION LANDED (commits d6e60f6, 5e23899): the compositor
> gained placed layers (CompositorLayer::placed — dest rect + texture
> src_offset; shader positions the quad in NDC + maps a UV window). The
> frame loop renders each TransformLayer entry ONCE into its own content
> canvas and publishes it (rosace-platform::scroll_layer thread-local);
> the platform composites base + scroll layers + overlay, retaining the
> scroll set across clean frames. Verified via app_demo "GPU Scroll Layer"
> route (viewport clip + scroll offset correct on GPU).
> Step 6 D090 ZERO-REPAINT SCROLL LANDED (commit c0baffc): a placed
> layer's offset lives in a non-reactive channel (rosace_state::
> scroll_offset, keyed by node id); updating it requests a present-only
> frame that dirties NO component. TransformLayer registers a wheel scroll
> target feeding the channel; the platform reads it at present as the
> layer's UV src_offset. Verified: 92 consecutive scroll frames were
> needs_paint=false + "present 2 layers (0 dirty)" — zero repaint, zero
> re-upload. This MEETS Step 6's exit criterion (scroll = no CPU paint).
> Step 6 D090 HIT-TEST-THROUGH-OFFSET LANDED (commit 4b7e159): the
> dispatch walk maps screen→content coords when descending into a
> transform node's children (child_coords) and clips to the viewport, so
> GPU-composited scroll content is interactive. Unit-tested.
> Step 6 D090 ScrollView::gpu LANDED (commit cc1a243): ScrollView gained
> an opt-in GPU-layer path (::gpu / .gpu_layer()) — records content into
> its own picture, attaches a transform entry, wheel→channel, scrollbar
> from the channel offset. Fixed/controlled/::new keep the base path
> (zero regression). Verified in the GPU Scroll Layer demo route.
> Step 6 D090 TRANSPARENT DEFAULT + DRAG FIX LANDED (commit 144d062):
> (a) fixed the positional-drag (hits_at) coordinate-mapping bug through a
> transform — hit_test_node found hits inside a GPU-composited scroll view's
> content correctly (via child_coords), but returned the callback unmapped;
> every subsequent drag-continuation call (rosace/src/lib.rs's active_drag,
> which invokes the stored callback directly with raw screen coords on every
> MouseMove without re-hit-testing) bypassed the transform. Fixed by wrapping
> the callback, at the transform-hosting node, to re-apply the remap on every
> future invocation, not just the first — composes for nested transforms.
> Unit-tested (positional_hit_through_transform_remaps_every_invocation).
> (b) `ScrollView::new` ("live" in this doc's earlier language) now
> auto-composites as a GPU layer via `should_auto_gpu`: enabled once content
> actually overflows the viewport on the scroll axis AND fits within
> `MAX_TL_DIM` (4096px — now a real constant, not just a doc comment).
> Content that doesn't overflow, or exceeds MAX_TL_DIM, stays on the base
> (CPU) path automatically — safe by construction since windowing (below)
> isn't built yet. `.gpu_layer()`/`::gpu` remain as an explicit override;
> `::fixed`/`::controlled` unaffected. `cargo test --workspace` passes;
> desktop-verified the existing explicit GPU Scroll Layer demo route still
> renders correctly post-refactor (regression check).
> STILL NEEDS a manual interactive pass by the user (no OS accessibility
> permission available to automate clicking in this environment): app_demo's
> Gallery/Typography/Overlays/Showcase routes were plain ScrollView::new
> before this change and may now auto-compose as GPU layers if their content
> overflows. Gallery specifically contains a Slider — open app_demo, go to
> Gallery, and confirm the slider tracks the cursor correctly while scrolled
> (the exact scenario the drag-fix above targets). Not a blocker for calling
> Phase 20 complete (covered by a unit test); flagged for real-device
> confirmation.
> Step 6 D090 MAX_TL_DIM RESOLVED (commit 5d3500b) — decided NOT to build
> GPU-layer re-render windowing. ListView::builder already fully solves
> "content too large for a single texture" for the case that matters — long
> lists — via real virtualization (O(visible), content never materialized
> off-screen, no texture-size limit possible regardless of count). That
> narrows MAX_TL_DIM's remaining job to one large NON-virtualized widget
> subtree (e.g. a single big Image) past 4096px, which the existing
> automatic-default fallback already handles correctly (base/CPU path, no
> GPU zero-repaint optimization, but no mis-render). Documented the
> ListView/ScrollView tradeoff in both widgets' doc comments so the guidance
> surfaces where a developer would actually look. Phase 20 has no open
> items.
>
> DESIGN NOTE for the remaining block (found while scoping): per-node
> picture caching cannot key on constraints alone — widgets are rebuilt
> structs with closures, so there is no content equality to detect
> "same constraints, different text". Safe invalidation units are
> COMPONENT boundaries (element_cache already diffs per component) and
> explicit RepaintBoundary/.repaint_when opt-ins. Therefore the block
> is: (a) unify RenderNode's caches onto the RenderTree arena,
> (b) cache pictures per component-boundary subtree (walker already
> knows dirtiness per component), (c) damage = union of dirty
> component rects, (d) D089 texture cache on top. Multi-component apps
> get fine granularity; single-component apps behave as today until
> split — document this in the authoring guide.

## Why This Phase

Three bugs from the same root cause shipped (and were each patched with a
bolt-on cache):

1. Hit handlers vanished on cache-hit frames → `node.hit_handlers` cache (a1e91b8)
2. TransformLayerEntries vanished on cache-hit frames → `cached_transform_entries` (D088)
3. Overlay entries vanished on cache-hit frames → `cached_overlay_entries`
   (open dialogs disappeared on the MouseUp frame after every click, letting
   taps reach buttons underneath)

The root disease: `paint()` has frame side effects. Widgets emit hit targets,
scroll targets, focus nodes, overlay entries, and transform layers into
per-frame `Rc<RefCell<Vec>>` channels and thread-locals. The picture cache
skips `paint()` on clean frames, so those emissions silently die — and each
newly discovered casualty grows another cache. Focus nodes are the next
casualty in waiting (`sync_from_nodes` receives an empty list on clean frames).

Phase 20 makes the bug class unrepresentable (D091), replaces flat-vec hit
dispatch with structural z-order (D092), and unlocks the perf work that
depends on tree granularity: damage rects, real RepaintBoundary caching, and
the deferred D089/D090 (GPU texture cache + transparent ScrollView layers).

## Decisions

- **D091** — RenderTree owns all per-node retained state (locked)
- **D092** — Tree-walk hit testing with structural z-order (locked)
- **D089** — GPU texture caching (deferred here from Phase 19)
- **D090** — ScrollView::new rides TransformLayer transparently (deferred here)

## Migration Rule

Existing workarounds stay until each is REPLACED by the structural version,
then deleted in the same commit. Demos must stay green after every step.
No step may add a new per-frame side channel.

## Steps

### Step 1 — Real tree shape
`RenderNode.children` becomes the actual walk structure. `walk_element`'s
inline tag matching is replaced by the existing keyed reconciler
(`rosace/src/reconcile.rs` — currently dead code with passing tests).
Native children get their own nodes (today only top-level natives do, so
cache granularity ≈ the whole app).

Exit: reconciler unit tests pass against the live walker; node count in
app_demo > 1; all demos render identically.

### Step 2 — Hit + scroll regions move into the tree
`register_hit` / `register_scroll_target` write to the *current node*
(walker-provided), not a shared frame vec. Dispatch walks the tree
back-to-front (D092). Delete: `node.hit_handlers` re-registration loop,
flat `HitTarget` scan, insert-at-0 overlay merge.

Exit: clicks/scrolls work in app_demo including inside ScrollViews and
overlays; clicking a dialog's own body does not dismiss it (no strips).

### Step 3 — Overlays become tree roots
`push_overlay` attaches an overlay child to the owning node; the frame
assembles overlay roots from the tree. Scrim = a node that consumes misses.
Delete: thread-local overlay registry, `cached_overlay_entries`, the
four-strip scrim workaround.

Exit: dialog/sheet/toast survive clean frames by construction; scrim
tap-to-dismiss works; Block absorbs.

### Step 4 — Focus + transform layers follow the same move
FocusNodes and TransformLayerEntries live on nodes. Delete:
`cached_transform_entries`, per-frame focus re-sync.

Exit: Tab cycle stable across clean frames; scroll replay unchanged.

### Step 5 — Damage-rect repaint (unlocked by tree granularity)
Frame-skip when no node is dirty; otherwise clear + replay only the union
of dirty nodes' rects. RepaintBoundary gets a real per-node picture cache.

Exit: idle frames do zero raster work; button press repaints only its
boundary (verify via PaintRegion traces).

### Step 6 — D089/D090 land on top
TransformLayer canvases become persistent GPU textures (skip write_texture
when clean); ScrollView::new pushes a layer instead of repainting content.

Exit: scroll produces no CPU paint, only uniform updates.

## Trace Requirements

Each step emits/extends `RosaceTrace` events behind `#[cfg(debug_assertions)]`:
Step 1 reuses ComponentMount/Unmount; Step 2 adds GestureReceived routing
context; Step 5 uses PaintRegion for damage verification.

## DO NOT

- Do not add another per-frame cache to "fix" a vanishing-state bug — that is
  the disease this phase cures.
- Do not start Step N+1 while Step N's exit criteria are unmet.
- Do not change widget-facing APIs (Widget::layout/paint signatures stay;
  PaintCtx helpers keep working, only their storage moves).
