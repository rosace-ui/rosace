# Phase 18 — ScrollView × TransformLayer + Multi-Layer Compositing

> Status: COMPLETE
> Started: 2026-07-01
> Completed: 2026-07-01

## Why This Phase

Phase 17 shipped `TransformLayer<W>` as a first-class widget and wired the GPU
offset uniform. Phase 18 closes the final ScrollView gap (D083): `ScrollView`
automatically routes through TransformLayer when the content fits within the
4096px cap, giving GPU-accelerated scroll without the user having to wrap
widgets manually.

Additionally Phase 17 was hard-coded to exactly two compositor layers (base +
overlay). Phase 18 removes that cap — the compositor can blend N layers in one
frame, enabling side-by-side independent scroll regions.

---

## Decisions

- **D084** — ScrollView auto-TransformLayer: `ScrollView` gains an `atom` for
  its scroll offset and delegates to `TransformLayer` internally. If the child
  natural size exceeds `MAX_TRANSFORM_DIM` the old CPU-clip path is used as a
  fallback. The `offset` builder is kept for static scenarios; if an `Atom<f32>`
  is provided it drives live scrolling.

- **D085** — N-layer compositor: The `present_layers` loop already supports
  arbitrary layers. Phase 18 exposes this to the platform: `run_layered` closure
  receives a `&mut Vec<TransformLayerData>` (one entry per active TransformLayer
  in the render tree). The platform clears this vec, the render walk pushes to
  it, and the compositor blends the layers in order after the overlay pass.

- **D086** — TransformLayer render-tree discovery: During `paint()` a
  `TransformLayer` pushes a `TransformLayerData { canvas, offset }` into
  `PaintCtx.transform_layers`. The platform allocates the canvas; the render
  walk populates it. After the primary pass the compositor assembles the final
  frame: `[base, overlay, tl_0, tl_1, …]`.

---

## Steps

### Step 1 — PaintCtx gains transform_layers Vec

Add `transform_layers: Vec<TransformLayerData>` to `PaintCtx` (with a platform-
provided Vec). `TransformLayer::paint()` paints the child into a sub-rect, pushes
`TransformLayerData` with current offset atom value.

### Step 2 — ScrollView delegates to TransformLayer

`ScrollView::new_live(child, scroll_atom)` — builds a `TransformLayer` internally
when content ≤ MAX_TRANSFORM_DIM. `ScrollView::new(child)` stays static offset.

### Step 3 — Platform plumbs N layers

After `paint_fn`, the platform reads `ctx.transform_layers`, appends them after
base+overlay in the `present_layers` call.

### Step 4 — phase18_demo

Two side-by-side `ScrollView` columns with independent scroll atoms. A third
panel shows a static diagram. All three are separate compositor layers.

---

## Exit Criteria

```
□ ScrollView accepts Atom<f32> for live scroll
□ TransformLayer::paint pushes to PaintCtx.transform_layers
□ Platform sends N layers to compositor after base+overlay
□ Two independent scroll regions work in phase18_demo
□ Workspace tests pass
□ D084–D086 in DECISIONS.md
```

---

## Approved dependencies (new)
- None

## DO NOT
- DO NOT add texture atlas or persistent GPU textures yet (Phase 19)
- DO NOT add scale or rotation transforms (Phase 19)
- DO NOT add WebGL/WASM backend (out of scope)
