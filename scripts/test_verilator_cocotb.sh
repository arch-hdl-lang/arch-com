#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

python_request="${PYTHON:-python3}"
python_bin="$("$python_request" -c 'import sys; print(sys.executable)')"
python_dir="$(dirname "$python_bin")"
cocotb_config="${python_dir}/cocotb-config"

if [[ ! -x "$cocotb_config" ]]; then
  echo "ERROR: cocotb-config is not installed next to ${python_bin}" >&2
  echo "Install python/requirements-cocotb-verilator.txt into that environment." >&2
  exit 1
fi
if ! command -v verilator >/dev/null 2>&1; then
  echo "ERROR: verilator is not on PATH" >&2
  exit 1
fi

"$python_bin" -c \
  'import cocotb, cocotbext.axi; print(f"cocotb {cocotb.__version__}, cocotbext-axi {cocotbext.axi.__version__}")'
verilator --version

cocotb_major="$("$python_bin" -c 'import cocotb; print(cocotb.__version__.split(".")[0])')"
if (( cocotb_major >= 2 )); then
  test_module_variable="COCOTB_TEST_MODULES"
else
  test_module_variable="MODULE"
fi

cargo build

build_root="$(mktemp -d /tmp/arch-cocotb-verilator.XXXXXX)"
trap 'rm -rf "$build_root"' EXIT
makefiles="$("$cocotb_config" --makefiles)"
log_level="${COCOTB_LOG_LEVEL:-WARNING}"

check_results() {
  "$python_bin" - "$1" "$2" <<'PY'
import sys
import xml.etree.ElementTree as ET

result_path, label = sys.argv[1:]
root = ET.parse(result_path).getroot()
tests = root.findall(".//testcase")
failures = root.findall(".//failure")
errors = root.findall(".//error")
if not tests:
    print(f"FAIL: {label}: cocotb reported no test cases", file=sys.stderr)
    raise SystemExit(1)
if failures or errors:
    print(
        f"FAIL: {label}: {len(failures)} failures, {len(errors)} errors",
        file=sys.stderr,
    )
    raise SystemExit(1)
print(f"PASS: {label} ({len(tests)} test case)")
PY
}

run_case() {
  local label="$1"
  local source="$2"
  local top="$3"
  local test_dir="$4"
  local test_module="$5"
  local case_dir="${build_root}/${label}"
  local results="${case_dir}/results.xml"

  mkdir -p "$case_dir"
  cp "$source" "${case_dir}/${top}.arch"
  target/debug/arch build "${case_dir}/${top}.arch" \
    -o "${case_dir}/${top}.sv"

  PATH="${python_dir}:${PATH}" \
    PYTHONPATH="${repo_root}/${test_dir}" \
    COCOTB_LOG_LEVEL="$log_level" \
    make -f "${makefiles}/Makefile.sim" \
      SIM=verilator \
      TOPLEVEL_LANG=verilog \
      VERILOG_SOURCES="${case_dir}/${top}.sv" \
      TOPLEVEL="$top" \
      "${test_module_variable}=${test_module}" \
      SIM_BUILD="${case_dir}/sim_build" \
      COCOTB_RESULTS_FILE="$results"

  check_results "$results" "$label"
}

run_case \
  axil \
  tests/cocotb_axi/AxiLiteMemory.arch \
  AxiLiteMemory \
  tests/cocotb_axi \
  test_axil_native
run_case \
  axi \
  tests/cocotb_axi/AxiMemory.arch \
  AxiMemory \
  tests/cocotb_axi \
  test_axi_native
run_case \
  wide \
  tests/cocotb_native/WidePorts.arch \
  WidePorts \
  tests/cocotb_native \
  test_wide_native
