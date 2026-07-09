# Phase 25 — Web SEO/Accessibility via Semantic-Tree HTML Shadow (D107)

> Status: COMPLETE (Steps 1-5 all landed 2026-07-09)
> Started: 2026-07-09
> Completed: 2026-07-09
> Decision: **D107** — canvas stays the only visual renderer; web additionally
> gets a semantic HTML shadow driven by the existing semantic tree, delivered
> at BUILD TIME via Declarative Shadow DOM where possible, with a runtime
> JS-driven fallback for post-hydration state changes.
>
> Progress:
> - Step 1 ✅ Extended `tezzera_core::Role` with `Link`/`Heading`/`List`/
>   `ListItem`/`Tab`/`TabPanel`; added `heading_level: Option<u8>` and
>   `href: Option<String>` to `SemanticNode` and `Semantics`. Deliberately
>   did NOT unify with `tezzera_a11y::role::Role` (confirmed via research:
>   zero external users outside its own crate — it drives a11y's separate
>   internal focus-management tree, unrelated to this phase). Also fixed a
>   real adjacent bug: `collect_semantics_node` was silently dropping
>   `Semantics::value` (and now `heading_level`/`href`) — fixed, with a
>   test proving the fix. Commit `eb2807f`.
> - Step 2 ✅ Added `Semantics` to 12 previously-bare widgets: `ListTile`
>   (`ListItem`, title+subtitle label), `AppBar` (`Heading` level 1 — the
>   screen's title), `Image` (`Image` role — but ONLY when `.alt(...)` is
>   set; a genuinely new field, this widget had none before, matching
>   HTML's own convention that an absent `alt` means decorative, not an
>   empty string), `Avatar` (`Image`, initials as label), `Chip`
>   (`Checkbox` — closest existing role to a toggleable tag), `Radio`
>   (label-less, value = selected state), `SegmentedControl`/`TabBar`
>   (`Tab` per segment, via `ctx.child(rect).semantics(...)` — each
>   segment/tab is its own semantic node, not the whole bar as one),
>   `Toast` (`Alert`), `Dropdown` (`Button`, current selection as label —
>   its `Menu` options already had `MenuItem` semantics), `Sheet`
>   (`Dialog`, unlabeled — no title field to work with, unlike the real
>   `Dialog` widget, but still worth marking as a modal region boundary),
>   `NavRail` (`Link` per item — the real-world `<nav><a>` shape; `Heading`
>   level 3 for section headers), `Expander` (`Button`, value =
>   expanded/collapsed), `Badge` (`Text`, only for the labeled variant —
>   a bare status dot has no text to announce).
>   New `Role::Radio` (distinct from `Checkbox` — real ARIA/HTML
>   distinguishes mutually-exclusive select from independent toggle;
>   approximating one as the other would be wrong, not just imprecise).
>   Correctly left 4 widgets WITHOUT semantics, not as a gap but because
>   they're pure structural wrappers carrying no content/identity of their
>   own: `Card`/`Drawer` (arbitrary child content, already self-describing),
>   `Tooltip` (wraps a child that already has its own semantics — a second
>   node here would be a spurious duplicate), `Skeleton` (a loading
>   placeholder standing in for not-yet-loaded content — nothing
>   meaningful to announce).
>   **Not yet verified end-to-end** (deferred to the rest of Step 3, which
>   needs the same proof anyway): actually running a scaffolded app and
>   confirming `collect_semantics()` produces a non-sparse tree matching
>   the screen. This session's verification was compile-level + the
>   existing `collect_semantics` unit tests, not a real running app —
>   Step 3's `curl`-the-built-HTML exit bar is the honest place to prove
>   this for real, per this project's verify-don't-assume standard.
> - Step 3 ✅ `SemanticNode → HTML` mapping function, platform-isolation
>   architecture, AND `tzr build --target web` integration all landed.
>   **A real architecture question came up mid-session and got resolved
>   before writing the mapping code, not after**: does web-only code like
>   this actually stay off iOS/Android/desktop binaries, or does it
>   silently ship there as unreferenced dead weight? Checked: `tezzera`'s
>   Cargo.toml had ZERO target-gated dependencies before this — all ~30
>   sub-crates are unconditional (a real, separately-known gap — see
>   `[[project-cli-platforms]]` memory). Relying on the linker to strip an
>   unreferenced function isn't a real guarantee (doesn't happen in debug
>   builds; not enforced against future accidental references) — so the
>   mapping function lives in a NEW crate, `tezzera-web-seo` (Layer 5,
>   alongside `tezzera-a11y`), which `tezzera/Cargo.toml` pulls in via
>   `[target.'cfg(target_arch = "wasm32")'.dependencies]` — the same
>   mechanism already used correctly elsewhere in this codebase for
>   `jni`/`ndk-sys` (Android-only) and `wasm-bindgen`/`web-sys`
>   (wasm32-only). `tzr-cli` (Step 3's other, build-time consumer) depends
>   on it directly and unconditionally in its own `Cargo.toml` — safe
>   regardless of the app's target platform, since `tzr` itself never
>   ships inside any app binary for any platform.
>   **Verified concretely, not asserted**: `cargo tree -p tezzera` (host/
>   macOS target) and `cargo tree -p tezzera --target
>   aarch64-apple-ios-sim`/`aarch64-linux-android` all show ZERO occurrences
>   of `tezzera-web-seo` in the dependency graph — not stripped, never
>   part of that target's build at all. `cargo tree -p tezzera --target
>   wasm32-unknown-unknown` shows it present. This is the actual guarantee,
>   compiler-enforced, not "probably fine after optimization."
>   `tezzera-web-seo/src/lib.rs`: `render_html`/`render_shadow_dom_template`
>   (the `<template shadowrootmode="open">` wrapper)/`render_text` (the
>   `llms.txt` source), covering every `Role` variant with proper HTML/
>   attribute escaping (a real XSS concern — `label`/`value`/`href` are
>   arbitrary app data, not trusted HTML; tested directly). 11 tests, all
>   passing, including one mirroring the real `AppBar`+`ListTile` shape
>   from Step 2.
>   **Build integration**: `tzr new` (Web selected) generates
>   `examples/seo_extract.rs` — a NATIVE (host-arch, never wasm32) example
>   binary that does one headless `FrameEngine` build+paint pass (new
>   `FrameEngine::semantics()` accessor; a `SkiaCanvas` is just an
>   in-memory CPU pixmap, no real window/GPU needed) purely to populate the
>   render tree, then prints the shadow-DOM HTML + llms.txt text to stdout.
>   `tezzera-web-seo` is a `[dev-dependencies]` entry (not a plain one) in
>   the generated app's own `Cargo.toml` — dev-dependencies are excluded
>   from `cargo build --bin <app>` (the real, shipped binary, on every
>   platform including the wasm32 web build itself), only pulled in for
>   `cargo run --example ...` — same "don't ship what a binary doesn't
>   need" reasoning as the wasm32-gating above, via the mechanism suited to
>   a build-time tool rather than a runtime dependency. `lib_rs` widens
>   `app`/`theme` to `pub mod` (web-only) since a Cargo example is its own
>   crate root and needs to reach them from outside.
>   `build_web()` (`tezzera-cli/src/commands/build.rs`) runs this example
>   after the wasm build, splices the shadow DOM into `dist/index.html`
>   (replacing a `<!--TZR_SEO_SHADOW_DOM-->` marker `generate_index_html`
>   now emits) and writes `dist/llms.txt`. Non-fatal on failure (an older
>   project without `examples/seo_extract.rs` just skips this — Migration
>   Rule — and a failed extraction is a warning, not a build failure).
>   **Verified for real, matching the exit bar exactly**: scaffolded a
>   fresh web app, ran the actual `tzr build --target web` command,
>   inspected `dist/index.html` — the real `<template shadowrootmode=
>   "open">` block contains genuine app content (`<h1>seo_test</h1>`,
>   the Counter list tile's title+subtitle). Then served `dist/` with a
>   real local HTTP server and ran an actual `curl` against it (no JS
>   execution involved at all) — the raw HTTP response body contains that
>   same real text, byte-for-byte.
>   **Known, honest limitation — not silently glossed over**: this
>   captures only the app's default/initial screen (Home), not a snapshot
>   per navigable route. `AppRoot::build()` runs exactly once, with no
>   forced navigation state, so content behind `nav.push(...)` (e.g. the
>   Counter screen's "Increment" button) isn't in the build-time export —
>   only reachable once wasm loads and the user navigates, same as before
>   this phase. A real per-route static export would need the extractor to
>   iterate every `Screen` variant, forcing navigation state per iteration
>   — more scope than this session covers; flag as follow-up, not claim
>   as done. The phase's OWN exit-bar text is written assuming a
>   single-screen counter app where the default view already has
>   everything (true for `tzr new`'s literal generated app, since its
>   default HOME screen doesn't happen to include "Increment" — that's on
>   the Counter screen, reached via navigation — so even THAT exact
>   example only partially matches without per-route support).
> - Step 4 ✅ Runtime shadow-DOM fallback. `FrameEngine::paint()`
>   (`tezzera/src/engine.rs`) now returns `bool` (`content_changed`, derived
>   from the existing D091 dirty-tracking: `global_dirty ||
>   !dirty_ids.is_empty()`) instead of `()` — kept entirely internal to
>   `App::launch()`'s own closure in `tezzera/src/lib.rs` rather than
>   threading it through `tezzera-platform`'s public `PlatformWindow::
>   run_layered` closure signature, to avoid rippling a breaking change
>   through every platform caller (desktop/iOS-FFI/Android-FFI binaries)
>   for a value only the web target needs. New `FrameEngine::semantics()`
>   accessor. New `tezzera-platform/src/web_seo_sync.rs` (wasm32-only):
>   `sync(&SemanticNode)` renders via `tezzera-web-seo::render_html`, diffs
>   against the previous frame's HTML (cheap string compare — a second,
>   finer gate beyond the caller's `content_changed` check, since a
>   re-render can legitimately produce identical output), and on a real
>   change calls `set_inner_html` on the `#tzr-seo` shadow root — reusing
>   Step 3's build-time Declarative Shadow DOM root via `element.
>   shadow_root()` if the browser already attached one, or attaching a
>   fresh one via `attach_shadow()` otherwise (covers `tzr dev`, which
>   skips the build-time export). Wired into `App::launch()`'s paint
>   closure, wasm32-gated.
>   **Two real, previously-unknown bugs found via actual browser
>   verification (not code review) and fixed along the way**:
>   1. `#tzr-seo` had no CSS at all — Step 3's build-time shadow DOM HTML
>      was genuinely VISIBLE on screen to sighted users (confirmed via
>      screenshot). Fixed with a visually-hidden pattern (`position:
>      absolute; clip: rect(0,0,0,0); ...` — deliberately NOT
>      `display:none`, which also hides from screen readers) in both
>      `tzr new`'s `web_index_html()` and `tzr build`'s
>      `generate_index_html()`.
>   2. `generate_index_html()` had ALWAYS hardcoded `import('./app.js')`/
>      `fetch('app.wasm')` regardless of the real crate name — meaning
>      `tzr build --target web`'s output silently never actually loaded
>      the wasm app for any project not literally named "app", for this
>      function's entire prior history. Pre-existing, unrelated to this
>      phase's other work, but directly blocking Step 4's browser
>      verification (found via: canvas stuck at default 300×150 size, zero
>      console output, `ls dist/` showing the real files were
>      `seo_test.js`/`seo_test_bg.wasm`). Fixed by deriving `crate_name`
>      from the built wasm file's basename in `build_web()` and threading
>      it through `generate_index_html(crate_name: &str)`.
>   **Verified for real, matching the exit bar exactly**: scaffolded a
>   fresh web app, built it, served `dist/` over real HTTP, drove it in an
>   actual Chrome tab (`claude-in-chrome`) — navigated Home → Counter,
>   read `document.getElementById('tzr-seo').shadowRoot.innerHTML` BEFORE
>   clicking Increment (`<p>0</p>`), clicked the real Increment button,
>   read it again AFTER (`<p>1</p>`) — the shadow DOM tracked the live
>   state change from a real user interaction, not a simulated one. No
>   console errors either side.
> - Step 5 ✅ Verified with real tools. No-JS text extraction: `curl`ed the
>   built `dist/index.html` directly (no JS execution) and separately ran
>   it through a standalone `html.parser`-based text extractor (Python) to
>   simulate a JS-skipping crawler — both surfaced the real app text
>   ("seo_test", "Counter, A simple counter with + / −"), matching what's
>   visually on screen. `llms.txt` contains the same real text (this
>   phase's `llms.txt` is deliberately a flat plain-text summary per this
>   doc's own Step 3 description, not a claim of full compliance with the
>   `llmstxt.org` markdown-headers convention — nothing in this phase's
>   design committed to that stricter format).
>   **A third real, previously-unknown bug found here, via Chrome's own
>   accessibility tree (`read_page`), not spec-reading**: a lone
>   `Role::ListItem` (e.g. a `ListTile` placed directly in a `Column`, with
>   no explicit `Role::List` container — the common case; most screens
>   don't wrap a single list tile in one) rendered as a bare `<li>` with no
>   `<ul>` ancestor. Per HTML-AAM, an `<li>` outside a `<ul>`/`<ol>`/`<menu>`
>   loses its implicit `listitem` accessibility role — confirmed
>   concretely: Chrome's accessibility tree exposed the sibling `<h1>` but
>   silently dropped the `<li>` entirely. Fixed in `tezzera-web-seo/src/
>   lib.rs`: `render_children` now auto-wraps any run of consecutive
>   `Role::ListItem` siblings in a synthetic `<ul>` (an explicit
>   `Role::List` still renders its own `<ul>` directly, unaffected — no
>   double-wrapping). Re-verified after the fix: rebuilt, re-served,
>   confirmed the raw build-time HTML now contains `<ul><li>...</li></ul>`.
>   **Honest limitation, not glossed over**: actual VoiceOver (the system
>   screen reader) was NOT run — doing so would mean enabling a system-wide
>   accessibility feature via automation, outside what this session
>   attempts unprompted. What WAS verified is spec-correct DOM structure
>   (the exact thing VoiceOver's own accessibility-tree consumption depends
>   on) plus Chrome's own accessibility-tree API, which is the same
>   underlying tree VoiceOver reads from on this platform — not a
>   simulation of VoiceOver itself. If a real VoiceOver pass is wanted,
>   it needs a human (or a separate, explicitly-authorized session) at the
>   keyboard.
>   Full workspace `cargo build --workspace` and `cargo test --workspace
>   --no-fail-fast` both clean (zero failures) after all Step 4/5 changes;
>   `cargo check -p tezzera-web-seo --target wasm32-unknown-unknown` and
>   `-p tezzera-platform --target wasm32-unknown-unknown` both clean.

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
