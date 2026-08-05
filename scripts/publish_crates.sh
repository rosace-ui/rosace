#!/usr/bin/env bash
# Publish the ROSACE framework to crates.io in dependency order.
#
# Order matches .steering/DEV_PREVIEW_PUBLISH.md (computed from the actual
# path-dependency graph — DO NOT hand-edit this list without recomputing).
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
  rosace-view-syntax
  rosace-macros
  rosace-trace
  rosace-state
  rosace-core
  rosace-theme
  rosace-style
  rosace-layout
  rosace-render
  rosace-scroll
  rosace-animate
  rosace-forms
  rosace-nav
  rosace-text
  rosace-a11y
  rosace-shader
  rosace-widgets
  rosace-hot-reload
  rosace-cli
  rosace-asset-codegen
  rosace-web-seo
  rosace-compositor
  rosace-ime
  rosace-platform
  rosace-nav-anim
  rosace-devtools
  rosace-gesture
  rosace-net
  rosace-file
  rosace-storage
  rosace-shaping
  rosace-bidi
  rosace-i18n
  rosace-clipboard
  rosace-ws
  rosace-media
  rosace-test-utils
  rosace
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
    if ! cargo publish --dry-run -p "$crate"; then
      echo
      echo "DRY RUN FAILED at $crate. Fix the issue, then resume with:"
      echo "  scripts/publish_crates.sh --from $crate"
      exit 1
    fi
  fi
  echo
done

echo "Done. $(if $LIVE; then echo "All crates published."; else echo "Dry run clean — re-run with --live to publish for real."; fi)"
