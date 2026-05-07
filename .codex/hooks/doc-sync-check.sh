#!/usr/bin/env bash
# Stop hook: remind to sync user-facing docs when src/*.rs changes are
# pending in the working tree but no doc files have been updated.
# Runs once per assistant turn. Non-blocking — output lands in the
# transcript so Claude can decide whether to act.
#
# Exit 0: always (advisory only, never blocks the Stop).
# Detection uses `git status` on the working tree (staged + unstaged + untracked).
# If the user is already editing docs in the same turn, the hook stays silent.

set -euo pipefail

REPO=/Users/shuqingzhao/github/arch-com
cd "$REPO" 2>/dev/null || exit 0

# Check both (a) uncommitted working-tree changes and (b) changes
# since the branch's merge-base with main. (b) is what catches the
# common "edit → commit → push → Stop" workflow where the tree is
# clean by the time the hook evaluates.
UNCOMMITTED=$(git diff --name-only HEAD 2>/dev/null) || true
BASE=$(git merge-base HEAD origin/main 2>/dev/null || git merge-base HEAD main 2>/dev/null || true)
BRANCH=""
if [ -n "$BASE" ]; then
  BRANCH=$(git diff --name-only "$BASE" HEAD 2>/dev/null) || true
fi
CHANGED=$(printf '%s\n%s\n' "$UNCOMMITTED" "$BRANCH" | sort -u | { grep -v '^$' || true; })

SRC=$(printf '%s\n' "$CHANGED" | { grep -E '^src/.*\.rs$' || true; } | wc -l | tr -d ' ')
[ "$SRC" -eq 0 ] && exit 0

DOC=$(printf '%s\n' "$CHANGED" | { grep -E '^(doc/.*\.md|CLAUDE\.md|README\.md)$' || true; } | wc -l | tr -d ' ')
[ "$DOC" -gt 0 ] && exit 0

# Detect user-visible src changes: new keyword/token/lexeme, new CLI
# subcommand, new AST variant/struct, new method-name string literal
# (covers Vec methods like "any"/"all"/"find_first" added as match arms),
# new concurrent SVA emission. If any appear, escalate to LOUD
# (exit 2 forces Claude to acknowledge).
LOUD=0
SRC_DIFF=""
if [ -n "$UNCOMMITTED" ]; then
  SRC_DIFF=$(git diff HEAD -- 'src/*.rs' 2>/dev/null || true)
fi
if [ -n "$BASE" ] && [ -z "$SRC_DIFF" ]; then
  SRC_DIFF=$(git diff "$BASE" HEAD -- 'src/*.rs' 2>/dev/null || true)
fi
if printf '%s' "$SRC_DIFF" | grep -Eq '^\+.*(#\[token\(|Subcommand\]|#\[command\(subcommand|pub enum |pub struct |"[a-z_]+" =>)'; then
  LOUD=1
fi

# Suppress LOUD escalation when the working tree is clean and every
# branch commit since merge-base is a refactor/chore/test/docs/style
# commit (conventional-commit prefix). These don't ship user-visible
# behavior, so extracted submodules moving `pub struct`/`pub enum`
# around shouldn't block the Stop every turn.
if [ "$LOUD" -eq 1 ] && [ -n "$BASE" ] && [ -z "$UNCOMMITTED" ]; then
  SUBJECTS=$(git log --format=%s "$BASE..HEAD" 2>/dev/null || true)
  if [ -n "$SUBJECTS" ] && ! printf '%s\n' "$SUBJECTS" | grep -Evq '^(refactor|chore|test|docs|style)(\(|:|!)'; then
    LOUD=0
  fi
fi

# All reminder output goes to stderr — Claude Code's Stop-hook harness
# captures stderr, not stdout.
cat <<EOF >&2
⚠ Doc sync reminder
   $SRC src/*.rs file(s) modified without any doc update in this working tree.

   If this change is user-visible (new keyword, construct, variant, CLI
   flag, emission shape), update BEFORE committing:
     - doc/ARCH_HDL_Specification.md  ← authoritative language spec (most commonly skipped)
     - doc/Arch_AI_Reference_Card.md  ← AI-facing quick reference
     - doc/COMPILER_STATUS.md         ← feature status / CLI flags
     - CLAUDE.md / README.md          ← agent / user guidance

   Pure refactor, bug fix, or test fixture? Ignore this reminder.
EOF

if [ "$LOUD" -eq 1 ]; then
  cat <<'EOF' >&2

   🔴 Detected new token / Subcommand / pub enum / pub struct / method arm in src/ diff.
   This looks like a user-visible change. Update the spec + reference card
   before the next commit, or explicitly say the change is internal-only.
EOF
  # Advisory only — exit 0 so the Stop event is not blocked.
  exit 0
fi

exit 0
