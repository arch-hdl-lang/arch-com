#!/bin/bash
# Run a VerilogEval problem against ARCH-generated SV
# Usage: ./run_bench.sh <prob_dir> <arch_file> [port_map]
# port_map: comma-separated old=new pairs, e.g. "in_sig=in,out_sig=out"

set -e

PROB_DIR="/Users/shuqingzhao/github/verilog-eval/dataset_spec-to-rtl"
ARCH_DIR="/Users/shuqingzhao/github/arch-com"
BUILD_DIR="/Users/shuqingzhao/github/arch-com/tests/verilog_eval/vltor_build"

prob_name="$1"
arch_file="$2"
port_map="$3"

# Find the test files
prompt_file="$PROB_DIR/${prob_name}_prompt.txt"
ref_file="$PROB_DIR/${prob_name}_ref.sv"
test_file="$PROB_DIR/${prob_name}_test.sv"

if [ ! -f "$ref_file" ]; then
    echo "ERROR: $ref_file not found"
    exit 1
fi

# Build ARCH to SV
sv_file="${arch_file%.arch}.sv"
(cd "$ARCH_DIR" && cargo run --quiet -- build "$arch_file" 2>&1)

# If port_map is provided, create a wrapper
if [ -n "$port_map" ]; then
    # Read the generated SV to get the inner module interface
    inner_sv="$ARCH_DIR/$sv_file"
    wrapper_file="$BUILD_DIR/TopModule_wrapper.sv"
    mkdir -p "$BUILD_DIR"

    # Rename the inner module to TopModuleInner
    sed 's/module TopModule/module TopModuleInner/' "$inner_sv" > "$BUILD_DIR/TopModuleInner.sv"

    # Build wrapper: parse port map and generate connections
    echo "// Auto-generated wrapper" > "$wrapper_file"

    # Extract original port list from ref file for the wrapper interface
    # Use the ref module's port list as the target interface
    ref_ports=$(sed -n '/^module RefModule/,/);/p' "$ref_file" | grep -v 'module\|);')

    echo "module TopModule (" >> "$wrapper_file"
    echo "$ref_ports" >> "$wrapper_file"
    echo ");" >> "$wrapper_file"
    echo "  TopModuleInner inner (" >> "$wrapper_file"

    # Parse port_map: arch_name=bench_name,...
    IFS=',' read -ra MAPS <<< "$port_map"
    first=1
    for map in "${MAPS[@]}"; do
        arch_port="${map%%=*}"
        bench_port="${map##*=}"
        if [ $first -eq 0 ]; then echo "," >> "$wrapper_file"; fi
        printf "    .%s(%s)" "$arch_port" "$bench_port" >> "$wrapper_file"
        first=0
    done
    echo "" >> "$wrapper_file"
    echo "  );" >> "$wrapper_file"
    echo "endmodule" >> "$wrapper_file"

    dut_files="$BUILD_DIR/TopModuleInner.sv $wrapper_file"
else
    dut_files="$ARCH_DIR/$sv_file"
fi

# Run Verilator
mkdir -p "$BUILD_DIR"
cd "$BUILD_DIR"
verilator --binary -Wall -Wno-DECLFILENAME -Wno-UNUSEDSIGNAL -Wno-UNUSEDPARAM \
    -Wno-WIDTHEXPAND -Wno-WIDTHTRUNC -Wno-UNDRIVEN -Wno-PINMISSING \
    --timing --top-module tb \
    "$ref_file" $dut_files "$test_file" \
    -o sim_out 2>&1

if [ $? -ne 0 ]; then
    echo "VERILATOR COMPILE FAILED"
    exit 1
fi

# Run simulation
timeout 10 ./obj_dir/sim_out 2>&1 || true
