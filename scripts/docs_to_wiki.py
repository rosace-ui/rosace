#!/usr/bin/env python3
"""Generate the GitHub Wiki pages from the in-repo `docs/` books.

The wiki (https://github.com/rosace-ui/rosace/wiki, backed by
rosace.wiki.git) is the published source of truth for readers, but the
EDITABLE source is `docs/` in this repo. This script converts one into the
other so `docs/` stays the place you edit and the wiki is regenerated.

Transform:
  * File rename: docs/architecture/core.md -> Architecture-Core.md, etc.
    (explicit mapping below — GitHub wiki pages are flat, hyphenated, and a
    couple of names aren't pure-algorithmic: cli->CLI, persistence-networking
    -> Persistence-and-Networking).
  * Inter-chapter .md links -> flat wiki page name, preserving #anchors.
  * Links into the repo (source files, dirs, DECISIONS.md, .steering/...)
    -> absolute GitHub blob URLs, resolved against the source file's dir.
  * External http(s) links -> unchanged.
  * Same-page anchor links -> kept as #anchor.

Usage:
    python3 scripts/docs_to_wiki.py --wiki /path/to/rosace.wiki [--check]

`--check` writes nothing; it just reports what would change and fails if any
inter-page link can't be resolved (a broken-link guard for CI/pre-push).
"""
import argparse
import os
import re
import sys

REPO = "rosace-ui/rosace"
BLOB = f"https://github.com/{REPO}/blob/main/"
DOCS = "docs"

# docs repo-relative path  ->  wiki page name (no .md)
PAGE_MAP = {
    "docs/architecture/README.md": "Architecture-Home",
    "docs/architecture/core.md": "Architecture-Core",
    "docs/architecture/state-and-reactivity.md": "Architecture-State-and-Reactivity",
    "docs/architecture/render-pipeline.md": "Architecture-Render-Pipeline",
    "docs/architecture/widget-protocol.md": "Architecture-Widget-Protocol",
    "docs/architecture/platform-and-app-loop.md": "Architecture-Platform-and-App-Loop",
    "docs/architecture/cli.md": "Architecture-CLI",
    "docs/architecture/hot-reload.md": "Architecture-Hot-Reload",
    "docs/guide/README.md": "Guide-Home",
    "docs/guide/getting-started.md": "Guide-Getting-Started",
    "docs/guide/components-and-state.md": "Guide-Components-and-State",
    "docs/guide/layout-and-widgets.md": "Guide-Layout-and-Widgets",
    "docs/guide/interaction.md": "Guide-Interaction",
    "docs/guide/navigation.md": "Guide-Navigation",
    "docs/guide/theming.md": "Guide-Theming",
    "docs/guide/forms-and-text.md": "Guide-Forms-and-Text",
    "docs/guide/animation.md": "Guide-Animation",
    "docs/guide/persistence-networking.md": "Guide-Persistence-and-Networking",
    "docs/guide/multi-platform.md": "Guide-Multi-Platform",
    "docs/guide/hot-reload.md": "Guide-Hot-Reload",
    "docs/GLOSSARY.md": "Glossary",
}

LINK_RE = re.compile(r"(!?\[[^\]]*\])\(([^)]+)\)")


def norm(path):
    """Normalize a repo-relative path (collapse .. and .)."""
    return os.path.normpath(path).replace(os.sep, "/")


def convert_target(target, src_repo_path, this_page, unresolved):
    """Rewrite one link target for the wiki. src_repo_path is the docs
    source file's repo-relative path (e.g. docs/architecture/core.md)."""
    if re.match(r"^[a-z]+://", target) or target.startswith("mailto:"):
        return target  # external
    frag = ""
    path = target
    if "#" in target:
        path, frag = target.split("#", 1)
        frag = "#" + frag
    if path == "":  # pure same-page anchor
        return target
    src_dir = os.path.dirname(src_repo_path)
    resolved = norm(os.path.join(src_dir, path))
    if resolved in PAGE_MAP:
        page = PAGE_MAP[resolved]
        if page == this_page:  # self-link -> same-page anchor
            return frag if frag else target
        return page + frag
    # Not a wiki page: a repo file/dir/steering doc -> blob URL.
    if resolved.startswith(".."):
        unresolved.append((src_repo_path, target))
        return target
    return BLOB + resolved  # (source links carry no #anchor)


def convert_file(src_repo_path, this_page, unresolved):
    with open(src_repo_path, encoding="utf-8") as f:
        text = f.read()

    def repl(m):
        return f"{m.group(1)}({convert_target(m.group(2), src_repo_path, this_page, unresolved)})"

    return LINK_RE.sub(repl, text)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--wiki", required=True, help="path to the rosace.wiki clone")
    ap.add_argument("--check", action="store_true", help="report only, write nothing")
    args = ap.parse_args()

    unresolved = []
    written = 0
    for src, page in PAGE_MAP.items():
        if not os.path.exists(src):
            print(f"  MISSING SOURCE: {src}", file=sys.stderr)
            continue
        out = convert_file(src, page, unresolved)
        dest = os.path.join(args.wiki, page + ".md")
        if args.check:
            old = open(dest, encoding="utf-8").read() if os.path.exists(dest) else ""
            print(f"  {'CHANGED' if old != out else 'same   '}  {page}.md")
        else:
            with open(dest, "w", encoding="utf-8") as f:
                f.write(out)
            written += 1

    if unresolved:
        print("\nUNRESOLVED links (left as-is — fix the mapping):", file=sys.stderr)
        for src, tgt in unresolved:
            print(f"  {src}: {tgt}", file=sys.stderr)

    if not args.check:
        print(f"\nWrote {written} wiki pages to {args.wiki}")
        print("NOTE: _Sidebar.md and Home.md are hand-maintained — not regenerated.")
    return 1 if unresolved else 0


if __name__ == "__main__":
    sys.exit(main())
