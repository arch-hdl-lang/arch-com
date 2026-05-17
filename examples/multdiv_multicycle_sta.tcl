#!/usr/bin/env -S sta -no_splash -exit
# OpenSTA driver for the multdiv_multicycle vs ibex_multdiv_fast
# comparison. Reads the post-synth gate-level netlists produced by
# `multdiv_multicycle_synth.sh`, applies a clock + I/O delays, and
# reports WNS/TNS/critical path.
#
# Runs ONE design at a time, picked via the env var DESIGN:
#   DESIGN=mul_nosdc      — multicycle netlist, no SDC (worst case)
#   DESIGN=mul_with_mc    — multicycle netlist, multicycle paths
#                            applied (sources arch-com's emitted .sdc,
#                            with a small flat-prefix translation; see
#                            note below)
#   DESIGN=fsm            — ibex_multdiv_fast FSM netlist, no SDC
#   DESIGN=hier_nosdc     — two-pass hierarchy-split netlist, no SDC
#   DESIGN=hier_with_mc   — two-pass hierarchy-split netlist, multicycle
#                            SDC applied (the post-PR-#349 wildcard form
#                            resolves directly, no prefix rewrite needed)
#
# CLOCK_NS picks the target clock period.

set out_dir "/tmp/multdiv-synth"
set lib "/Users/shuqingzhao/.volare/sky130A/libs.ref/sky130_fd_sc_hd/lib/sky130_fd_sc_hd__tt_025C_1v80.lib"

if {[info exists ::env(DESIGN)]} {
    set design $::env(DESIGN)
} else {
    set design "mul_with_mc"
}
if {[info exists ::env(CLOCK_NS)]} {
    set clk_ns $::env(CLOCK_NS)
} else {
    set clk_ns 4.0
}
if {[info exists ::env(OUT_DIR)]} { set out_dir $::env(OUT_DIR) }
if {[info exists ::env(LIB)]} { set lib $::env(LIB) }

read_liberty $lib

switch -- $design {
    "mul_nosdc" -
    "mul_with_mc" {
        read_verilog $out_dir/MultdivMulticycle_synth.v
        link_design MultdivMulticycle
        set clk_port [get_ports clk]
    }
    "fsm" {
        read_verilog $out_dir/ibex_multdiv_fast_synth.v
        link_design ibex_multdiv_fast
        set clk_port [get_ports clk_i]
    }
    "hier_nosdc" -
    "hier_with_mc" {
        read_verilog $out_dir/MultdivMulticycleHier_synth.v
        link_design MultdivMulticycleHier
        set clk_port [get_ports clk]
    }
    default { error "Unknown DESIGN=$design" }
}

create_clock -name clk -period $clk_ns $clk_port
set_input_delay  -clock clk 0.1 [all_inputs]
set_output_delay -clock clk 0.1 [all_outputs]

# arch-com's emitted SDC uses cell names with the module-name prefix:
#     set_multicycle_path 3 -setup -to [get_cells {MultdivMulticycle/mul_result_reg*}]
# That form is correct for HIERARCHICAL synth (where MultdivMulticycle
# is instantiated under a wrapper). For our STANDALONE / flat synth,
# the top-level cells have no `MultdivMulticycle/` prefix — they
# appear at the root of the netlist. OpenSTA's `get_cells` glob does
# not implicitly strip the top-module name, so `Mod/<reg>*` matches
# nothing and the multicycle constraints fail silently.
#
# Workaround for the comparison: read arch-com's SDC, but rewrite the
# `MultdivMulticycle/` prefix to a flat pattern before sourcing.
# Documented as the second open arch-com SDC compat issue (the first
# was the `[*]`→`get_cells` syntax, fixed in PR #347).
if {$design eq "mul_with_mc"} {
    set sdc_path "$out_dir/multdiv_multicycle.sdc"
    if {![file exists $sdc_path]} {
        error "arch-com SDC not at $sdc_path; run multdiv_multicycle_synth.sh first"
    }
    set fp [open $sdc_path]
    set sdc [read $fp]
    close $fp
    # Strip "MultdivMulticycle/" hierarchical prefix from get_cells globs.
    set sdc_flat [regsub -all {MultdivMulticycle/} $sdc {}]
    set tmpfp [open "$out_dir/multdiv_multicycle.sdc.flat" w]
    puts $tmpfp $sdc_flat
    close $tmpfp
    puts "=== Sourcing arch-com SDC (flat-prefix translated) ==="
    source "$out_dir/multdiv_multicycle.sdc.flat"
}

# For the two-pass hier variant, arch-com's SDC now emits
# `[get_cells -hierarchical {*<reg>_reg*}]` directly (post this PR's
# codegen fix). The `-hierarchical` flag makes OpenSTA's `get_cells`
# descend into instance subhierarchies, so the multicycle cells inside
# `dp/` (the datapath child instance) resolve cleanly with no
# rewriting needed. Source the SDC verbatim.
if {$design eq "hier_with_mc"} {
    set sdc_path "$out_dir/multdiv_multicycle_hier.sdc"
    if {![file exists $sdc_path]} {
        error "arch-com SDC not at $sdc_path; run multdiv_multicycle_two_pass.sh first"
    }
    puts "=== Sourcing arch-com SDC (two-pass hier) ==="
    source $sdc_path
}

puts "=== DESIGN=$design  CLOCK_NS=$clk_ns ==="
report_wns
report_tns
puts "--- Critical path ---"
report_checks -path_delay max -digits 3
