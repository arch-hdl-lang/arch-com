#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage:
  scripts/sync_skill_snapshots.sh check    # verify snapshots match their sources (CI)
  scripts/sync_skill_snapshots.sh refresh  # copy sources over snapshots (local)

The arch-programming skill bundles point-in-time copies of core ARCH docs under
skills/arch-programming/references/ so the skill works when installed outside an
arch-com checkout. This script is the single source of truth for the
source -> snapshot mapping. `check` fails (exit 1) if any snapshot has drifted;
`refresh` re-copies every source over its snapshot.
USAGE
}

mode="${1:-}"
if [[ "$mode" != "check" && "$mode" != "refresh" ]]; then
  usage >&2
  exit 2
fi

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

ref_dir="skills/arch-programming/references"

# source path                         snapshot filename under $ref_dir
mappings=(
  "README.md|README.md"
  "doc/Arch_AI_Reference_Card.md|Arch_AI_Reference_Card.md"
  "doc/ARCH_HDL_Specification.md|ARCH_HDL_Specification.md"
  "doc/COMPILER_STATUS.md|COMPILER_STATUS.md"
  "doc/arch_sim_cocotb.md|arch_sim_cocotb.md"
  "doc/plan_arch_doc_comments.md|plan_arch_doc_comments.md"
  "mcp/README.md|mcp_README.md"
  "mcp/instructions.md|mcp_instructions.md"
  "LICENSE|LICENSE"
)

drift=0
for entry in "${mappings[@]}"; do
  src="${entry%%|*}"
  dst="$ref_dir/${entry##*|}"

  if [[ ! -f "$src" ]]; then
    echo "ERROR: missing source $src" >&2
    drift=1
    continue
  fi
  if [[ ! -f "$dst" ]]; then
    echo "ERROR: missing snapshot $dst" >&2
    drift=1
    continue
  fi

  if [[ "$mode" == "refresh" ]]; then
    cp "$src" "$dst"
    continue
  fi

  if ! cmp -s "$src" "$dst"; then
    echo "DRIFT: $dst is out of sync with $src" >&2
    drift=1
  fi
done

if [[ "$mode" == "refresh" ]]; then
  echo "Refreshed ${#mappings[@]} skill reference snapshot(s)."
  exit 0
fi

if [[ "$drift" -ne 0 ]]; then
  cat >&2 <<'MSG'

One or more arch-programming skill snapshots are stale.
Run `scripts/sync_skill_snapshots.sh refresh` and commit the result.
MSG
  exit 1
fi

echo "All ${#mappings[@]} skill reference snapshots are in sync."
