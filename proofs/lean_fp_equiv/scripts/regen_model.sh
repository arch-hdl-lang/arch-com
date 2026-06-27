#!/usr/bin/env bash
# Regenerate ArchFpEquiv/Model.lean from the shared FP IR.
#
# The Lean model is the third renderer of the one in-Rust operator description
# (src/fp_ops.rs -> src/fp_ir.rs::render_lean), alongside the SystemVerilog
# (arch build) and SMT-LIB2 (arch formal) renderers. Keeping it generated means
# it can never drift from the synthesized RTL.
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
out="$repo_root/proofs/lean_fp_equiv/ArchFpEquiv/Model.lean"

{
  echo "-- GENERATED — do not edit by hand."
  echo "-- Regenerate with: proofs/lean_fp_equiv/scripts/regen_model.sh"
  echo "--"
  echo "-- These BitVec defs are the SAME source as the emitted SystemVerilog"
  echo "-- (arch build) and SMT-LIB2 (arch formal): all three are rendered from"
  echo "-- src/fp_ops.rs via src/fp_ir.rs. A Lean proof here therefore transfers"
  echo "-- to the synthesized RTL with no re-transcription."
  echo ""
  ( cd "$repo_root" && cargo run --release --quiet --example dump_fp -- lean )
} > "$out"

echo "wrote $out ($(wc -l < "$out") lines)"
