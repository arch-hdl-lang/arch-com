#!/usr/bin/env bash
# Regenerates doc/generated/pipelined_ops.md from the compiler-owned
# pipelined-operator registry (src/pipelined_ops.rs::BUILTIN_REGISTRY).
#
# The registry doc is deliberately generated (not hand-maintained) so
# "what's builtin" can never drift from what the compiler accepts, per
# doc/proposal_pipelined_operators.md §1 point 3. Its content is also
# covered by a `cargo test` drift check (tests/pipelined_ops_cli_test.rs),
# so this script is a convenience — CI does not depend on running it.
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

cargo build --release --quiet
./target/release/arch ops --markdown > doc/generated/pipelined_ops.md
echo "wrote doc/generated/pipelined_ops.md"
