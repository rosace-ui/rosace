# Phase 25 — Web SEO/Accessibility via Semantic-Tree HTML Shadow (D107)

> Status: PLANNED (not started)
> Started: —
> Completed: —
> Decision: **D107** — canvas stays the only visual renderer; web additionally
> gets a semantic HTML shadow driven by the existing semantic tree, delivered
> at BUILD TIME via Declarative Shadow DOM where possible, with a runtime
> JS-driven fallback for post-hydration state changes.

## Why This Phase

The web target renders everything to `<canvas>` — same pipeline as native.
That makes the app invisible to search engines (no text/structure in a
canvas, same as a screenshot) and hurts Core Web Vitals (nothing paints until
wasm loads). The tempting fix — compile widgets to real HTML/CSS — is
actually a second, parallel widget renderer to maintain forever in lockstep
with the canvas one. Flutter tried exactly this (HTML renderer alongside
CanvasKit) and has been deprecating it for that reason. D105 already rejected
this shape of cost for platform theming (one API, two implementations); this
phase avoids repeating it for rendering.

TEZZERA already has the right ingredient: `RenderTree::collect_semantics()`
(`tezzera-widgets/src/tree/render_tree.rs:420`, built for D099 accessibility)
derives a nested `SemanticNode { role, label, children }` tree from widgets'
declared `Semantics` entries, in paint order, matching render-tree structure.
A search crawler and a screen reader want the same thing — real text and
structure, not pixels — so one semantic tree can serve both.

**2026 refinement over the original plan**: rather than only building the
HTML shadow at runtime (in-browser, after wasm loads), prefer generating it
at **build time** using **Declarative Shadow DOM** (`<template
shadowrootmode="open">`) — a real, now widely-shipped web-platform feature
that creates a shadow root straight from HTML, with no JavaScript required.
For any route whose content is knowable when `tzr build --target web` runs,
this bakes real, crawlable content into the literal HTML response — reaching
crawlers that skip JS execution, and improving first-paint (content is
visible before wasm even downloads). The runtime JS-driven shadow (the
original plan) becomes the fallback for state that changes after hydration.

## The Model

- Canvas remains the ONLY visual renderer, everywhere. No parallel DOM
  render backend for widgets.
- The SAME `SemanticNode`→HTML mapping is used two ways:
  1. **Build-time (preferred)**: `tzr build --target web` runs the mapping
     offline for statically-knowable routes/content and emits Declarative
     Shadow DOM directly in the generated HTML.
  2. **Runtime (fallback)**: after hydration, as app state changes in ways
     the build step couldn't know about, update the shadow tree live via
     `web_sys` DOM calls — mirrors the render tree's existing dirty-tracking
     rather than diffing from scratch every frame.
- Also export a per-route `llms.txt` / plain-text summary from the same
  semantic tree at build time — the emerging convention AI/LLM crawlers
  (Perplexity, GPT search, etc.) look for; nearly free once (1) exists.
- Crawlers and assistive tech read the real HTML/shadow DOM; sighted users
  see only the canvas. Nothing here touches desktop/iOS/Android.
- Full dynamic server-side rendering of app LOGIC (per-request personalized
  markup, not just the semantic tree) is explicitly OUT of scope — see D107.

## Steps

### Step 1 — Unify/extend the Role enum for real HTML semantics
Today there are two `Role` enums: `tezzera_core::semantic_node::Role` (the
one actually wired into `collect_semantics()` — Button/Text/Image/Slider/
Alert/Dialog/Checkbox/Switch/TextInput/MenuItem/ProgressBar/Unknown) and
`tezzera_a11y::role::Role` (richer — adds Link/Heading/List/ListItem/Tab/
TabPanel, has `is_interactive()`). Real HTML/SEO needs heading LEVELS
(`<h1>`–`<h6>`, not just "Heading") and real anchors (`<a href>`) — decide
whether to extend `tezzera_core::Role` with these or make `collect_semantics`
emit `tezzera_a11y::Role` instead, and add whatever `Semantics` needs (e.g.
`heading_level: Option<u8>`, `href: Option<String>`) to carry that data.

Exit: one Role source of truth, with enough data for a faithful HTML mapping
(headings, links, lists, buttons, text, images w/ alt).

### Step 2 — Comprehensive `Semantics` coverage across widgets
Today `Semantics` entries are declared sparsely (proven only in a
`render_tree.rs` unit test for `Button`). Every widget that carries
user-facing text or interactive behavior needs to push a `Semantics` entry:
`Text` → labelled text node, `Button`/`ListTile`/interactive widgets →
appropriate role + label, `Image` → alt text, etc.

Exit: `collect_semantics()` on a real app screen (e.g. the `tzr new` counter
app) produces a complete, non-sparse tree matching what's visually on
screen.

### Step 3 — Build-time semantic HTML export (`tzr build --target web`)
The `SemanticNode`→HTML mapping (Step 1) runs offline against a rendered
route's semantic tree and emits: (a) a Declarative Shadow DOM `<template
shadowrootmode="open">` block embedded in that route's HTML output, (b) a
companion `llms.txt`/markdown summary for AI crawlers. This is the preferred,
primary delivery mechanism — reaches JS-skipping crawlers, improves
first-paint.

Exit: `curl`-ing a built page's HTML (no JS execution) shows real text
content ("Counter", "A simple counter with + / −", "Increment") in the raw
response.

### Step 4 — Runtime shadow-DOM fallback (web-only, post-hydration)
A small module (likely in `tezzera-platform`'s wasm path) that walks
`SemanticNode` → creates/updates matching `web_sys` DOM elements for state
that changes AFTER hydration (the build step in Step 3 had no way to know
about it). Diffs against the previous frame's shadow tree — mirror the
render tree's existing dirty-tracking (D091) rather than rebuild wholesale
every frame.

Exit: after interacting with the live app (e.g. incrementing the counter),
the shadow DOM reflects the new state, verified via the DOM inspector /
accessibility tree, not just visually.

### Step 5 — Verify with real tools
Test with an actual crawler-like tool (`curl` + text-extraction, or a
headless browser with JS disabled to simulate a JS-skipping crawler) and a
real screen reader (VoiceOver), not just visual inspection — this is exactly
the kind of claim that needs measurement, not assumption (see
[[feedback_verify_dont_assume]] — same discipline that caught the iOS
view-frame bug applies here).

Exit: text-extraction from the built page's raw HTML (no JS) matches what's
semantically on screen; VoiceOver reads the same content and can interact
with buttons/links; the `llms.txt` export validates against the emerging
convention's expected format.

## Migration Rule

No effect on desktop/iOS/Android — this is entirely additive on the web
target. Apps that don't declare `Semantics` on custom widgets simply don't
appear in the shadow for that widget (degrades gracefully, doesn't crash);
Step 2 aims to make the built-in widget set complete so most apps get this
for free without authors doing anything.

## DO NOT

- Do not build a second visual renderer (HTML/CSS widget backend). Canvas
  stays the only pixels. This is the whole point of the decision.
- Do not attempt full dynamic per-request SSR of app logic as part of this
  phase — separate, bigger, and not everyone needs it (see D107's explicit
  scope boundary). The build-time export here only needs the semantic tree,
  not running components server-side.
- Do not rebuild the DOM shadow wholesale every frame at runtime — reuse/
  mirror the render tree's existing dirty-tracking (D091) rather than
  diff-from-scratch.
- Do not skip the build-time path (Step 3) and ship only the runtime
  fallback (Step 4) — the build-time path is what reaches JS-skipping
  crawlers and improves first-paint; runtime-only was the weaker, earlier
  version of this plan.
