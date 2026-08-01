#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

python_bin="${PYTHON:-python3}"
"$python_bin" -c 'import pybind11, cocotbext.axi'

cargo build

shim_path="${repo_root}/python/cocotb_shim:${repo_root}/python"
PYTHONDONTWRITEBYTECODE=1 PYTHONPATH="$shim_path" \
  "$python_bin" -m unittest discover -s python/tests -v

build_root="$(mktemp -d /tmp/arch-native-cocotb.XXXXXX)"
trap 'rm -rf "$build_root"' EXIT

python_dir="$(dirname "$python_bin")"
export PATH="${python_dir}:${PATH}"
unset ARCH_PYTHON_DIR

target/debug/arch sim --pybind \
  --test tests/cocotb_axi/test_axil_native.py \
  --outdir "${build_root}/axil" \
  tests/cocotb_axi/AxiLiteMemory.arch

target/debug/arch sim --pybind \
  --test tests/cocotb_axi/test_axi_native.py \
  --outdir "${build_root}/axi" \
  tests/cocotb_axi/AxiMemory.arch

target/debug/arch sim --pybind --inputs-start-uninit \
  --test tests/cocotb_native/test_wide_native.py \
  --outdir "${build_root}/wide" \
  tests/cocotb_native/WidePorts.arch
