# Phase 9 — Text Shaping, Style System, CLI Polish

> Status: COMPLETE
> Target: v1.0 readiness — shaping prep, CSS-like styles, CLI completeness

## Steps

### Step 1 — Text shaping stub (`rosace-shaping`)
- `ShapedGlyph { glyph_id: u32, x_advance: f32, y_advance: f32, x_offset: f32, y_offset: f32, cluster: u32 }`
- `GlyphRun { glyphs: Vec<ShapedGlyph>, font_size: f32, direction: TextDirection, script: Script }`
- `Script` enum: `Latin`, `Arabic`, `Hebrew`, `Devanagari`, `Han`, `Unknown`
- `ShapingEngine` trait: `shape(text: &str, font_size: f32, direction: TextDirection) -> GlyphRun`
- `FallbackShaper` — stub impl that maps each char to a synthetic glyph using fontdue metrics; no ligatures
- `ShapingPipeline` — chains multiple engines with fallback
- Designed so HarfBuzz can be slotted in as a `ShapingEngine` impl in v1.0

### Step 2 — Style system (`rosace-style`)
- `StyleValue` enum: `Color(Color)`, `Length(f32, LengthUnit)`, `Percent(f32)`, `Keyword(String)`, `None`
- `LengthUnit` enum: `Px`, `Em`, `Rem`, `Vw`, `Vh`
- `StyleProperty` enum (20 properties): `Background`, `Color`, `FontSize`, `FontWeight`, `Padding`, `PaddingTop/Right/Bottom/Left`, `Margin`, `MarginTop/Right/Bottom/Left`, `Width`, `Height`, `BorderRadius`, `BorderWidth`, `BorderColor`, `Opacity`, `Display`, `FlexDirection`, `Gap`
- `StyleRule { selector: Selector, properties: HashMap<StyleProperty, StyleValue> }`
- `Selector` enum: `Id(String)`, `Class(String)`, `Element(String)`, `Any`
- `StyleSheet` — `Vec<StyleRule>`, `add_rule`, `rules_for(selector: &Selector)`, `merge(&StyleSheet)`
- `InlineStyle` — `HashMap<StyleProperty, StyleValue>` for per-widget styles
- `ComputedStyle` — merged result of stylesheet + inline, `get(prop)`, `color()`, `font_size()`, `padding_px()`

### Step 3 — rsc CLI polish
- Add `rsc check` — runs `cargo check --workspace`, prints errors in colored format
- Add `rsc test` — runs `cargo test --workspace`, shows pass/fail counts per crate
- Add `rsc lint` — runs `cargo clippy --workspace -- -D warnings`, shows lint count
- Add `rsc fmt` — runs `cargo fmt --workspace --check`, reports unformatted files
- Each command: `CommandResult { exit_code: i32, stdout: String, stderr: String, duration_ms: u64 }`
- `TzrCommand` trait: `name()`, `run(args: &[String]) -> Result<CommandResult, CliError>`
- Add subcommands to existing `rosace-cli` (read `rosace-cli/src/main.rs` first)

### Step 4 — Phase 9 showcase
- `rosace-examples/src/bin/phase9_demo.rs`
- 1400×900 PNG, 4 panels:
  1. Text shaping — GlyphRun visualization, FallbackShaper glyph map, Script enum
  2. Style system — StyleSheet rule diagram, StyleProperty grid, ComputedStyle resolver
  3. CLI commands — rsc check/test/lint/fmt flow diagram, CommandResult display
  4. Framework overview — all 25 crates in a dependency graph visualization

## Exit Criteria

- [ ] `FallbackShaper::shape("Hello", 14.0, TextDirection::Ltr)` returns 5 glyphs
- [ ] `GlyphRun::total_advance()` sums x_advance correctly
- [ ] `StyleSheet::rules_for(Selector::Class("btn"))` returns matching rules
- [ ] `ComputedStyle::color()` returns the resolved color value
- [ ] `ComputedStyle::padding_px()` returns the resolved padding
- [ ] `rsc check`, `rsc test`, `rsc lint`, `rsc fmt` subcommands registered in CLI
- [ ] All workspace tests pass, zero warnings, clean release build

## Approved dependencies

- No HarfBuzz — shaping is stub; prep only
- No CSS parser crate — hand-roll the property/value model
- No colored terminal crate — use ANSI escape codes directly in CLI output

## DO NOT

- DO NOT implement real ligature shaping — HarfBuzz swap in v1.0
- DO NOT implement a real CSS parser — property enum is sufficient
- DO NOT add cascading specificity scoring — merge order determines precedence
- DO NOT add media queries — v1.0
