# yosys-tcl helper invoked from the synth script via `tcl <file>`.
#
# Purpose: rename DFF cells emitted by `dfflibmap` from yosys's
# anonymous `_NNNN_` form to `<wire>_reg_<bit>` (or `<wire>_reg` for
# scalars), so that arch-com's emitted SDC pattern
#     set_multicycle_path ... -to [get_cells {Module/<wire>_reg*}]
# resolves directly against the post-synth netlist without any TCL
# fallback at the OpenSTA stage.
#
# Notes:
#   - Yosys 0.64's `rename -wire` is a no-op for cells driving public
#     bus-indexed nets (e.g. `\mul_result[3]`). The pass appears to
#     skip them; this script does it manually by parsing the textual
#     `dump` of cells of type `sky130_fd_sc_hd__dfxtp_1` (the only
#     flop cell `dfflibmap` produces for our designs).
#   - The naming convention `<wire>_reg_<bit>` was chosen to match
#     arch-com's SDC glob `<wire>_reg*` (PR #347). Both forms
#     `_reg_<bit>` and `_reg<bit>` would match; the underscore form is
#     more conventional in synth flows.
#   - Must be invoked AFTER `splitnets` so the cell's Q connects to a
#     single per-bit wire rather than a bus slice.
#   - Module is hard-coded to `MultdivMulticycle` for the multdiv
#     example. Generalize by parameterizing via an env var if other
#     modules need this treatment in the future.

yosys cd MultdivMulticycle
# Dump path; honor OUT_DIR if set externally (the synth.sh driver
# always sets it before invoking yosys), else fall back to /tmp.
if {[info exists ::env(OUT_DIR)]} {
    set tmp_dump "$::env(OUT_DIR)/dff_dump.txt"
} else {
    set tmp_dump "/tmp/multdiv-synth/dff_dump.txt"
}
yosys dump -o $tmp_dump t:sky130_fd_sc_hd__dfxtp_1
set fp [open $tmp_dump]
set content [read $fp]
close $fp
set cell ""
foreach line [split $content "\n"] {
    if {[regexp {^\s*cell \\\S+\s+(\$\S+)} $line _ cellpriv]} {
        set cell $cellpriv
    } elseif {$cell ne "" && [regexp {^\s*connect \\Q\s+(.+)\s*$} $line _ qval]} {
        set qclean [string trim $qval]
        if {[string index $qclean 0] eq "\\"} {
            set qclean [string range $qclean 1 end]
        }
        # Decompose `<name>[bit]` → `<name>_reg_<bit>`.
        # Scalar  `<name>`      → `<name>_reg`.
        if {[regexp {^(\S+)\[(\d+)\]$} $qclean _ base bit]} {
            set target "${base}_reg_${bit}"
        } else {
            set target "${qclean}_reg"
        }
        if {[catch {yosys rename $cell $target} err]} {
            puts "FAIL: $cell -> $target : $err"
        }
        set cell ""
    }
}
