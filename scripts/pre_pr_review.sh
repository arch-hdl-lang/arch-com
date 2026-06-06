#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage:
  scripts/pre_pr_review.sh mark [base-ref]
  scripts/pre_pr_review.sh check [base-ref]

Records or checks that the current branch HEAD has received a code-review pass
before it is pushed/opened as a PR. Review markers are local and stored under
.git/pre-pr-reviews/.
USAGE
}

mode="${1:-}"
base_ref="${2:-origin/main}"

if [[ "$mode" != "mark" && "$mode" != "check" ]]; then
  usage >&2
  exit 2
fi

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

branch="$(git branch --show-current)"
if [[ -z "$branch" ]]; then
  echo "pre-pr-review: detached HEAD is not supported" >&2
  exit 2
fi

head_sha="$(git rev-parse HEAD)"
safe_branch="$(printf '%s' "$branch" | tr '/: ' '___')"
marker_dir="$(git rev-parse --git-path pre-pr-reviews)"
marker="${marker_dir}/${safe_branch}.review"
merge_base="$(git merge-base "${base_ref}" HEAD)"

mkdir -p "$marker_dir"

changed_files="$(git diff --name-only "${base_ref}...HEAD" 2>/dev/null || true)"
if [[ -z "$changed_files" ]]; then
  changed_files="$(git diff --name-only HEAD~1...HEAD 2>/dev/null || true)"
fi

if [[ "$mode" == "mark" ]]; then
  {
    printf 'branch=%s\n' "$branch"
    printf 'head=%s\n' "$head_sha"
    printf 'base=%s\n' "$base_ref"
    printf 'merge_base=%s\n' "$merge_base"
    printf 'reviewed_at_utc=%s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
    printf 'changed_files<<EOF\n%s\nEOF\n' "$changed_files"
  } >"$marker"
  echo "pre-pr-review: recorded code review marker for ${branch} at ${head_sha}"
  exit 0
fi

if [[ ! -f "$marker" ]]; then
  cat >&2 <<EOF
pre-pr-review: missing code-review marker for ${branch}.

Before creating/pushing a PR, run a code-review pass against:
  git diff ${base_ref}...HEAD

Review stance:
  findings first, ordered by severity, with file/line references;
  then open questions, then a brief summary and validation gaps.

After the review is complete and findings are addressed or accepted, run:
  scripts/pre_pr_review.sh mark
EOF
  exit 1
fi

reviewed_head="$(awk -F= '$1 == "head" { print $2; exit }' "$marker")"
reviewed_merge_base="$(awk -F= '$1 == "merge_base" { print $2; exit }' "$marker")"
if [[ "$reviewed_head" != "$head_sha" ]]; then
  cat >&2 <<EOF
pre-pr-review: marker is stale for ${branch}.
  reviewed: ${reviewed_head:-<none>}
  current : ${head_sha}

Run a fresh code-review pass, then:
  scripts/pre_pr_review.sh mark
EOF
  exit 1
fi

if [[ "$reviewed_merge_base" != "$merge_base" ]]; then
  cat >&2 <<EOF
pre-pr-review: marker was recorded against a different base for ${branch}.
  reviewed merge-base: ${reviewed_merge_base:-<none>}
  current  merge-base: ${merge_base}
  check base ref      : ${base_ref}

Run a fresh code-review pass against:
  git diff ${base_ref}...HEAD

Then refresh the marker:
  scripts/pre_pr_review.sh mark ${base_ref}
EOF
  exit 1
fi

echo "pre-pr-review: review marker is current for ${branch}"
