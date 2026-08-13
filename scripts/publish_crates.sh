#!/usr/bin/env bash
# Publish the ROSACE framework to crates.io in dependency order.
#
# Order matches .steering/DEV_PREVIEW_PUBLISH.md (computed from the actual
# path-dependency graph — DO NOT hand-edit this list without recomputing).
# 26 crates as of D131 (2026-08-08); was 39 before the consolidation.
#
# Usage:
#   scripts/publish_crates.sh                 # dry run (default, safe)
#   scripts/publish_crates.sh --live           # the real thing
#   scripts/publish_crates.sh --live --from rosace-widgets   # resume partway
#
# Safe to re-run: already-published versions are detected and skipped, so a
# run interrupted partway through (rate limit, network blip, ctrl-c) can
# just be re-run with the same flags.
set -euo pipefail

CRATES=(
  rosace-trace
  rosace-state
  rosace-core
  rosace-animate
  rosace-layout
  rosace-view-syntax
  rosace-macros
  rosace-render
  rosace-theme
  rosace-nav
  rosace-shader
  rosace-text
  rosace-widgets
  rosace-devtools
  rosace-hot-reload
  rosace-media
  rosace-net
  rosace-compositor
  rosace-platform
  rosace-storage
  rosace-style
  rosace-test-utils
  rosace
  rosace-asset-codegen
  rosace-cli
  rosace-ffi
)

LIVE=false
FROM=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --live) LIVE=true; shift ;;
    --from) FROM="$2"; shift 2 ;;
    *) echo "unknown flag: $1" >&2; exit 1 ;;
  esac
done

if [[ -n "$FROM" ]]; then
  found=false
  filtered=()
  for c in "${CRATES[@]}"; do
    if [[ "$c" == "$FROM" ]]; then found=true; fi
    if $found; then filtered+=("$c"); fi
  done
  if ! $found; then
    echo "error: --from '$FROM' is not in the publish list" >&2
    exit 1
  fi
  CRATES=("${filtered[@]}")
fi

# crates.io's API 403s bare/default-User-Agent requests (anti-bot policy,
# undocumented in the error body — found 2026-08-05 resuming this exact
# script: the skip-check below was silently failing closed, treating every
# already-published crate as unpublished). A descriptive UA is required.
CURL_UA="rosace-publish-script (https://github.com/rosace-ui/rosace)"

VERSION=$(grep -A5 '^\[workspace.package\]' Cargo.toml | grep '^version' | head -1 | sed -E 's/.*"(.*)".*/\1/')
echo "Publishing ${#CRATES[@]} crates at version $VERSION (live=$LIVE)"
echo

for crate in "${CRATES[@]}"; do
  echo "── $crate ──────────────────────────────────────────"

  # Resumability: skip if this exact version is already on crates.io.
  if curl -sf -H "User-Agent: $CURL_UA" "https://crates.io/api/v1/crates/$crate/$VERSION" -o /dev/null 2>/dev/null; then
    echo "  already published at $VERSION — skipping"
    echo
    continue
  fi

  if $LIVE; then
    if ! cargo publish -p "$crate"; then
      echo
      echo "FAILED at $crate. Fix the issue, then resume with:"
      echo "  scripts/publish_crates.sh --live --from $crate"
      exit 1
    fi
    # crates.io index needs a moment to propagate before the next crate's
    # path->registry dep can resolve.
    echo "  published. waiting for index propagation..."
    for i in $(seq 1 30); do
      if curl -sf -H "User-Agent: $CURL_UA" "https://crates.io/api/v1/crates/$crate/$VERSION" -o /dev/null 2>/dev/null; then
        break
      fi
      sleep 2
    done
  else
    # --no-verify is REQUIRED here, not a shortcut.
    #
    # `cargo publish --dry-run` resolves each crate's dependencies from the
    # REGISTRY, where the new version does not exist yet. So the moment a
    # release adds an API in an upstream crate (say rosace-core), the dry run
    # of every downstream crate builds against the PREVIOUS published version
    # and fails with "method not found" — an error about the old release, not
    # about anything wrong with this one. It fails 100% of the time on any
    # release that changes a cross-crate API, which makes it useless as a gate.
    #
    # With --no-verify the dry run still does the part only it can do: pack
    # the tarball and check the manifest (missing `version` on a path dep,
    # files outside the crate dir, an unset `license`/`description`). The
    # actual compile is already covered by `cargo test --workspace` on the
    # real source tree, which is a STRONGER check than building the tarball
    # against stale registry deps.
    if ! cargo publish --dry-run --no-verify -p "$crate"; then
      echo
      echo "DRY RUN FAILED at $crate. Fix the issue, then resume with:"
      echo "  scripts/publish_crates.sh --from $crate"
      exit 1
    fi
  fi
  echo
done

echo "Done. $(if $LIVE; then echo "All crates published."; else echo "Dry run clean — re-run with --live to publish for real."; fi)"
