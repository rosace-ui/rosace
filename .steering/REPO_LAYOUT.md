# Repo layout — what is tracked where

Everything ROSACE lives under one folder, but it is **three independent
git repos**, not one. This folder is the working source of truth; each
nested repo owns and publishes its own contents.

```
Tezzera/                    github.com/rosace-ui/rosace          ← THIS repo
├── rosace-*/               framework crates            tracked here
├── docs/                   mdBook guide + architecture tracked here
├── .steering/              decisions, plans, audits    tracked here
│
├── examples/               github.com/rosace-ui/rosace-examples ← own repo
│   └── showcase/           the widget catalog app
├── page/                   github.com/rosace-ui/rosace-page     ← own repo
│                           the website (rosace.godwinj.com)
└── wiki/                   github.com/rosace-ui/rosace.wiki     ← own repo
                            GENERATED from docs/ — never edit here
```

## The rule

`examples/` and `page/` are listed in this repo's `.gitignore`. This repo
never tracks, stages or commits a single file inside them. Each has its
own remote, own history, own branches — commit and push from inside the
directory:

```sh
git -C examples status      # NOT `git status` from the root
git -C page commit -am "..."
```

Running `git add -A` at the root is safe: the ignore entries mean the
nested repos cannot be swept in by accident.

## Why nested rather than siblings

Framework source, the app that exercises it, and the site that documents
it are read together constantly — a widget change, its showcase screen and
its docs page are usually one piece of work. Siblings in `~/Development`
meant three windows and a lot of `../../`. Nesting costs nothing because
the ignore entries keep the histories fully separate.

## The wiki is GENERATED, not a fourth source of truth

`github.com/rosace-ui/rosace/wiki` (backed by `rosace.wiki.git`) is the
published reader-facing copy. It is **output**:

```
docs/  ──  scripts/docs_to_wiki.py  ──▶  rosace.wiki.git
(edit here)                              (never edit here)
```

Edits made in the wiki UI are silently destroyed by the next regeneration.
That is the one rule worth remembering about it.

It is cloned in at `wiki/` for convenience; `.gitignore` covers `/wiki/`
and `*.wiki/`, so it cannot be swept into this repo:

```sh
git clone https://github.com/rosace-ui/rosace.wiki.git wiki   # if missing
python3 scripts/docs_to_wiki.py --wiki wiki --check   # dry run, link guard
python3 scripts/docs_to_wiki.py --wiki wiki           # write
git -C wiki commit -am "Regenerate from docs/" && git -C wiki push
```

Without those ignore entries, cloning it here would make this repo start
tracking a second copy of content it already owns — the same pages
committed twice, in two places, free to drift apart. That is the
double-tracking risk, and it is the reason the entries exist before the
directory does.

`--check` writes nothing and fails on any unresolvable inter-page link, so
it is the thing to run after editing `docs/`.

**Run `--check` before every release.** Nothing automates it — no CI job,
no git hook — so the published wiki drifts silently whenever `docs/` is
edited and the script is not run. It had drifted on 6 of 21 pages when the
wiki was first cloned in (2026-08-13), including a Glossary entry still
naming `fontdue` after the Phase 30 swap to `swash`. Readers were being
told the wrong text stack.

## `docs/` is NOT one of them

`docs/` is tracked by THIS repo. It is the mdBook source (guide +
architecture), not a nested sibling. The names are similar and the
distinction matters: edits under `docs/` get committed here; edits under
`page/` do not.

## Setting this up on a fresh machine

Cloning `rosace` alone gives you the framework with two empty holes. Fill
them:

```sh
git clone https://github.com/rosace-ui/rosace.git Tezzera
cd Tezzera
git clone https://github.com/rosace-ui/rosace-examples.git examples
git clone https://github.com/rosace-ui/rosace-page.git page
```

Deliberately NOT git submodules. A submodule pins a specific commit of
the child in the parent's history, so every example or site edit would
also need a pointer-bump commit here — friction on work that is already
tracked perfectly well in its own repo. The cost is that the clone above
is a manual step; that is the trade, and it is worth it.

## Caution: nothing outside these three is backed up

The showcase app sat untracked in BOTH repos for months (ignored by
rosace, never `git add`-ed in examples) — discovered 2026-08-13. If you
create a new top-level directory here, decide immediately which repo owns
it, or it belongs to none.
