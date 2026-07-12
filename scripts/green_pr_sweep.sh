#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
# green_pr_sweep.sh — the stranded-PR sweep.
#
# Lists every open PR with its CI verdict, mergeability, and age, and calls out
# PRs that are GREEN + CLEAN + older than a day: those are finished work that
# nobody merged. The mirror image of claim_check.sh — that one catches
# duplicate work before it starts; this one catches completed work that never
# landed (the PR #643 failure mode: a fix PR sat unmerged and its bug shipped
# live for a week).
#
# Usage:
#   scripts/green_pr_sweep.sh                  # arch-com + harc-com
#   scripts/green_pr_sweep.sh owner/repo ...   # explicit repo list
#
# Advisory, like claim_check.sh — it informs, it does not merge.
# ─────────────────────────────────────────────────────────────────────────────
set -euo pipefail

if ! command -v gh >/dev/null 2>&1; then
  echo "green-pr-sweep: gh CLI not found" >&2
  exit 1
fi

repos=("$@")
[ ${#repos[@]} -gt 0 ] || repos=(arch-hdl-lang/arch-com arch-hdl-lang/harc-com)

stranded=0

for repo in "${repos[@]}"; do
  echo "== ${repo} =="
  gh pr list --repo "$repo" --state open \
    --json number,title,createdAt,isDraft,labels,mergeStateStatus,statusCheckRollup \
    --jq '
      .[] |
      ([ .statusCheckRollup[]?
         | (.conclusion // .state // "PENDING")
         | ascii_upcase ]) as $cs |
      ([ .labels[]?.name ] | any(. == "awaiting-decision")) as $parked |
      (if ($cs | any(. == "FAILURE" or . == "ERROR" or . == "TIMED_OUT" or . == "CANCELLED")) then "RED"
       elif ($cs | any(. == "PENDING" or . == "QUEUED" or . == "IN_PROGRESS" or . == "")) then "PENDING"
       elif ($cs | length) == 0 then "NO-CI"
       else "GREEN" end) as $ci |
      (((now - (.createdAt | fromdate)) / 86400) | floor) as $age |
      (if .isDraft then "DRAFT"
       elif $parked then "PARKED"
       elif $ci == "GREEN" and .mergeStateStatus != "DIRTY" and $age >= 1 then "STRANDED?"
       else "" end) as $flag |
      [ "#\(.number)",
        $ci,
        .mergeStateStatus,
        "\($age)d",
        $flag,
        (.title | .[0:70]) ]
      | @tsv
    ' | column -t -s $'\t' || echo "  (query failed for ${repo})"

  n="$(gh pr list --repo "$repo" --state open \
        --json number,createdAt,isDraft,labels,mergeStateStatus,statusCheckRollup \
        --jq '
          [ .[] | select(.isDraft | not)
            | select([ .labels[]?.name ] | any(. == "awaiting-decision") | not)
            | select(.mergeStateStatus != "DIRTY")
            | select(((now - (.createdAt | fromdate)) / 86400) >= 1)
            | select(
                ([ .statusCheckRollup[]? | (.conclusion // .state // "PENDING") | ascii_upcase ])
                | (length > 0 and all(. == "SUCCESS" or . == "SKIPPED" or . == "NEUTRAL"))
              )
          ] | length
        ' 2>/dev/null || echo 0)"
  stranded=$((stranded + n))
  echo
done

if [ "$stranded" -gt 0 ]; then
  echo "green-pr-sweep: ⚠ ${stranded} PR(s) look STRANDED (green, mergeable, ≥1 day old)."
  echo "                Merge them, or say in the PR why they wait — finished work"
  echo "                sitting unmerged is how fixed bugs ship live (see PR #643)."
else
  echo "green-pr-sweep: ✓ no stranded PRs."
fi
