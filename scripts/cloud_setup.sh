#!/usr/bin/env bash
# Toolchain setup for a Claude Code cloud session (or any fresh Ubuntu 24.04
# box): Lean 4 for the proof backends and Verilator 5.034 for the SV legs of
# the test suite. Mirrors the pins of harc-com's `harc-ci` Lean image.
#
# Cloud environment: set the environment's Setup script to
#
#     bash arch-com/scripts/cloud_setup.sh
#
# (the setup script starts one directory above the repository). Safe to re-run:
# each tool is skipped when the pinned version is already installed.
#
# Only GitHub release/clone URLs are used, because the default cloud network
# policy blocks release.lean-lang.org (elan's own toolchain mirror). The Lean
# toolchain is therefore downloaded directly and registered with
# `elan toolchain link` under the name the repo's `lean-toolchain` requests.
set -euo pipefail

ELAN_VERSION=4.2.4
ELAN_SHA256=42b94d4244e8353142c456ec0e4ca6528fd898a6c604d4059f494e706e431f63
LEAN_VERSION=4.30.0
LEAN_ZIP_SHA256=3ffb3dc406912936a6b30885ce47a349c7ed8ee7e4e4dfac7361a497608bc8d1
VERILATOR_VERSION=5.034

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
sudo_cmd=""
[[ "$(id -u)" -ne 0 ]] && sudo_cmd="sudo"
log() { printf '[cloud_setup] %s\n' "$*"; }

# The pins above must match what the Lean projects request.
for tc in "$repo_root"/proofs/*/lean-toolchain; do
  if [[ "$(cat "$tc")" != "leanprover/lean4:v${LEAN_VERSION}" ]]; then
    echo "error: $tc requests $(cat "$tc"), but this script pins v${LEAN_VERSION};" \
      "update LEAN_VERSION and LEAN_ZIP_SHA256 together" >&2
    exit 1
  fi
done

# ── Lean ──────────────────────────────────────────────────────────────────
export ELAN_HOME="${ELAN_HOME:-$HOME/.elan}"
export PATH="$ELAN_HOME/bin:$PATH"
if lean --version 2>/dev/null | grep -q "version ${LEAN_VERSION}"; then
  log "Lean ${LEAN_VERSION} already installed"
else
  tmp="$(mktemp -d)"
  log "installing elan ${ELAN_VERSION}"
  curl --proto '=https' --tlsv1.2 -sSfL -o "$tmp/elan.tar.gz" \
    "https://github.com/leanprover/elan/releases/download/v${ELAN_VERSION}/elan-x86_64-unknown-linux-gnu.tar.gz"
  echo "${ELAN_SHA256}  $tmp/elan.tar.gz" | sha256sum -c - >/dev/null
  tar -xzf "$tmp/elan.tar.gz" -C "$tmp"
  "$tmp/elan-init" -y --no-modify-path --default-toolchain none >/dev/null
  log "installing Lean ${LEAN_VERSION}"
  curl --proto '=https' --tlsv1.2 -sSfL -o "$tmp/lean.zip" \
    "https://github.com/leanprover/lean4/releases/download/v${LEAN_VERSION}/lean-${LEAN_VERSION}-linux.zip"
  echo "${LEAN_ZIP_SHA256}  $tmp/lean.zip" | sha256sum -c - >/dev/null
  mkdir -p "$ELAN_HOME/linked"
  rm -rf "$ELAN_HOME/linked/lean-${LEAN_VERSION}-linux"
  unzip -q "$tmp/lean.zip" -d "$ELAN_HOME/linked"
  elan toolchain link "leanprover/lean4:v${LEAN_VERSION}" "$ELAN_HOME/linked/lean-${LEAN_VERSION}-linux"
  elan default "leanprover/lean4:v${LEAN_VERSION}" >/dev/null
  rm -rf "$tmp"
fi
lean --version
log "building the Lean proof library (needed by --check-*-proof-lean)"
(cd "$repo_root/proofs/lean_thread_lowering" && lake build 2>&1 | tail -1)

# ── Verilator ─────────────────────────────────────────────────────────────
if verilator --version 2>/dev/null | grep -q "Verilator ${VERILATOR_VERSION} "; then
  log "Verilator ${VERILATOR_VERSION} already installed"
else
  log "building Verilator ${VERILATOR_VERSION} from source (apt only ships 5.020)"
  $sudo_cmd apt-get update -qq
  DEBIAN_FRONTEND=noninteractive $sudo_cmd apt-get install -y -qq git make autoconf g++ \
    flex bison ccache help2man libgoogle-perftools-dev numactl perl-doc libfl2 libfl-dev \
    zlib1g zlib1g-dev >/dev/null
  src="$(mktemp -d)"
  git clone -q --depth 1 --branch "v${VERILATOR_VERSION}" https://github.com/verilator/verilator "$src"
  (cd "$src" && autoconf && ./configure -q && make -s -j"$(nproc)" >/dev/null && $sudo_cmd make -s install >/dev/null)
  rm -rf "$src"
fi
verilator --version

log "done. Add \$HOME/.elan/bin to PATH in new shells if \`lake\` is not found" \
  "(arch already falls back to ~/.elan/bin/lake on its own)."
