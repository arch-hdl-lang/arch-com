#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage:
  scripts/monitor_pr_ci.sh [pr-number-or-url-or-branch]

Watches GitHub PR checks for the current branch PR, or for the PR argument
provided. Exits 0 when all checks pass. Exits nonzero when any check fails,
and prints the failing check names and links so the author can inspect logs,
fix, push, and rerun the monitor.
USAGE
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

if ! command -v gh >/dev/null 2>&1; then
  echo "monitor-pr-ci: gh is required; install GitHub CLI and authenticate first" >&2
  exit 2
fi

if ! gh auth status >/dev/null 2>&1; then
  echo "monitor-pr-ci: gh is not authenticated; run gh auth login first" >&2
  exit 2
fi

pr="${1:-}"
if [[ -z "$pr" ]]; then
  pr="$(gh pr view --json number --jq .number)"
fi

echo "monitor-pr-ci: watching PR ${pr}"
deadline=$((SECONDS + ${ARCH_PR_CI_MAX_WAIT_SECONDS:-900}))
status=0
while true; do
  set +e
  output="$(gh pr checks "$pr" --watch --interval "${ARCH_PR_CI_POLL_SECONDS:-10}" 2>&1)"
  status=$?
  set -e

  if [[ "$output" != *"no checks reported"* ]]; then
    printf '%s\n' "$output"
    break
  fi

  if (( SECONDS >= deadline )); then
    printf '%s\n' "$output"
    status=8
    break
  fi

  echo "monitor-pr-ci: no checks reported yet; waiting for GitHub to enqueue CI"
  sleep "${ARCH_PR_CI_POLL_SECONDS:-10}"
done

if [[ "$status" -eq 0 ]]; then
  echo "monitor-pr-ci: all PR checks passed for ${pr}"
  exit 0
fi

echo "monitor-pr-ci: one or more PR checks failed or did not complete for ${pr}" >&2
echo >&2
gh pr checks "$pr" || true
echo >&2
echo "Failing checks:" >&2
gh pr checks "$pr" \
  --json name,bucket,state,workflow,link \
  --jq '.[] | select(.bucket == "fail") | "- \(.name) [\(.workflow // "unknown workflow")] \(.state): \(.link)"' >&2 || true
echo >&2
echo "Next step: inspect the failing check logs, fix the branch, push, then rerun scripts/monitor_pr_ci.sh ${pr}" >&2
exit "$status"
