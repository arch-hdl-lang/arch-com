#!/bin/bash
# Run one VerilogEval problem: build ARCH → SV, rename ports, Verilator test
# Usage: ./run_prob.sh <prob_id> <arch_file> [rename:arch_port=sv_port,...]
set -e

PROB_DIR="/Users/shuqingzhao/github/verilog-eval/dataset_spec-to-rtl"
ARCH_COM="/Users/shuqingzhao/github/arch-com"
BUILD="$ARCH_COM/tests/verilog_eval/vltor_build"

prob="$1"
arch="$2"
renames="$3"

ref="$PROB_DIR/${prob}_ref.sv"
test="$PROB_DIR/${prob}_test.sv"

# Build ARCH → SV
sv="${arch%.arch}.sv"
(cd "$ARCH_COM" && cargo run --quiet -- build "$arch" 2>&1) || { echo "ARCH BUILD FAILED: $prob"; exit 1; }

# Apply port renames if needed
mkdir -p "$BUILD"
cp "$ARCH_COM/$sv" "$BUILD/TopModule.sv"
if [ -n "$renames" ]; then
    IFS=',' read -ra MAPS <<< "$renames"
    for map in "${MAPS[@]}"; do
        from="${map%%=*}"
        to="${map##*=}"
        sed -i '' "s/\b${from}\b/${to}/g" "$BUILD/TopModule.sv"
    done
fi

# Verilator compile + run
cd "$BUILD"
rm -rf obj_dir
verilator --binary -Wno-fatal --timing --top-module tb \
    "$ref" TopModule.sv "$test" -o sim_out 2>/dev/null

output=$(timeout 10 ./obj_dir/sim_out 2>&1)
mismatches=$(echo "$output" | grep "Mismatches:" | grep -o '[0-9]* in [0-9]*')
if echo "$output" | grep -q "Mismatches: 0 in"; then
    echo "PASS $prob ($mismatches samples)"
else
    echo "FAIL $prob ($mismatches)"
    echo "$output" | grep -i "hint\|mismatch\|error" | head -5
fi
