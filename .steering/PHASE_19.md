# Phase 19 — Frozen-Texture Scroll: TransformLayerEntry + Persistent GPU Texture

> Status: COMPLETE
> Started: 2026-07-01
> Completed: 2026-07-01

## Why This Phase

Phase 17 wired the WGSL offset uniform and TransformLayer widget (D080–D082).
Phase 18 gave ScrollView a reactive atom (D084). But both phases still re-render
child content every frame — the "UV-offset only, no CPU re-render" path was deferred (D086).

Phase 19 delivers it:
1. TransformLayer paints child into a SEPARATE PictureRecorder (not the main one)
2. The picture is stored as a TransformLayerEntry in PaintCtx
3. After the main pass, the platform replays each entry into its own SkiaCanvas
4. The compositor uploads the canvas as an EXTRA GPU layer with the scroll offset
5. On subsequent frames where the TransformLayer is not dirty: the platform
   re-presents the SAME SkiaCanvas pixel data — no CPU re-render, no re-upload

The "no re-upload" guarantee requires caching the GPU texture across frames, which
is the big Phase 19 addition to the compositor.

---

## Decisions

- **D087** — TransformLayerEntry in PaintCtx: `PaintCtx` gains
  `transform_entries: Rc<RefCell<Vec<TransformLayerEntry>>>`.
  A `TransformLayerEntry` contains:
  - `picture: tezzera_render::Picture` — recorded child draw commands
  - `child_size: Size` — natural size of the child
  - `viewport_rect: Rect` — screen-space rect the TL occupies
  - `scroll_x: f32, scroll_y: f32` — current scroll offset in logical px
  - `dirty: bool` — true on first render or when content atom changed
  During paint, `TransformLayer::paint` records child into a sub-recorder,
  finishes it, and pushes a TransformLayerEntry. It does NOT paint into the
  main recorder.

- **D088** — Platform TransformLayer pass: After the overlay pass, the
  platform replays each TransformLayerEntry's picture into a dedicated SkiaCanvas
  (allocated or reused from `AppState.transform_canvases: Vec<SkiaCanvas>`).
  The canvas is sized to child_size (with HiDPI scale applied). The compositor
  receives these canvases as additional layers after the overlay, with their
  per-layer UV offsets.

- **D089** — GPU texture caching: `GpuPresenter` gains
  `texture_cache: Vec<Option<wgpu::Texture>>` parallel to the transform_canvases
  vec. When `dirty == false` the cached texture is reused; `queue.write_texture`
  is skipped. Only the uniform buffer is re-written with the new offset. This is
  the true "zero re-upload on scroll" path.

- **D090** — ScrollView integration: `ScrollView::live` pushes a
  TransformLayerEntry instead of painting child directly. The scroll offset atom
  drives the UV offset in the entry — no CPU paint on scroll.

---

## Steps

### Step 1 — TransformLayerEntry type

New type in `tezzera-render`:
```rust
pub struct TransformLayerEntry {
    pub picture:       Picture,
    pub child_size:    Size,
    pub viewport_rect: Rect,
    pub scroll_x:      f32,
    pub scroll_y:      f32,
    pub dirty:         bool,
}
```

### Step 2 — PaintCtx gains transform_entries

```rust
pub transform_entries: Rc<RefCell<Vec<TransformLayerEntry>>>,
```
Initialised by the platform to an empty Vec wrapped in Rc<RefCell>. Passed
through `child()` like `hit_targets`.

### Step 3 — TransformLayer::paint records separately

```rust
fn paint(&self, ctx: &mut PaintCtx) {
    let mut sub = PictureRecorder::new();
    // paint child into sub at (0,0) with child_size constraints
    // ...
    let picture = sub.finish();
    ctx.transform_entries.borrow_mut().push(TransformLayerEntry {
        picture,
        child_size: measured_size,
        viewport_rect: ctx.rect,
        scroll_x: self.scroll_x.get(),
        scroll_y: self.scroll_y.get(),
        dirty: true,
    });
    // Paint placeholder clip rect into main recorder so layout/hit-testing knows
    // where the viewport is
    ctx.fill_rect(ctx.rect, Color::TRANSPARENT);
}
```

### Step 4 — Platform TransformLayer pass

After overlay pass, iterate `paint_ctx.transform_entries.borrow()`. For each:
- Resize or allocate `transform_canvases[i]` to match child_size × HiDPI
- `canvas.play_picture(&entry.picture, &font)` only if `entry.dirty`
- Append `CompositorLayer { pixels, width, height, opacity: 1.0, offset }`
  where `offset = (scroll_x / child_size.width, scroll_y / child_size.height)`

### Step 5 — GpuPresenter texture cache (D089)

`GpuPresenter` gains `texture_cache: Vec<Option<CachedTexture>>`. `present_layers`
accepts an optional `dirty` flag per layer; if false, the cached texture wgpu
handle is rebound without re-uploading. A cache entry is invalidated when the
canvas dimensions change.

### Step 6 — phase19_demo

50-item list inside a TransformLayer. Scroll is driven by buttons. A frame
counter (stored in an atom) shows how many CPU renders happened — should stay
at 0 during pure scroll steps (only the TransformLayer's dirty flag is false).

---

## Exit Criteria

```
□ TransformLayerEntry type defined in tezzera-render (or tezzera-widgets)
□ PaintCtx.transform_entries plumbed end-to-end
□ TransformLayer::paint records into sub-recorder, not main recorder
□ Platform: separate SkiaCanvas per transform layer, replayed when dirty
□ Compositor: additional GPU layers from transform_entries after overlay
□ GpuPresenter: texture cache — no re-upload when dirty == false
□ ScrollView::live uses TransformLayer path (D090)
□ phase19_demo shows frame counter unchanged on pure scroll
□ Workspace tests pass
□ D087–D090 in DECISIONS.md
```

---

## Approved dependencies (new)
- None

## DO NOT
- DO NOT add texture atlas (Phase 20)
- DO NOT support scale/rotation transforms (Phase 20)
