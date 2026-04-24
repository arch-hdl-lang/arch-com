#!/usr/bin/env bash
# Tier-2 SVA cross-check on the 4×4 mesh. Builds the design via
# `arch build`, runs Verilator with --assert across the full SV (80
# credit_channel SVA properties total), and runs a stress traffic
# pattern. A passing run = no _auto_cc_*_credit_bounds,
# _send_requires_credit, or _credit_return_requires_buffered fired.

set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
BUILD="${TMPDIR:-/tmp}/mesh_sva_build"

ARCH="${ARCH_BIN:-$REPO/target/debug/arch}"
[ -x "$ARCH" ] || ARCH="$REPO/target/release/arch"
[ -x "$ARCH" ] || { echo "no arch binary built — run 'cargo build' first" >&2; exit 1; }

SV="$BUILD/mesh4x4.sv"
mkdir -p "$BUILD"
"$ARCH" build "$HERE/mesh4x4.arch" -o "$SV"

cd "$BUILD"
verilator --binary --assert --top sva_tb -Wno-fatal \
    "$SV" "$HERE/mesh4x4_sva_tb.sv"
"$BUILD/obj_dir/Vsva_tb"
