#!/bin/bash
# RealBench integration test runner
# Usage: ./run_realbench.sh <problem_set> [module_name]
# Example: ./run_realbench.sh e203_hbirdv2
#          ./run_realbench.sh sdc sd_crc_7

set -euo pipefail

ARCH_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
REALBENCH_DIR="$ARCH_DIR/../RealBench"
ARCH_BIN="$ARCH_DIR/target/release/arch"

PROBLEM_SET="${1:?Usage: $0 <problem_set> [module_name]}"
MODULE_FILTER="${2:-}"

# Build arch compiler in release mode (faster)
if [ ! -f "$ARCH_BIN" ]; then
    echo "Building arch compiler (release)..."
    cd "$ARCH_DIR" && cargo build --release --quiet
fi

# Determine test directories and arch source dirs
case "$PROBLEM_SET" in
    e203_hbirdv2|e203)
        BENCH_DIR="$REALBENCH_DIR/e203_hbirdv2"
        ARCH_SRC_DIR="$ARCH_DIR/tests/e203"
        ;;
    sdc)
        BENCH_DIR="$REALBENCH_DIR/sdc"
        ARCH_SRC_DIR="$ARCH_DIR/tests/sdc"
        ;;
    aes)
        BENCH_DIR="$REALBENCH_DIR/aes"
        ARCH_SRC_DIR="$ARCH_DIR/tests/aes"
        ;;
    *)
        echo "Unknown problem set: $PROBLEM_SET"
        exit 1
        ;;
esac

PASS=0
FAIL=0
SKIP=0
ERRORS=""

# Get module list
if [ -n "$MODULE_FILTER" ]; then
    MODULES="$MODULE_FILTER"
else
    MODULES=$(ls -d "$BENCH_DIR"/*/verification 2>/dev/null | xargs -I{} dirname {} | xargs -I{} basename {})
fi

TOTAL=$(echo "$MODULES" | wc -w | tr -d ' ')
echo "Running $TOTAL RealBench $PROBLEM_SET integration tests..."
echo "============================================================"

for MOD in $MODULES; do
    VERIF_DIR="$BENCH_DIR/$MOD/verification"
    TOP_SV="$VERIF_DIR/${MOD}_top.sv"
    ARCH_FILE="$ARCH_SRC_DIR/$MOD.arch"

    if [ ! -d "$VERIF_DIR" ]; then
        echo "SKIP $MOD (no verification dir)"
        SKIP=$((SKIP + 1))
        continue
    fi

    if [ ! -f "$ARCH_FILE" ]; then
        echo "SKIP $MOD (no .arch file)"
        SKIP=$((SKIP + 1))
        continue
    fi

    printf "%-45s " "$MOD"

    # Create temp work directory
    WORK_DIR=$(mktemp -d)
    trap "rm -rf $WORK_DIR" EXIT

    # Copy all verification files
    cp "$VERIF_DIR"/* "$WORK_DIR/" 2>/dev/null || true

    # Build ARCH -> SV per-module. The compiler auto-discovers
    # transitively-needed .archi files in $ARCH_SRC_DIR (cwd-based
    # search), so each module's build is isolated from unrelated drift
    # in OTHER arch sources. This means a stale wrapper module (e.g.
    # arch-side e203_core_top.arch with broken inst port lists) no
    # longer takes down the whole batch.
    MOD_ARCH="$ARCH_SRC_DIR/$MOD.arch"
    GEN_FILE="$ARCH_SRC_DIR/$MOD.sv"

    if [ ! -f "$MOD_ARCH" ]; then
        echo "SKIP (no .arch source for $MOD)"
        SKIP=$((SKIP + 1))
        rm -rf "$WORK_DIR"
        continue
    fi

    if ! (cd "$ARCH_SRC_DIR" && "$ARCH_BIN" build "$MOD.arch" 2>"$WORK_DIR/arch_err.txt"); then
        echo "FAIL (arch build error)"
        FAIL=$((FAIL + 1))
        ERRORS="$ERRORS\n  $MOD: arch build failed - $(head -1 $WORK_DIR/arch_err.txt)"
        rm -rf "$WORK_DIR"
        continue
    fi

    # The generated .sv lands next to the source; copy to expected place.
    # The ARCH compiler may inline transitive dependencies into the main output,
    # so the top SV already includes sub-modules. Remove any dependency .sv files
    # that also exist in the work dir to avoid MODDUP.
    if [ -f "$GEN_FILE" ]; then
        cp "$GEN_FILE" "$TOP_SV"
        # Remove .sv files for instantiated sub-modules (already inlined in top SV)
        for sv in "$WORK_DIR"/*.sv; do
            bn=$(basename "$sv" .sv)
            if [ "$bn" != "${MOD}" ] && grep -q "^module $bn " "$TOP_SV" 2>/dev/null; then
                rm -f "$sv"
            fi
        done
    fi

    # Run Verilator compile
    cd "$WORK_DIR"

    # Auto-detect top module name from testbench (some use 'tb', others 'tb_<mod>')
    TOP_MOD="tb"
    if grep -q '^module tb_' *_testbench.sv 2>/dev/null; then
        TOP_MOD=$(grep '^module tb_' *_testbench.sv | head -1 | sed 's/.*module \(tb_[^(]*\).*/\1/')
    fi
    VFLAGS="-cc --exe --binary --trace --assert --timing -j 4 --top $TOP_MOD"
    VFLAGS="$VFLAGS -Wno-SIDEEFFECT -Wno-CASEOVERLAP -Wno-LATCH -Wno-UNOPTFLAT"
    VFLAGS="$VFLAGS -Wno-MULTIDRIVEN -Wno-ASCRANGE -Wno-COMBDLY -Wno-IMPLICIT"
    VFLAGS="$VFLAGS -Wno-CASEINCOMPLETE -Wno-PINMISSING -Wno-WIDTHTRUNC"
    VFLAGS="$VFLAGS -Wno-MODDUP -Wno-WIDTHEXPAND"
    VFLAGS="$VFLAGS -Wno-TIMESCALEMOD -Wno-INITIALDLY -Wno-EOFNEWLINE"
    VFLAGS="$VFLAGS -Wno-DECLFILENAME -Wno-WIDTHEXPAND -Wno-WIDTHCONCAT"
    VFLAGS="$VFLAGS -fno-table"

    if ! verilator $VFLAGS *.v *.sv 2>"$WORK_DIR/vltor_err.txt"; then
        echo "FAIL (verilator compile)"
        FAIL=$((FAIL + 1))
        ERRORS="$ERRORS\n  $MOD: verilator compile - $(grep '%Error' $WORK_DIR/vltor_err.txt | head -3)"
        cd "$ARCH_DIR"
        rm -rf "$WORK_DIR"
        continue
    fi

    # Run simulation with timeout (portable: macOS lacks `timeout`)
    if perl -e 'alarm shift; exec @ARGV' 30 obj_dir/Vtb >"$WORK_DIR/sim_out.txt" 2>&1; then
        # Check for mismatches
        if grep -q "Mismatches: 0" "$WORK_DIR/sim_out.txt"; then
            SAMPLES=$(grep -o '[0-9]* samples' "$WORK_DIR/sim_out.txt" | head -1)
            echo "PASS ($SAMPLES)"
            PASS=$((PASS + 1))
        elif grep -q "Hint: Total mismatched samples is 0" "$WORK_DIR/sim_out.txt"; then
            SAMPLES=$(grep -o '[0-9]* samples' "$WORK_DIR/sim_out.txt" | head -1)
            echo "PASS ($SAMPLES)"
            PASS=$((PASS + 1))
        elif grep -q "mismatched" "$WORK_DIR/sim_out.txt"; then
            MISMATCH=$(grep -o '[0-9]* mismatched' "$WORK_DIR/sim_out.txt" | head -1)
            echo "FAIL ($MISMATCH)"
            FAIL=$((FAIL + 1))
            ERRORS="$ERRORS\n  $MOD: $MISMATCH"
        else
            echo "PASS (completed)"
            PASS=$((PASS + 1))
        fi
    else
        echo "FAIL (timeout/crash)"
        FAIL=$((FAIL + 1))
        ERRORS="$ERRORS\n  $MOD: sim timeout or crash"
    fi

    cd "$ARCH_DIR"
    rm -rf "$WORK_DIR"
done

echo ""
echo "============================================================"
echo "Results: $PASS PASS, $FAIL FAIL, $SKIP SKIP (of $TOTAL)"
if [ -n "$ERRORS" ]; then
    echo ""
    echo "Failures:"
    echo -e "$ERRORS"
fi
