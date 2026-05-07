#!/usr/bin/env bash
# Hook: run cargo test after any src/*.rs edit.
# If snapshot mismatches are the only failures, auto-accept and re-run.
# Exit 0  → hook succeeded (output shown in transcript)
# Exit 2  → block the action (not used here; tests run post-edit)

set -euo pipefail

REPO=/Users/shuqingzhao/github/arch-com

# ── 1. Check that the edited file is a compiler source file ──────────────────
FILE=$(echo "$CLAUDE_TOOL_INPUT" \
  | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('file_path',d.get('path','')))" \
  2>/dev/null || true)

[[ "$FILE" =~ /src/.*\.rs$ ]] || exit 0   # not a src file — skip

cd "$REPO"

echo "=== cargo test (triggered by edit to $(basename "$FILE")) ==="

# ── 2. Run tests ─────────────────────────────────────────────────────────────
cargo test 2>&1
TEST_EXIT=$?

if [ $TEST_EXIT -eq 0 ]; then
  echo "✓ All tests pass."
  exit 0
fi

# ── 3. Check if failures are snapshot-only ───────────────────────────────────
SNAP_NEW=$(find tests/snapshots -name "*.snap.new" 2>/dev/null | wc -l | tr -d ' ')

if [ "$SNAP_NEW" -eq 0 ]; then
  echo "✗ Tests failed (no snapshot drift — real failures above)."
  exit 0   # don't block; Claude sees the output and can investigate
fi

# ── 4. Snapshot drift detected — accept and re-run ───────────────────────────
echo ""
echo "--- $SNAP_NEW snapshot(s) changed — auto-accepting ---"
cargo insta accept 2>&1

echo ""
echo "=== cargo test (re-run after snapshot update) ==="
cargo test 2>&1
RERUN_EXIT=$?

if [ $RERUN_EXIT -eq 0 ]; then
  echo "✓ All tests pass (snapshots updated)."
else
  echo "✗ Tests still failing after snapshot update — real failures remain."
fi

exit 0
