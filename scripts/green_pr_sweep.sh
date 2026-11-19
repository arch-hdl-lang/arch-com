#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
# green_pr_sweep.sh — the stranded-PR sweep.
#
# Lists every open PR with its CI verdict, mergeability, and age, and calls out
# PRs that are finished-but-unmerged: all required checks green, ≥1 day old. The
# mirror image of claim_check.sh — that one catches duplicate work before it
# starts; this one catches completed work that never landed (the PR #643 failure
# mode: a fix PR sat unmerged and its bug shipped live for a week).
#
# STRANDED? is NOT "mergeStateStatus == CLEAN" and NOT "no failing check":
#   * This repo uses strict ("require branches up to date") protection, so every
#     finished PR drops to BEHIND the moment anything else merges (#1139/#1140,
#     #1163/#1164 all landed from BEHIND after an update-branch). Gating on CLEAN
#     would stop flagging exactly the PRs the sweep exists to surface.
#   * The CI verdict ($ci) is computed only from checks PRESENT in the rollup, so
#     a fork PR whose workflow runs are action_required (awaiting maintainer
#     approval) shows only its `cla` check and reads GREEN there — but its 6
#     other required checks never ran. Those cannot merge until a maintainer
#     approves the runs; "merge them" is not their remedy.
#
# The discriminator that separates the two is REQUIRED-CHECK PRESENCE, read from
# branch protection: a finished-but-BEHIND PR has every required context present
# and successful in the rollup; an approval-gated fork PR has them absent. So
# STRANDED? = (every required context present + SUCCESS/SKIPPED/NEUTRAL) AND
# mergeStateStatus != "DIRTY" AND age >= 1. BLOCKED/BEHIND/UNKNOWN with full
# green required checks still flag (their remedy is update-branch + merge);
# approval-gated forks and DIRTY PRs do not. Such a green-but-not-all-required
# PR is shown as GATED in the CI column so a human can see why it is held.
#
# Fallback: if branch protection is unreadable (permissions) or `jq` is missing,
# the sweep warns and reverts to the old present-checks heuristic ($ci == GREEN),
# which can over-flag fork PRs — better to over-surface than to hide finished
# work.
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

have_jq="false"
command -v jq >/dev/null 2>&1 && have_jq="true"

repos=("$@")
[ ${#repos[@]} -gt 0 ] || repos=(arch-hdl-lang/arch-com arch-hdl-lang/harc-com)

# Shared jq program. Emits, per PR, the display row (@tsv) when $mode=="rows" or
# "1" per stranded PR when $mode=="count". Uses required-context presence when
# $have_req, else the present-checks heuristic.
read -r -d '' JQ_PROG <<'JQ' || true
.[]
| ([ .statusCheckRollup[]? | (.conclusion // .state // "PENDING") | ascii_upcase ]) as $cs
| ([ .statusCheckRollup[]?
     | { (.name // .context // "?"): ((.conclusion // .state // "PENDING") | ascii_upcase) } ]
   | add // {}) as $byname
| ([ .labels[]?.name ] | any(. == "awaiting-decision")) as $parked
| (if   ($cs | any(. == "FAILURE" or . == "ERROR" or . == "TIMED_OUT" or . == "CANCELLED")) then "RED"
   elif ($cs | any(. == "PENDING" or . == "QUEUED" or . == "IN_PROGRESS" or . == "")) then "PENDING"
   elif ($cs | length) == 0 then "NO-CI"
   else "GREEN" end) as $ci
| (if $have_req
   then ($req | length) > 0
        and ($req | all(. as $r | ($byname[$r] // "MISSING")
                         | (. == "SUCCESS" or . == "SKIPPED" or . == "NEUTRAL")))
   else ($cs | (length > 0 and all(. == "SUCCESS" or . == "SKIPPED" or . == "NEUTRAL")))
   end) as $ok
| (((now - (.createdAt | fromdate)) / 86400) | floor) as $age
| (if ($ci == "GREEN" and ($ok | not)) then "GATED" else $ci end) as $ci_disp
| (if   .isDraft then "DRAFT"
   elif $parked then "PARKED"
   elif ($ci == "GREEN" and $ok and .mergeStateStatus != "DIRTY" and $age >= 1) then "STRANDED?"
   else "" end) as $flag
| if $mode == "count"
  then (if $flag == "STRANDED?" then "1" else empty end)
  else [ "#\(.number)", $ci_disp, .mergeStateStatus, "\($age)d", $flag, (.title | .[0:70]) ] | @tsv
  end
JQ

stranded=0

for repo in "${repos[@]}"; do
  echo "== ${repo} =="

  req_json="[]"; have_req="false"
  if [ "$have_jq" = "true" ]; then
    default_branch="$(gh repo view "$repo" --json defaultBranchRef --jq '.defaultBranchRef.name' 2>/dev/null || echo main)"
    # Fetch raw, then extract+validate with standalone jq. gh api prints the
    # error body (e.g. {"message":"Not Found"}) to stdout on 403/404; extracting
    # with `.contexts[]?` yields [] for such a body, and the jq -e length gate
    # below then forces the documented fallback instead of trusting it.
    raw_bp="$(gh api "repos/${repo}/branches/${default_branch}/protection/required_status_checks" 2>/dev/null || true)"
    req_json="$(printf '%s' "$raw_bp" | jq -c '[ .contexts[]?, (.checks[]?.context) ] | unique' 2>/dev/null || true)"
    if printf '%s' "$req_json" | jq -e 'type == "array" and length > 0' >/dev/null 2>&1; then
      have_req="true"
    else
      req_json="[]"; have_req="false"
      echo "  (branch protection unreadable — falling back to present-checks heuristic; fork PRs may over-flag)" >&2
    fi
  else
    echo "  (jq not found — falling back to present-checks heuristic; fork PRs may over-flag)" >&2
  fi

  prs="$(gh pr list --repo "$repo" --state open \
          --json number,title,createdAt,isDraft,labels,mergeStateStatus,statusCheckRollup 2>/dev/null || echo '[]')"

  if [ "$have_jq" = "true" ]; then
    printf '%s' "$prs" | jq -r --argjson req "$req_json" --argjson have_req "$have_req" --arg mode rows \
      "$JQ_PROG" | column -t -s $'\t' || echo "  (query failed for ${repo})"
    n="$(printf '%s' "$prs" | jq -r --argjson req "$req_json" --argjson have_req "$have_req" --arg mode count \
          "$JQ_PROG" 2>/dev/null | grep -c . || true)"
  else
    # No jq: minimal gh --jq heuristic (present-checks), display + count.
    printf '%s' "$prs" | gh pr list --repo "$repo" --state open \
      --json number,title,createdAt,isDraft,labels,mergeStateStatus,statusCheckRollup \
      --jq '
        .[] |
        ([ .statusCheckRollup[]? | (.conclusion // .state // "PENDING") | ascii_upcase ]) as $cs |
        ([ .labels[]?.name ] | any(. == "awaiting-decision")) as $parked |
        (if ($cs | any(. == "FAILURE" or . == "ERROR" or . == "TIMED_OUT" or . == "CANCELLED")) then "RED"
         elif ($cs | any(. == "PENDING" or . == "QUEUED" or . == "IN_PROGRESS" or . == "")) then "PENDING"
         elif ($cs | length) == 0 then "NO-CI" else "GREEN" end) as $ci |
        (((now - (.createdAt | fromdate)) / 86400) | floor) as $age |
        (if .isDraft then "DRAFT" elif $parked then "PARKED"
         elif $ci == "GREEN" and .mergeStateStatus != "DIRTY" and $age >= 1 then "STRANDED?"
         else "" end) as $flag |
        [ "#\(.number)", $ci, .mergeStateStatus, "\($age)d", $flag, (.title | .[0:70]) ] | @tsv
      ' | column -t -s $'\t' || echo "  (query failed for ${repo})"
    n="$(gh pr list --repo "$repo" --state open \
          --json number,createdAt,isDraft,labels,mergeStateStatus,statusCheckRollup \
          --jq '[ .[] | select(.isDraft | not)
                  | select([ .labels[]?.name ] | any(. == "awaiting-decision") | not)
                  | select(.mergeStateStatus != "DIRTY")
                  | select(((now - (.createdAt | fromdate)) / 86400) >= 1)
                  | select([ .statusCheckRollup[]? | (.conclusion // .state // "PENDING") | ascii_upcase ]
                           | (length > 0 and all(. == "SUCCESS" or . == "SKIPPED" or . == "NEUTRAL")))
                ] | length' 2>/dev/null || echo 0)"
  fi

  [ -n "${n:-}" ] || n=0
  stranded=$((stranded + n))
  echo
done

if [ "$stranded" -gt 0 ]; then
  echo "green-pr-sweep: ⚠ ${stranded} PR(s) look STRANDED (all required checks green, ≥1 day old)."
  echo "                Merge them (update-branch first if BEHIND), or say in the PR"
  echo "                why they wait — finished work sitting unmerged is how fixed"
  echo "                bugs ship live (see PR #643)."
else
  echo "green-pr-sweep: ✓ no stranded PRs."
fi
