#!/usr/bin/env bash
# claim_check.sh — before you start (or push) a piece of work, check whether
# another open PR or active branch is already touching the same area, so two
# concurrent agents/sessions don't implement the same thing twice.
#
# Usage:
#   scripts/claim_check.sh --grep "<keyword>"     # pre-work: scan PR titles + branch names
#   scripts/claim_check.sh <path> [<path>...]     # scan by file/dir overlap
#   scripts/claim_check.sh                         # infer files from origin/main...HEAD
#
# Exit codes:
#   0  no overlap found — clear to proceed
#   3  overlap found — advisory; reconcile before continuing (callers decide whether to block)
#   2  usage error
#
# The check is best-effort: if `gh` is missing/unauthed the open-PR scan is
# skipped (branch scan still runs). It is advisory everywhere — it never
# rewrites or blocks on its own; the pre-push hook calls it non-fatally.
set -uo pipefail

repo_root="$(git rev-parse --show-toplevel 2>/dev/null)" || {
  echo "claim-check: not inside a git repo" >&2
  exit 2
}
cd "$repo_root"

have_gh=0
if command -v gh >/dev/null 2>&1 && gh auth status >/dev/null 2>&1; then
  have_gh=1
fi

overlap=0

# ---------------------------------------------------------------- grep mode --
if [[ "${1:-}" == "--grep" ]]; then
  term="${2:-}"
  if [[ -z "$term" ]]; then
    echo "usage: scripts/claim_check.sh --grep <keyword>" >&2
    exit 2
  fi
  echo "claim-check: scanning open PRs and remote branches for \"$term\""

  if [[ "$have_gh" == 1 ]]; then
    while IFS=$'\t' read -r num br title; do
      [[ -z "${num:-}" ]] && continue
      if printf '%s %s\n' "$br" "$title" | grep -iqF -- "$term"; then
        echo "  ⚠ OPEN PR #$num [$br] — $title"
        overlap=1
      fi
    done < <(gh pr list --state open --limit 100 \
               --json number,title,headRefName \
               --jq '.[] | [.number, .headRefName, .title] | @tsv' 2>/dev/null)
  else
    echo "  (gh unavailable/unauthed — skipping open-PR scan)"
  fi

  git fetch -q origin --prune 2>/dev/null || true
  # Scan both remote branches (cross-machine) and local branches / other
  # worktrees (same-machine in-progress work that isn't pushed yet).
  while IFS= read -r br; do
    short="${br#origin/}"
    [[ "$short" == "main" || "$short" == "HEAD" ]] && continue
    if printf '%s\n' "$short" | grep -iqF -- "$term"; then
      echo "  ⚠ BRANCH origin/$short"
      overlap=1
    fi
  done < <(git for-each-ref refs/remotes/origin --format='%(refname:short)' 2>/dev/null)
  cur="$(git branch --show-current 2>/dev/null || true)"
  while IFS= read -r short; do
    [[ "$short" == "main" || "$short" == "$cur" ]] && continue
    # skip locals that also exist on origin (already reported above)
    git show-ref --verify --quiet "refs/remotes/origin/$short" && continue
    if printf '%s\n' "$short" | grep -iqF -- "$term"; then
      echo "  ⚠ LOCAL BRANCH $short (not pushed)"
      overlap=1
    fi
  done < <(git for-each-ref refs/heads --format='%(refname:short)' 2>/dev/null)

# -------------------------------------------------------- file-overlap mode --
else
  if [[ "$#" -gt 0 ]]; then
    targets=("$@")
  else
    git fetch -q origin main 2>/dev/null || true
    mapfile -t targets < <(git diff --name-only origin/main...HEAD 2>/dev/null)
  fi
  if [[ "${#targets[@]}" -eq 0 ]]; then
    echo "claim-check: no target files (clean vs origin/main) — nothing to check."
    exit 0
  fi
  echo "claim-check: scanning for parallel work on:"
  printf '  - %s\n' "${targets[@]}"

  is_target() {
    local f="$1" t
    for t in "${targets[@]}"; do
      # exact path match, or target is a dir prefix of the changed file
      [[ "$f" == "$t" || "$f" == "${t%/}/"* ]] && return 0
    done
    return 1
  }

  if [[ "$have_gh" == 1 ]]; then
    while IFS=$'\t' read -r num br title path; do
      [[ -z "${num:-}" ]] && continue
      if is_target "$path"; then
        echo "  ⚠ OPEN PR #$num [$br] touches $path — $title"
        overlap=1
      fi
    done < <(gh pr list --state open --limit 100 \
               --json number,title,headRefName,files \
               --jq '.[] | .number as $n | .headRefName as $b | .title as $t
                     | .files[].path | [$n, $b, $t, .] | @tsv' 2>/dev/null)
  else
    echo "  (gh unavailable/unauthed — skipping open-PR scan)"
  fi

  # Active branches with unique work vs main — remote (cross-machine) plus
  # local branches / other worktrees not yet pushed (same-machine in-progress).
  # Each candidate is "<display>\t<ref-to-diff>"; bounded to 40 most recent.
  git fetch -q origin --prune 2>/dev/null || true
  cur="$(git branch --show-current 2>/dev/null || true)"
  candidates() {
    # remote: display + ref are both "origin/<name>"
    git for-each-ref --sort=-committerdate refs/remotes/origin \
      --format='%(refname:short)|%(refname:short)' 2>/dev/null
    # local branches not also on origin: display "<name> (local...)", ref "<name>"
    while IFS= read -r short; do
      git show-ref --verify --quiet "refs/remotes/origin/$short" && continue
      printf '%s (local, not pushed)|%s\n' "$short" "$short"
    done < <(git for-each-ref refs/heads --format='%(refname:short)' 2>/dev/null)
  }
  while IFS='|' read -r display ref; do
    [[ -z "${ref:-}" ]] && continue
    short="${ref#origin/}"
    [[ "$short" == "main" || "$short" == "HEAD" || "$short" == "$cur" ]] && continue
    ahead="$(git rev-list --count "origin/main..$ref" 2>/dev/null || echo 0)"
    [[ "$ahead" =~ ^[0-9]+$ ]] || ahead=0
    [[ "$ahead" -eq 0 ]] && continue
    changed="$(git diff --name-only "origin/main...$ref" 2>/dev/null || true)"
    hit=""
    while IFS= read -r f; do
      [[ -z "$f" ]] && continue
      is_target "$f" && hit="$hit $f"
    done < <(printf '%s\n' "$changed")
    [[ -n "$hit" ]] && {
      echo "  ⚠ BRANCH $display touches$hit"
      overlap=1
    }
  done < <(candidates | head -40)
fi

if [[ "$overlap" -eq 1 ]]; then
  echo ""
  echo "claim-check: ⚠ possible duplicate work — reconcile with the above before continuing."
  echo "             (advisory: if this is intentional, proceed.)"
  exit 3
fi
echo "claim-check: ✓ no open PR or active branch overlaps."
exit 0
