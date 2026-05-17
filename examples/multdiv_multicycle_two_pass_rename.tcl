# yosys-tcl helper for the two-pass synthesis flow. Renames DFF cells
# in BOTH modules of MultdivMulticycleHier so arch-com's emitted SDC
# `*<wire>_reg*` glob resolves cleanly against the post-synth netlist.
#
# Differs from multdiv_multicycle_yosys_rename.tcl in two ways:
#   1. Handles two modules (parent + datapath child), not one.
#   2. Uses arch-com's PR #349 wildcard SDC prefix `*<wire>_reg*`, so
#      the cells can live in either module scope and still match.
#
# Must run AFTER `splitnets`. See multdiv_multicycle_yosys_rename.tcl
# for rationale on why we do this in TCL rather than `rename -wire`.

proc rename_dffs_in_module {modname} {
    # Tolerate missing module: pass 1 of the two-pass flow synthesizes
    # only the child (`MultdivMulticycleDatapath`); pass 2 has both.
    # Same tcl runs in both, so silently skip if module isn't present.
    if {[catch {yosys cd $modname} err]} {
        puts "rename_dffs_in_module: skipping $modname (not present)"
        return
    }
    if {[info exists ::env(OUT_DIR)]} {
        set tmp_dump "$::env(OUT_DIR)/dff_dump_${modname}.txt"
    } else {
        set tmp_dump "/tmp/dff_dump_${modname}.txt"
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
    yosys cd ..
}

rename_dffs_in_module MultdivMulticycleDatapath
rename_dffs_in_module MultdivMulticycleHier
