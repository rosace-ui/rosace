# Post-Release TODO (captured ideas, not started)

Things to do AFTER the first release. Recorded so they're not forgotten.

## ~~rsc new — bundle a free AI-context file into every scaffolded app~~ SHIPPED
Promoted out of "deferred" 2026-08-04 (D129) — this should be the norm from
day one, not something only apps scaffolded by a future CLI get. `rsc new`
now emits `AGENTS.md` (widget catalog, core patterns, theming status, docs
links) and `CLI.md` (the `rsc` command reference) into every new project,
unconditionally. See `rosace-cli/src/commands/new.rs`'s `agents_md()`/
`cli_md()` and D129 in `DECISIONS.md` for the content plan and the "keep it
current" maintenance note.
