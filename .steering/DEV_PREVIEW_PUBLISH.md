# Dev preview: crates.io publish plan

Scope decided 2026-08-04: full framework publish (not CLI-only), license
MIT OR Apache-2.0, version 0.1.0, AI-context doc bundling deferred to
post-release. Publishing is irreversible (crates.io has no delete, only
yank) — do not run any `cargo publish` command from this list without a
fresh, explicit go-ahead, even though the prep steps below are done.

## Why the full framework, not just the CLI

`rsc new` (`rosace-cli/src/commands/new.rs`, `framework_dep()`) already has
dual-mode dependency generation: path deps when a local framework checkout
sits next to `rsc`'s own binary, otherwise a crates.io version dep pinned to
`rsc`'s own `CARGO_PKG_VERSION`. That means a scaffolded app only builds for
someone who installed `rsc` from crates.io if the full dependency closure
(`rosace` and everything it re-exports) is *also* on crates.io at the same
version. The CLI's own closure is small (5 crates) but that's not the bar —
"install and scaffold something that builds" is.

## Prep completed (2026-08-04)

- [x] `LICENSE-MIT` / `LICENSE-APACHE` added at repo root.
- [x] `license = "MIT OR Apache-2.0"` already correct in `[workspace.package]` —
      only the README text was stale (fixed).
- [x] `description` added to all 34 crates that were missing it (required by
      `cargo publish`).
- [x] `rosace-thirdparty-material-test` marked `publish = false` — internal
      proof-of-concept crate, not part of the public surface.
- [x] README license section + badge updated to match reality.

## Still open before running any publish

- [x] Crate names confirmed available on crates.io (user checked manually
      2026-08-04, since scripted `curl` is blocked by their anti-bot policy).
- [x] crates.io API token already configured locally (`~/.cargo/credentials.toml`
      present, confirmed 2026-08-04) — `cargo publish` can authenticate.
- [ ] Dry-run each crate with `cargo publish --dry-run -p <crate>` in the
      order below, fixing whatever it surfaces (missing `readme`, path deps
      that don't yet have a matching published version, etc.) before any
      real publish.
- [ ] Decide `repository`/`homepage` fields are accurate (currently
      `https://github.com/rosace-ui/rosace` — confirm this repo is actually
      public before publishing, since crates.io will link to it).
- [ ] Explicit user go-ahead for the actual `cargo publish` run.

## Topological publish order (39 crates, computed 2026-08-04)

Path deps only; each crate must publish before anything that depends on it.

```
 1. rosace-view-syntax
 2. rosace-macros
 3. rosace-trace
 4. rosace-state
 5. rosace-core
 6. rosace-theme
 7. rosace-style
 8. rosace-layout
 9. rosace-render
10. rosace-scroll
11. rosace-animate
12. rosace-forms
13. rosace-nav
14. rosace-text
15. rosace-a11y
16. rosace-shader
17. rosace-widgets
18. rosace-hot-reload
19. rosace-cli
20. rosace-asset-codegen
21. rosace-web-seo
22. rosace-compositor
23. rosace-ime
24. rosace-platform
25. rosace-nav-anim
26. rosace-devtools
27. rosace-gesture
28. rosace-net
29. rosace-file
30. rosace-storage
31. rosace-shaping
32. rosace-bidi
33. rosace-i18n
34. rosace-clipboard
35. rosace-ws
36. rosace-media
37. rosace-test-utils
38. rosace
39. rosace-ffi
```

`rosace-thirdparty-material-test` is excluded (`publish = false`).

Note `rosace-cli` (#19) doesn't need to wait for `rosace` (#38) — its own
dependency closure is just `rosace-core`, `rosace-hot-reload`,
`rosace-trace`. It's placed here purely by the graph's natural order, not
because it needs to come late.

## After a real publish

- Flip the README's "Not yet on crates.io" note (`## Getting Started` →
  `### rsc CLI`) to make `cargo install rosace-cli` the primary instruction,
  keeping `cargo install --path rosace-cli` as the source-build alternative.
- Bundle the AI-context markdown doc into `rsc new` scaffolds
  (`.steering/POST_RELEASE_TODO.md`) — separate, deliberately deferred item.
