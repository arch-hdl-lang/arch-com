#!/usr/bin/env bash
# Sweep every .arch file under the given directories (default: tests examples)
# through `arch build --check-codegen-proof-lean` and report:
#
#   CERTIFIED  every module in the file's covered subset replayed OK in Lean
#   FAIL       a certificate was emitted but Lean REJECTED it — either a real
#              ARCH->SV miscompile or a gap in the validator; both need a look
#   SKIP       no module in the file is in the covered subset (or the file does
#              not build standalone)
#
# Exits 1 if any file FAILs. Requires a built Lean project:
#   (cd proofs/lean_thread_lowering && lake build)
#
# Usage: scripts/codegen_cert_sweep.sh [-j N] [dir ...]
set -euo pipefail

jobs=8
if [[ "${1:-}" == "-j" ]]; then
  jobs="$2"
  shift 2
fi
repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"
dirs=("$@")
[[ ${#dirs[@]} -eq 0 ]] && dirs=(tests examples)

cargo build --release --quiet
arch_bin="$repo_root/target/release/arch"
project="$repo_root/proofs/lean_thread_lowering"
work="$(mktemp -d)"
trap 'rm -rf -- "$work"' EXIT

check_one() {
  local f="$1" arch_bin="$2" project="$3" work="$4"
  local id cert
  id="$(echo "$f" | tr '/' '_')"
  cert="$work/$id.lean"
  if (cd "$(dirname "$f")" && ARCH_NO_LEARN=1 "$arch_bin" build "$(basename "$f")" \
        -o "$work/$id.sv" --emit-codegen-proof-lean="$cert" --check-codegen-proof-lean \
        --codegen-proof-lean-project="$project" >"$work/$id.log" 2>&1); then
    echo "CERTIFIED $f"
  elif [[ -f "$cert" ]]; then
    echo "FAIL $f (log: see below)"
    sed 's/^/    /' "$work/$id.log"
  else
    echo "SKIP $f"
  fi
}
export -f check_one

find "${dirs[@]}" -name '*.arch' | sort |
  xargs -P "$jobs" -I{} bash -c 'check_one "$@"' _ {} "$arch_bin" "$project" "$work" \
  >"$work/results.txt"

grep -v '^    ' "$work/results.txt" | cut -d' ' -f1 | sort | uniq -c
if grep -q '^FAIL' "$work/results.txt"; then
  echo
  grep -A1000 '^FAIL' "$work/results.txt" | grep -v '^CERTIFIED\|^SKIP'
  exit 1
fi
