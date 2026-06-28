#!/usr/bin/env bash
# Install the versioned local git hooks for this clone. Run once per clone.
set -euo pipefail
repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

git config core.hooksPath .githooks
chmod +x .githooks/* scripts/pre_pr_review.sh 2>/dev/null || true

echo "Installed git hooks (core.hooksPath=.githooks):"
echo "  pre-commit  — keep work out of the shared PRIMARY checkout; commit from a"
echo "                linked worktree instead (bypass: WORKTREE_ENFORCE_SKIP=1)"
echo "  pre-push    — PR-review-marker + duplicate-work (claim-check) gates"
