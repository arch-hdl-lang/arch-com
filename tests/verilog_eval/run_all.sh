#!/bin/bash
# Batch run VerilogEval problems through ARCH → SV → Verilator
PROB="/Users/shuqingzhao/github/verilog-eval/dataset_spec-to-rtl"
EVAL="/Users/shuqingzhao/github/arch-com/tests/verilog_eval"
BASE_BUILD="$EVAL/vltor_build"

pass=0; fail=0; total=0

run() {
    local prob="$1"     # e.g. Prob005_notgate
    local sv_src="$2"   # source .sv file
    local renames="$3"  # sed expression for port renaming
    total=$((total + 1))

    local bdir="$BASE_BUILD/$prob"
    mkdir -p "$bdir"

    # Copy and rename
    cp "$sv_src" "$bdir/TopModule.sv"
    if [ -n "$renames" ]; then
        sed -i '' "$renames" "$bdir/TopModule.sv"
    fi

    # Verilator compile
    cd "$bdir"
    rm -rf obj_dir
    if ! verilator --binary -Wno-fatal --timing --top-module tb \
        "$PROB/${prob}_ref.sv" TopModule.sv "$PROB/${prob}_test.sv" \
        -o sim_out 2>verilator_err.txt; then
        echo "FAIL $prob (verilator compile)"
        head -5 verilator_err.txt | sed 's/^/  /'
        fail=$((fail + 1))
        return
    fi

    # Run
    local output
    output=$(./obj_dir/sim_out 2>&1) || true
    if echo "$output" | grep -q "Mismatches: 0 in"; then
        samples=$(echo "$output" | grep "Mismatches:" | sed 's/.*0 in //' | sed 's/ .*//')
        echo "PASS $prob ($samples samples)"
        pass=$((pass + 1))
    else
        mismatch=$(echo "$output" | grep "Mismatches:" || echo "(no output)")
        echo "FAIL $prob: $mismatch"
        echo "$output" | grep -i "hint.*mismatch" | head -3 | sed 's/^/  /'
        fail=$((fail + 1))
    fi
}

echo "=== VerilogEval Benchmark: ARCH HDL ==="
echo ""

run "Prob001_zero"           "$EVAL/Prob001_zero.sv"           ''
run "Prob007_wire"           "$EVAL/Prob007_wire.sv"           's/in_sig/in/g; s/out_sig/out/g'
run "Prob009_popcount3"      "$EVAL/Prob009_popcount3.sv"      's/in_sig/in/g; s/out_sig/out/g'
run "Prob022_mux2to1"        "$EVAL/Prob022_mux2to1.sv"        's/out_sig/out/g'
run "Prob024_hadd"           "$EVAL/Prob024_hadd.sv"           's/sum_sig/sum/g'
run "Prob027_fadd"           "$EVAL/Prob027_fadd.sv"           's/sum_sig/sum/g'
run "Prob122_kmap4"          "$EVAL/Prob122_kmap4.sv"          's/out_sig/out/g'
run "Prob005_notgate"       "$EVAL/prob005_notgate.sv"       's/in_sig/in/g; s/out_sig/out/g'
run "Prob014_andgate"       "$EVAL/prob014_andgate.sv"       's/out_sig/out/g'
run "Prob040_count10"       "$EVAL/prob040_count10.sv"       's/ rst/ reset/g; s/(rst)/(reset)/g; s/,rst/,reset/g'
run "Prob045_edgedetect2"   "$EVAL/prob045_edgedetect2.sv"   's/in_sig/in/g'
run "Prob060_m2014_q4k"     "$EVAL/prob060_m2014_q4k.sv"     's/in_sig/in/g; s/out_sig/out/g'
run "Prob075_counter_2bc"   "$EVAL/prob075_counter_2bc.sv"   's/state_sig/state/g'
run "Prob080_timer"         "$EVAL/prob080_timer.sv"         ''
run "Prob100_fsm3comb"      "$EVAL/prob100_fsm3comb.sv"      's/in_sig/in/g; s/state_sig/state/g; s/out_sig/out/g'
run "Prob110_fsm2"          "$EVAL/prob110_fsm2.sv"          's/out_sig/out/g'
run "Prob120_fsm3s"         "$EVAL/prob120_fsm3s.sv"         's/ rst/ reset/g; s/(rst)/(reset)/g; s/in_sig/in/g; s/out_sig/out/g'
run "Prob130_circuit5"      "$EVAL/prob130_circuit5.sv"      ''
run "Prob038_count15"       "$EVAL/prob038_count15.sv"       's/ rst/ reset/g; s/(rst)/(reset)/g; s/,rst/,reset/g'
run "Prob046_dff8p"         "$EVAL/prob046_dff8p.sv"         's/ rst/ reset/g; s/(rst)/(reset)/g; s/,rst/,reset/g'
run "Prob086_lfsr5"         "$EVAL/prob086_lfsr5.sv"         's/ rst/ reset/g; s/(rst)/(reset)/g; s/,rst/,reset/g'
run "Prob095_review2015_fsmshift" "$EVAL/prob095_review2015_fsmshift.sv" 's/ rst/ reset/g; s/(rst)/(reset)/g; s/,rst/,reset/g'
run "Prob115_shift18"       "$EVAL/prob115_shift18.sv"       ''
run "Prob096_review2015_fsmseq" "$EVAL/Prob096_review2015_fsmseq.sv" 's/ rst/ reset/g; s/(rst)/(reset)/g; s/,rst/,reset/g'
run "Prob128_fsm_ps2"       "$EVAL/Prob128_fsm_ps2.sv"       's/ rst/ reset/g; s/(rst)/(reset)/g; s/,rst/,reset/g; s/in_sig/in/g'
run "Prob129_ece241_2013_q8" "$EVAL/Prob129_ece241_2013_q8.sv" ''
run "Prob138_2012_q2fsm"    "$EVAL/Prob138_2012_q2fsm.sv"    's/ rst/ reset/g; s/(rst)/(reset)/g; s/,rst/,reset/g'
run "Prob140_fsm_hdlc"      "$EVAL/Prob140_fsm_hdlc.sv"      's/ rst/ reset/g; s/(rst)/(reset)/g; s/,rst/,reset/g; s/in_sig/in/g'

echo ""
echo "=== Results: $pass/$total passed, $fail failed ==="
