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
    if ! verilator --binary -Wno-fatal -Wno-BLKANDNBLK --timing --top-module tb \
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
run "Prob007_wire"           "$EVAL/Prob007_wire.sv"           ''
run "Prob009_popcount3"      "$EVAL/Prob009_popcount3.sv"      ''
run "Prob022_mux2to1"        "$EVAL/Prob022_mux2to1.sv"        ''
run "Prob024_hadd"           "$EVAL/Prob024_hadd.sv"           ''
run "Prob027_fadd"           "$EVAL/Prob027_fadd.sv"           ''
run "Prob122_kmap4"          "$EVAL/Prob122_kmap4.sv"          ''
run "Prob005_notgate"       "$EVAL/prob005_notgate.sv"       ''
run "Prob014_andgate"       "$EVAL/prob014_andgate.sv"       ''
run "Prob040_count10"       "$EVAL/prob040_count10.sv"       ''
run "Prob045_edgedetect2"   "$EVAL/prob045_edgedetect2.sv"   ''
run "Prob060_m2014_q4k"     "$EVAL/prob060_m2014_q4k.sv"     ''
run "Prob075_counter_2bc"   "$EVAL/prob075_counter_2bc.sv"   ''
run "Prob080_timer"         "$EVAL/prob080_timer.sv"         ''
run "Prob100_fsm3comb"      "$EVAL/prob100_fsm3comb.sv"      ''
run "Prob110_fsm2"          "$EVAL/prob110_fsm2.sv"          ''
run "Prob120_fsm3s"         "$EVAL/prob120_fsm3s.sv"         ''
run "Prob130_circuit5"      "$EVAL/prob130_circuit5.sv"      ''
run "Prob038_count15"       "$EVAL/prob038_count15.sv"       ''
run "Prob046_dff8p"         "$EVAL/prob046_dff8p.sv"         ''
run "Prob086_lfsr5"         "$EVAL/prob086_lfsr5.sv"         ''
run "Prob095_review2015_fsmshift" "$EVAL/prob095_review2015_fsmshift.sv" ''
run "Prob115_shift18"       "$EVAL/prob115_shift18.sv"       ''
run "Prob096_review2015_fsmseq" "$EVAL/Prob096_review2015_fsmseq.sv" ''
run "Prob128_fsm_ps2"       "$EVAL/Prob128_fsm_ps2.sv"       ''
run "Prob129_ece241_2013_q8" "$EVAL/Prob129_ece241_2013_q8.sv" ''
run "Prob138_2012_q2fsm"    "$EVAL/Prob138_2012_q2fsm.sv"    ''
run "Prob140_fsm_hdlc"      "$EVAL/Prob140_fsm_hdlc.sv"      ''

# Batch 4: simple combinational
run "Prob002_m2014_q4i"    "$EVAL/Prob002_m2014_q4i.sv"    ''
run "Prob003_step_one"     "$EVAL/Prob003_step_one.sv"     ''
run "Prob004_vector2"      "$EVAL/Prob004_vector2.sv"      ''
run "Prob006_vectorr"      "$EVAL/Prob006_vectorr.sv"      ''
run "Prob008_m2014_q4h"    "$EVAL/Prob008_m2014_q4h.sv"    ''
run "Prob010_mt2015_q4a"   "$EVAL/Prob010_mt2015_q4a.sv"   ''
run "Prob011_norgate"      "$EVAL/Prob011_norgate.sv"      ''
run "Prob012_xnorgate"     "$EVAL/Prob012_xnorgate.sv"     ''
run "Prob013_m2014_q4e"    "$EVAL/Prob013_m2014_q4e.sv"    ''
run "Prob015_vector1"      "$EVAL/Prob015_vector1.sv"      ''
run "Prob016_m2014_q4j"    "$EVAL/Prob016_m2014_q4j.sv"    ''
run "Prob017_mux2to1v"     "$EVAL/Prob017_mux2to1v.sv"     ''
run "Prob019_m2014_q4f"    "$EVAL/Prob019_m2014_q4f.sv"    ''
run "Prob020_mt2015_eq2"   "$EVAL/Prob020_mt2015_eq2.sv"   ''
run "Prob025_reduction"    "$EVAL/Prob025_reduction.sv"    ''
run "Prob026_alwaysblock1" "$EVAL/Prob026_alwaysblock1.sv" ''
run "Prob029_m2014_q4g"    "$EVAL/Prob029_m2014_q4g.sv"    ''
run "Prob032_vector0"      "$EVAL/Prob032_vector0.sv"      ''
run "Prob033_ece241_2014_q1c" "$EVAL/Prob033_ece241_2014_q1c.sv" ''
run "Prob036_ringer"       "$EVAL/Prob036_ringer.sv"       ''
run "Prob039_always_if"    "$EVAL/Prob039_always_if.sv"    ''
run "Prob044_vectorgates"  "$EVAL/Prob044_vectorgates.sv"  ''
run "Prob050_kmap1"        "$EVAL/Prob050_kmap1.sv"        ''
run "Prob051_gates4"       "$EVAL/Prob051_gates4.sv"       ''
run "Prob055_conditional"  "$EVAL/Prob055_conditional.sv"  ''
run "Prob057_kmap2"        "$EVAL/Prob057_kmap2.sv"        ''
run "Prob059_wire4"        "$EVAL/Prob059_wire4.sv"        ''
run "Prob065_7420"         "$EVAL/Prob065_7420.sv"         ''
run "Prob069_truthtable1"  "$EVAL/Prob069_truthtable1.sv"  ''
run "Prob070_ece241_2013_q2" "$EVAL/Prob070_ece241_2013_q2.sv" ''
run "Prob071_always_casez" "$EVAL/Prob071_always_casez.sv" ''
run "Prob072_thermostat"   "$EVAL/Prob072_thermostat.sv"   ''
run "Prob076_always_case"  "$EVAL/Prob076_always_case.sv"  ''
run "Prob077_wire_decl"    "$EVAL/Prob077_wire_decl.sv"    ''
run "Prob079_fsm3onehot"   "$EVAL/Prob079_fsm3onehot.sv"   ''
run "Prob081_7458"         "$EVAL/Prob081_7458.sv"         ''

# Batch 5: sequential
run "Prob031_dff"          "$EVAL/Prob031_dff.sv"          ''
run "Prob034_dff8"         "$EVAL/Prob034_dff8.sv"         ''
run "Prob035_count1to10"   "$EVAL/Prob035_count1to10.sv"   ''
run "Prob037_review2015_count1k" "$EVAL/Prob037_review2015_count1k.sv" ''
run "Prob041_dff8r"        "$EVAL/Prob041_dff8r.sv"        ''
run "Prob047_dff8ar"       "$EVAL/Prob047_dff8ar.sv"       ''
run "Prob048_m2014_q4c"    "$EVAL/Prob048_m2014_q4c.sv"    ''
run "Prob049_m2014_q4b"    "$EVAL/Prob049_m2014_q4b.sv"    ''
run "Prob053_m2014_q4d"    "$EVAL/Prob053_m2014_q4d.sv"    ''
run "Prob054_edgedetect"   "$EVAL/Prob054_edgedetect.sv"   ''
run "Prob056_ece241_2013_q7" "$EVAL/Prob056_ece241_2013_q7.sv" ''
run "Prob058_alwaysblock2" "$EVAL/Prob058_alwaysblock2.sv" ''
run "Prob061_2014_q4a"     "$EVAL/Prob061_2014_q4a.sv"     ''
run "Prob063_review2015_shiftcount" "$EVAL/Prob063_review2015_shiftcount.sv" ''
run "Prob066_edgecapture"  "$EVAL/Prob066_edgecapture.sv"  ''
run "Prob067_countslow"    "$EVAL/Prob067_countslow.sv"    ''
run "Prob073_dff16e"       "$EVAL/Prob073_dff16e.sv"       ''
run "Prob074_ece241_2014_q4" "$EVAL/Prob074_ece241_2014_q4.sv" ''

# Batch 6: simple combinational (new)
run "Prob062_bugs_mux2"       "$EVAL/Prob062_bugs_mux2.sv"       ''
run "Prob083_mt2015_q4b"      "$EVAL/Prob083_mt2015_q4b.sv"      ''
run "Prob087_gates"           "$EVAL/Prob087_gates.sv"           ''
run "Prob090_circuit1"        "$EVAL/Prob090_circuit1.sv"        ''
run "Prob093_ece241_2014_q3"  "$EVAL/Prob093_ece241_2014_q3.sv"  ''
run "Prob101_circuit4"        "$EVAL/Prob101_circuit4.sv"        ''
run "Prob102_circuit3"        "$EVAL/Prob102_circuit3.sv"        ''
run "Prob103_circuit2"        "$EVAL/Prob103_circuit2.sv"        ''
run "Prob106_always_nolatches" "$EVAL/Prob106_always_nolatches.sv" ''
run "Prob112_always_case2"    "$EVAL/Prob112_always_case2.sv"    ''
run "Prob113_2012_q1g"        "$EVAL/Prob113_2012_q1g.sv"        ''
run "Prob114_bugs_case"       "$EVAL/Prob114_bugs_case.sv"       ''
run "Prob116_m2014_q3"        "$EVAL/Prob116_m2014_q3.sv"        ''
run "Prob123_bugs_addsubz"    "$EVAL/Prob123_bugs_addsubz.sv"    ''
run "Prob125_kmap3"           "$EVAL/Prob125_kmap3.sv"           ''
run "Prob132_always_if2"      "$EVAL/Prob132_always_if2.sv"      ''

# Batch 7: vector/loop (new)
run "Prob018_mux256to1"       "$EVAL/Prob018_mux256to1.sv"       ''
run "Prob023_vector100r"      "$EVAL/Prob023_vector100r.sv"      ''
run "Prob042_vector4"         "$EVAL/Prob042_vector4.sv"         ''
run "Prob043_vector5"         "$EVAL/Prob043_vector5.sv"         ''
run "Prob064_vector3"         "$EVAL/Prob064_vector3.sv"         ''
run "Prob092_gatesv100"       "$EVAL/Prob092_gatesv100.sv"       ''
run "Prob094_gatesv"          "$EVAL/Prob094_gatesv.sv"          ''
run "Prob097_mux9to1v"        "$EVAL/Prob097_mux9to1v.sv"        ''

# Batch 8: sequential (new)
run "Prob068_countbcd"        "$EVAL/Prob068_countbcd.sv"        ''
run "Prob082_lfsr32"          "$EVAL/Prob082_lfsr32.sv"          ''
run "Prob084_ece241_2013_q12" "$EVAL/Prob084_ece241_2013_q12.sv" ''
run "Prob085_shift4"          "$EVAL/Prob085_shift4.sv"          ''
run "Prob091_2012_q2b"        "$EVAL/Prob091_2012_q2b.sv"        ''
run "Prob098_circuit7"        "$EVAL/Prob098_circuit7.sv"        ''
# Prob099_m2014_q6c: skipped — dataset bug: test connects Y2/Y4 but ref declares Y1/Y3
run "Prob104_mt2015_muxdff"   "$EVAL/Prob104_mt2015_muxdff.sv"   ''
run "Prob105_rotate100"       "$EVAL/Prob105_rotate100.sv"       ''
run "Prob117_circuit9"        "$EVAL/Prob117_circuit9.sv"        ''
# Prob118_history_shift: skipped — ref module mixes blocking/nonblocking (Verilator BLKANDNBLK)
run "Prob135_m2014_q6b"       "$EVAL/Prob135_m2014_q6b.sv"       ''

# Batch 9: FSM (new)
run "Prob088_ece241_2014_q5b" "$EVAL/Prob088_ece241_2014_q5b.sv" ''
run "Prob089_ece241_2014_q5a" "$EVAL/Prob089_ece241_2014_q5a.sv" ''
run "Prob107_fsm1s"           "$EVAL/Prob107_fsm1s.sv"           ''
run "Prob109_fsm1"            "$EVAL/Prob109_fsm1.sv"            ''
run "Prob111_fsm2s"           "$EVAL/Prob111_fsm2s.sv"           ''
run "Prob119_fsm3"            "$EVAL/Prob119_fsm3.sv"            ''
run "Prob127_lemmings1"       "$EVAL/Prob127_lemmings1.sv"       ''
run "Prob136_m2014_q6"        "$EVAL/Prob136_m2014_q6.sv"        ''
run "Prob137_fsm_serial"      "$EVAL/Prob137_fsm_serial.sv"      ''

# Batch 10: combinational (final)
run "Prob021_mux256to1v"      "$EVAL/Prob021_mux256to1v.sv"      ''
run "Prob030_popcount255"     "$EVAL/Prob030_popcount255.sv"     ''
run "Prob052_gates100"        "$EVAL/Prob052_gates100.sv"        ''
run "Prob126_circuit6"        "$EVAL/Prob126_circuit6.sv"        ''
run "Prob131_mt2015_q4"       "$EVAL/Prob131_mt2015_q4.sv"       ''
run "Prob134_2014_q3c"        "$EVAL/Prob134_2014_q3c.sv"        ''
run "Prob143_fsm_onehot"      "$EVAL/Prob143_fsm_onehot.sv"      ''
run "Prob150_review2015_fsmonehot" "$EVAL/Prob150_review2015_fsmonehot.sv" ''

# Batch 11: sequential (final)
run "Prob108_rule90"          "$EVAL/Prob108_rule90.sv"          ''
run "Prob124_rule110"         "$EVAL/Prob124_rule110.sv"         ''
run "Prob141_count_clock"     "$EVAL/Prob141_count_clock.sv"     ''
run "Prob144_conwaylife"      "$EVAL/Prob144_conwaylife.sv"      ''
run "Prob147_circuit10"       "$EVAL/Prob147_circuit10.sv"       ''
run "Prob153_gshare"          "$EVAL/Prob153_gshare.sv"          ''

# Batch 12: FSM (final)
run "Prob121_2014_q3bfsm"    "$EVAL/Prob121_2014_q3bfsm.sv"    ''
run "Prob133_2014_q3fsm"     "$EVAL/Prob133_2014_q3fsm.sv"     ''
run "Prob139_2013_q2bfsm"    "$EVAL/Prob139_2013_q2bfsm.sv"    ''
run "Prob142_lemmings2"      "$EVAL/Prob142_lemmings2.sv"       ''
run "Prob146_fsm_serialdata" "$EVAL/Prob146_fsm_serialdata.sv"  ''
run "Prob148_2013_q2afsm"   "$EVAL/Prob148_2013_q2afsm.sv"     ''
run "Prob149_ece241_2013_q4" "$EVAL/Prob149_ece241_2013_q4.sv"  ''
run "Prob151_review2015_fsm" "$EVAL/Prob151_review2015_fsm.sv"  ''
run "Prob152_lemmings3"      "$EVAL/Prob152_lemmings3.sv"       ''
run "Prob154_fsm_ps2data"    "$EVAL/Prob154_fsm_ps2data.sv"     ''
run "Prob155_lemmings4"      "$EVAL/Prob155_lemmings4.sv"       ''
run "Prob078_dualedge"        "$EVAL/Prob078_dualedge.sv"        ''
run "Prob028_m2014_q4a"       "$EVAL/Prob028_m2014_q4a.sv"       ''
run "Prob145_circuit8"        "$EVAL/Prob145_circuit8.sv"        ''
run "Prob156_review2015_fancytimer" "$EVAL/Prob156_review2015_fancytimer.sv" ''

echo ""
echo "=== Results: $pass/$total passed, $fail failed ==="
