#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DIST="${DIST:-$ROOT/dist}"
OUT_LINK="${OUT_LINK:-$ROOT/result-appimage}"

mkdir -p "$DIST"
nix build "$ROOT#AppImage" --out-link "$OUT_LINK" "$@"
cp -f "$OUT_LINK"/hearthstone-linux-gui-*-x86_64.AppImage "$DIST"/
