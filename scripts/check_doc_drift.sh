#!/usr/bin/env bash
# check_doc_drift.sh — grammar-surface PRs must also touch doc/.
#
# Usage:
#   scripts/check_doc_drift.sh <base-ref>   # e.g. origin/main
#
# Rationale: doc drift (spec updates lagging behind language/compiler changes)
# is the most recurring finding in this repo's daily reviews — features land
# without spec updates and doc/ rots. This script enforces a narrow, precise
# rule: IF a PR touches a "grammar-surface" file AND touches no file under
# doc/, THEN fail. It does not try to catch every doc-relevant change —
# precision over recall, because a noisy required check gets bypassed
# (`no-doc-needed`) and eventually ignored entirely.
#
# SURFACE_FILES is deliberately narrow: src/lexer.rs, src/parser.rs,
# src/ast.rs. These three are the actual grammar surface — new syntax, new
# AST node shapes, new token kinds. Deliberately EXCLUDED: typecheck.rs,
# elaborate.rs, and the codegen backends. Those files change constantly for
# internal reasons (bug fixes, refactors, new lowering strategies) that carry
# no user-facing syntax/semantics delta, and are far higher-churn than the
# grammar files — including them would flag nearly every PR and make the
# check noise instead of signal. Widening the list (e.g. adding
# src/typecheck.rs once it stabilizes, or the codegen dirs when doc coverage
# there catches up) is a one-line edit to SURFACE_FILES below.
#
# Escape hatch: PRs whose GitHub Actions job sees the `no-doc-needed` label
# skip this check entirely (see .github/workflows/doc-drift.yml — the label
# check happens in the workflow, not here, since a label isn't visible to a
# local `git diff`). Apply it for pure internal refactors that touch a
# surface file without changing user-facing syntax/semantics — e.g. the
# planned parser/elaborate splits.
#
# Locally runnable so contributors can pre-check before pushing:
#   scripts/check_doc_drift.sh origin/main

set -euo pipefail

# Grammar-surface files: precision over recall (see header rationale above).
SURFACE_FILES=(
  "src/lexer.rs"
  "src/parser.rs"
  "src/ast.rs"
)

usage() {
  cat <<'USAGE'
Usage: scripts/check_doc_drift.sh <base-ref>

Computes the diff between <base-ref> and HEAD. If any grammar-surface file
(src/lexer.rs, src/parser.rs, src/ast.rs) changed and no file under doc/
changed, fails with an actionable message.

Example:
  scripts/check_doc_drift.sh origin/main
USAGE
}

if [[ $# -ne 1 || "$1" == "-h" || "$1" == "--help" ]]; then
  usage >&2
  exit 2
fi

base_ref="$1"

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

# Use a three-dot diff (merge-base...HEAD) so the check reflects only this
# branch's changes, not unrelated commits that landed on base_ref meanwhile.
changed_files="$(git diff --name-only "${base_ref}...HEAD" 2>/dev/null || true)"

if [[ -z "$changed_files" ]]; then
  echo "check_doc_drift: no changes vs ${base_ref}; nothing to check."
  exit 0
fi

surface_hit=""
for f in "${SURFACE_FILES[@]}"; do
  if grep -qxF "$f" <<<"$changed_files"; then
    surface_hit="${surface_hit}${surface_hit:+, }${f}"
  fi
done

if [[ -z "$surface_hit" ]]; then
  echo "check_doc_drift: no grammar-surface file changed; pass."
  exit 0
fi

if grep -q '^doc/' <<<"$changed_files"; then
  echo "check_doc_drift: grammar-surface file(s) changed (${surface_hit}) and doc/ was also updated; pass."
  exit 0
fi

cat >&2 <<EOF
check_doc_drift: FAIL

This PR changes grammar-surface file(s): ${surface_hit}
but touches no file under doc/.

Changes to src/lexer.rs, src/parser.rs, or src/ast.rs usually mean new or
changed syntax, which the spec needs to reflect. Two ways to resolve this:

  1. If this change affects user-facing syntax or semantics, update the
     relevant spec doc in the same PR (doc/ARCH_HDL_Specification.docx,
     doc/Arch_AI_Reference_Card.docx, doc/thread_spec_section.md, spec.md,
     or whichever doc/ file documents the affected construct).

  2. If this is a pure internal refactor of a surface file with NO
     user-facing syntax/semantics change (e.g. an AST node internal
     reshuffle, a parser implementation detail), add the 'no-doc-needed'
     label to this PR to skip this check.

See CLAUDE.md's "Fix-PR lifecycle" section for details.
EOF
exit 1
