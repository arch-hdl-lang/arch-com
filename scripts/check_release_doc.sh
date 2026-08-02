#!/usr/bin/env bash
# check_release_doc.sh — a version bump must update COMPILER_STATUS.md in lock-step.
#
# Usage:
#   scripts/check_release_doc.sh <base-ref>   # e.g. origin/main
#
# Rationale: the release PR is where the compiler version bumps, and the
# convention is that it ALSO updates doc/COMPILER_STATUS.md — the version
# banner and a per-release highlights block. In practice release PRs keep
# landing as Cargo-only (0.71.0 shipped with the banner stuck at 0.70.8 and no
# highlights block; a follow-up doc PR had to reconcile it — the exact drift
# the daily review keeps catching). This check makes the coupling mechanical
# instead of relying on reviewer vigilance.
#
# The rule, narrow and precise (precision over recall — a noisy required check
# gets bypassed and then ignored):
#
#   IF this PR changes the package `version` in Cargo.toml
#   THEN doc/COMPILER_STATUS.md on this branch MUST have:
#        (a) a banner line `> Compiler version: <new-version>`, and
#        (b) a highlights heading `> **<new-version> release highlights:**`
#   ELSE fail with an actionable message.
#
# It does NOT judge the prose of the highlights — only that the version banner
# matches Cargo.toml and a block for that version exists. The skill-snapshot
# copy of COMPILER_STATUS.md is kept in sync by a separate check
# (scripts/sync_skill_snapshots.sh), so we deliberately don't duplicate that
# here.
#
# Escape hatch: PRs whose GitHub Actions job sees the `release-doc-exempt`
# label skip this check entirely (see .github/workflows/release-doc.yml — the
# label check happens in the workflow, not here, since a label isn't visible to
# a local `git diff`). Apply it for a rare bare re-publish bump that
# intentionally ships no user-facing change.
#
# Locally runnable so contributors can pre-check before pushing:
#   scripts/check_release_doc.sh origin/main

set -euo pipefail

STATUS_DOC="doc/COMPILER_STATUS.md"

usage() {
  cat <<'USAGE'
Usage: scripts/check_release_doc.sh <base-ref>

If this branch bumps the package `version` in Cargo.toml relative to <base-ref>,
requires doc/COMPILER_STATUS.md to carry a matching version banner and a
per-release highlights block. Otherwise passes.

Example:
  scripts/check_release_doc.sh origin/main
USAGE
}

if [[ $# -ne 1 || "$1" == "-h" || "$1" == "--help" ]]; then
  usage >&2
  exit 2
fi

base_ref="$1"

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

# Extract the package version (first line-anchored `version = "..."`, which is
# the [package] version — dependency versions are never line-anchored at column
# 0 in this Cargo.toml). Reads from a git blob so it works pre-commit too.
pkg_version() { # $1 = git ref, or "" for the working tree
  local ref="$1" content
  if [[ -n "$ref" ]]; then
    content="$(git show "${ref}:Cargo.toml" 2>/dev/null || true)"
  else
    content="$(cat Cargo.toml 2>/dev/null || true)"
  fi
  printf '%s\n' "$content" \
    | grep -m1 -E '^version[[:space:]]*=[[:space:]]*"' \
    | sed -E 's/^version[[:space:]]*=[[:space:]]*"([^"]+)".*/\1/'
}

base_version="$(pkg_version "$base_ref")"
head_version="$(pkg_version "")" # working tree = HEAD in CI checkout

if [[ -z "$head_version" ]]; then
  echo "check_release_doc: could not read package version from Cargo.toml; skipping." >&2
  exit 0
fi

if [[ "$base_version" == "$head_version" ]]; then
  echo "check_release_doc: package version unchanged (${head_version:-?}); not a release PR; pass."
  exit 0
fi

echo "check_release_doc: version bump detected: ${base_version:-<none>} -> ${head_version}"

if [[ ! -f "$STATUS_DOC" ]]; then
  cat >&2 <<EOF
check_release_doc: FAIL

This PR bumps the package version to ${head_version}, but ${STATUS_DOC} does
not exist. The release PR must update the status doc's banner and highlights.
EOF
  exit 1
fi

banner_line="> Compiler version: ${head_version}"
highlights_line="> **${head_version} release highlights:**"

missing=""
grep -qxF "$banner_line" "$STATUS_DOC" || missing="${missing}  - banner line:      ${banner_line}"$'\n'
grep -qxF "$highlights_line" "$STATUS_DOC" || missing="${missing}  - highlights block: ${highlights_line}"$'\n'

if [[ -z "$missing" ]]; then
  echo "check_release_doc: ${STATUS_DOC} carries the ${head_version} banner and highlights block; pass."
  exit 0
fi

cat >&2 <<EOF
check_release_doc: FAIL

This PR bumps the package version to ${head_version}, but ${STATUS_DOC} is
missing the matching release update. Expected (verbatim) line(s):

${missing}
The release PR must update ${STATUS_DOC} in lock-step with the Cargo bump:

  1. Set the banner to:  ${banner_line}
     (and refresh the "> Last updated:" date).
  2. Add a highlights block headed:  ${highlights_line}
     summarizing the user-facing changes merged since the previous release.
  3. Re-sync the skill snapshot:  scripts/sync_skill_snapshots.sh refresh

If this is a rare bare re-publish bump that ships no user-facing change, add
the 'release-doc-exempt' label to this PR to skip this check.

See CLAUDE.md's "Release" notes and the daily-review "document consistency"
duty for context.
EOF
exit 1
