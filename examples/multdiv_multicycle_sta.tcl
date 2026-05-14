#!/usr/bin/env -S sta -no_splash -exit
# OpenSTA driver for the multdiv_multicycle vs ibex_multdiv_fast
# comparison. Reads the post-synth gate-level netlists produced by
# `multdiv_multicycle_synth.sh`, applies a clock + I/O delays, and
# reports WNS/TNS/critical path.
#
# Runs ONE design at a time, picked via the env var DESIGN:
#   DESIGN=mul_nosdc      — multicycle netlist, no SDC (worst case)
#   DESIGN=mul_with_mc    — multicycle netlist, multicycle paths
#                            applied via manual get_pins fallback
#                            (see note below on arch-com SDC parse fail)
#   DESIGN=fsm            — ibex_multdiv_fast FSM netlist, no SDC
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
    default { error "Unknown DESIGN=$design" }
}

create_clock -name clk -period $clk_ns $clk_port
set_input_delay  -clock clk 0.1 [all_inputs]
set_output_delay -clock clk 0.1 [all_outputs]

# For mul_with_mc: arch-com's .sdc emits
#     set_multicycle_path 3 -setup -to {MultdivMulticycle/mul_result_reg[*]}
# which OpenSTA 3.1.0 rejects with "stoi: no conversion" — its parser
# does not accept the brace-pattern object form for -to/-from, and
# requires `-to [get_pins/get_cells ...]`. Additionally, yosys renames
# flop instances to anonymous `_NNNN_`, so a pattern matching the
# original `mul_result_reg[*]` name would not resolve either. The
# helper below walks all flop D-pins, looks at the cell's Q-net, and
# collects D-pins whose Q drives a net matching the original signal
# name. This is the manual translation of the arch-com SDC onto the
# post-synth netlist.
proc collect_flop_d_pins {net_pattern} {
    set d_pins {}
    foreach pin [get_pins -hier *] {
        set pname [get_full_name $pin]
        if {![string match "*/D" $pname]} continue
        set cell_name [string range $pname 0 [expr {[string last / $pname]-1}]]
        set q_net [get_net -of_objects [get_pins ${cell_name}/Q]]
        if {$q_net eq ""} continue
        if {[string match $net_pattern [get_full_name $q_net]]} {
            lappend d_pins $pin
        }
    }
    return $d_pins
}

if {$design eq "mul_with_mc"} {
    set mul_dpins [collect_flop_d_pins {mul_result\[*\]}]
    set div_dpins [collect_flop_d_pins {div_result\[*\]}]
    puts "mul_result D-pins: [llength $mul_dpins]"
    puts "div_result D-pins: [llength $div_dpins]"
    if {[llength $mul_dpins] > 0} {
        set_multicycle_path 3 -setup -to $mul_dpins
        set_multicycle_path 2 -hold  -to $mul_dpins
    }
    if {[llength $div_dpins] > 0} {
        set_multicycle_path 36 -setup -to $div_dpins
        set_multicycle_path 35 -hold  -to $div_dpins
    }
}

puts "=== DESIGN=$design  CLOCK_NS=$clk_ns ==="
report_wns
report_tns
puts "--- Critical path ---"
report_checks -path_delay max -digits 3
