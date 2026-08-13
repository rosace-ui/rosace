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
└── page/                   github.com/rosace-ui/rosace-page     ← own repo
                            the website (rosace.godwinj.com)
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
