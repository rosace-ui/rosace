# Post-Release TODO (captured ideas, not started)

Things to do AFTER the first release. Recorded so they're not forgotten.

## rsc new — bundle a free AI-context file into every scaffolded app
`rsc new <app>` should generate an AI-context Markdown (e.g. `AGENTS.md` /
`ROSACE_CONTEXT.md` / `.cursorrules`-adjacent) inside the new project, so a human
OR an AI assistant building that app has ROSACE's essentials up front and does
NOT waste tokens searching the framework:
- The supported widget catalog (names + one-liners + the common builder shape).
- The widget system + core patterns (Component/build/Context, `ctx.state` atoms,
  interactive-by-identity, the Quality-Bar conventions, theming/skin registry).
- Links to the docs (the GitHub Wiki: Guide / Architecture / Glossary).
- Kept current: generated from the same source the framework's own
  `AI_SNAPSHOT.md` idea uses, so it never drifts.
**Bundled with the CLI** (shipped in `rosace-cli`, emitted by `new.rs`), version-
matched to the SDK the app scaffolds against. Complements (does not replace) the
framework-repo `AI_SNAPSHOT.md`: that one is for people hacking on ROSACE; this
one ships INTO each generated app for people/AI building WITH ROSACE.
Requested 2026-07-24.
