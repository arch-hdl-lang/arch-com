#!/usr/bin/env bash
# Sweep every .arch file under the given directories (default: tests examples)
# through `arch build --check-codegen-proof-lean` and report:
#
#   CERTIFIED  every module in the file's covered subset replayed OK in Lean
#   FAIL       a certificate was emitted but Lean REJECTED it — either a real
#              ARCH->SV miscompile or a gap in the validator; both need a look
#   KNOWN      a FAIL listed in scripts/codegen_cert_known_divergences.txt: a
#              known, open spec/implementation divergence with an issue
#   SKIP       no module in the file is in the covered subset (or the file does
#              not build standalone)
#
# Exits 1 if any file FAILs, or if a listed known divergence no longer FAILs
# (remove its entry once the divergence is resolved). Requires a built Lean
# project:
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

known="$repo_root/scripts/codegen_cert_known_divergences.txt"

check_one() {
  local f="$1" arch_bin="$2" project="$3" work="$4" known="$5"
  local id cert
  id="$(echo "$f" | tr '/' '_')"
  cert="$work/$id.lean"
  if (cd "$(dirname "$f")" && ARCH_NO_LEARN=1 "$arch_bin" build "$(basename "$f")" \
        -o "$work/$id.sv" --emit-codegen-proof-lean="$cert" --check-codegen-proof-lean \
        --codegen-proof-lean-project="$project" >"$work/$id.log" 2>&1); then
    echo "CERTIFIED $f"
  elif [[ -f "$cert" ]] && issue="$(awk -v f="$f" '$1 == f { print $2 }' "$known")" &&
      [[ -n "$issue" ]]; then
    echo "KNOWN $f ($issue)"
  elif [[ -f "$cert" ]]; then
    echo "FAIL $f (log: see below)"
    sed 's/^/    /' "$work/$id.log"
  else
    echo "SKIP $f"
  fi
}
export -f check_one

find "${dirs[@]}" -name '*.arch' | sort |
  xargs -P "$jobs" -I{} bash -c 'check_one "$@"' _ {} "$arch_bin" "$project" "$work" "$known" \
  >"$work/results.txt"

grep -v '^    ' "$work/results.txt" | cut -d' ' -f1 | sort | uniq -c
status=0
if grep -q '^KNOWN' "$work/results.txt"; then
  echo
  grep '^KNOWN' "$work/results.txt"
fi
# A listed divergence that was swept but no longer FAILs is a stale entry.
while read -r path issue _; do
  [[ -z "$path" || "$path" == \#* ]] && continue
  if grep -q "^[A-Z]* $path\( \|$\)" "$work/results.txt" &&
    ! grep -q "^KNOWN $path " "$work/results.txt"; then
    echo "STALE known divergence (no longer fails): $path $issue"
    status=1
  fi
done <"$known"
if grep -q '^FAIL' "$work/results.txt"; then
  echo
  grep -A1000 '^FAIL' "$work/results.txt" | grep -v '^CERTIFIED\|^SKIP\|^KNOWN'
  status=1
fi
exit "$status"
