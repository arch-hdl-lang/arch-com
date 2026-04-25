#!/usr/bin/env python3
"""Behavioral test for the cam-refactored TLB. Drives the same scenarios
the original CVDP harness exercises: hit/miss, fill, replacement wrap,
flush, and post-flush re-fill. iverilog-only (no cocotb)."""
import subprocess, os, sys, tempfile

sv_file = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'TLB.sv')
workdir = tempfile.mkdtemp(prefix='tlb_sim_')

tb = """
`timescale 1ns/1ns
module tb;
  parameter TLB_SIZE = 4;
  parameter ADDR_WIDTH = 8;
  parameter PAGE_WIDTH = 8;

  reg clk = 0;
  reg rst = 0;
  reg [ADDR_WIDTH-1:0] virtual_address = 0;
  reg tlb_write_enable = 0;
  reg flsh = 0;
  reg [PAGE_WIDTH-1:0] page_table_entry = 0;
  wire [PAGE_WIDTH-1:0] physical_address;
  wire hit, miss;

  TLB #(.TLB_SIZE(TLB_SIZE), .ADDR_WIDTH(ADDR_WIDTH), .PAGE_WIDTH(PAGE_WIDTH)) dut (
    .clk(clk), .rst(rst),
    .virtual_address(virtual_address),
    .tlb_write_enable(tlb_write_enable),
    .flsh(flsh),
    .page_table_entry(page_table_entry),
    .physical_address(physical_address),
    .hit(hit), .miss(miss)
  );

  always #5 clk = ~clk;

  integer pass_count = 0;
  integer fail_count = 0;

  task check_miss(input [ADDR_WIDTH-1:0] vaddr, input [127:0] tag);
    begin
      virtual_address = vaddr;
      #1;
      if (miss && !hit) begin pass_count = pass_count + 1; $display("%0s: MISS as expected", tag); end
      else begin fail_count = fail_count + 1; $display("FAIL %0s: expected miss, got hit=%0d miss=%0d", tag, hit, miss); end
    end
  endtask

  task check_hit(input [ADDR_WIDTH-1:0] vaddr, input [PAGE_WIDTH-1:0] expected_pa, input [127:0] tag);
    begin
      virtual_address = vaddr;
      #1;
      if (hit && !miss && physical_address == expected_pa) begin
        pass_count = pass_count + 1;
        $display("%0s: HIT vaddr=%02x -> pa=%02x", tag, vaddr, physical_address);
      end else begin
        fail_count = fail_count + 1;
        $display("FAIL %0s: vaddr=%02x expected hit pa=%02x, got hit=%0d miss=%0d pa=%02x",
                 tag, vaddr, expected_pa, hit, miss, physical_address);
      end
    end
  endtask

  task fill_entry(input [ADDR_WIDTH-1:0] vaddr, input [PAGE_WIDTH-1:0] pa);
    begin
      virtual_address = vaddr;
      page_table_entry = pa;
      tlb_write_enable = 1;
      @(posedge clk); #1;
      tlb_write_enable = 0;
    end
  endtask

  task do_reset;
    begin
      rst = 1;
      @(posedge clk); #1;
      @(posedge clk); #1;
      rst = 0;
      @(posedge clk); #1;
    end
  endtask

  initial begin
    do_reset;

    // ── 1. Cold TLB: every lookup misses ──
    check_miss(8'h10, "Cold-1");
    check_miss(8'hAB, "Cold-2");

    // ── 2. Fill four entries; verify each hits ──
    fill_entry(8'h10, 8'hA0);
    fill_entry(8'h20, 8'hA1);
    fill_entry(8'h30, 8'hA2);
    fill_entry(8'h40, 8'hA3);

    check_hit(8'h10, 8'hA0, "Fill-1");
    check_hit(8'h20, 8'hA1, "Fill-2");
    check_hit(8'h30, 8'hA2, "Fill-3");
    check_hit(8'h40, 8'hA3, "Fill-4");

    // ── 3. Unknown vaddr still misses ──
    check_miss(8'h99, "Post-fill miss");

    // ── 4. Replacement wraps (TLB_SIZE=4 → 5th fill overwrites slot 0) ──
    fill_entry(8'h50, 8'hB5);
    check_hit(8'h50, 8'hB5, "Replace-new");
    check_miss(8'h10, "Replace-evicted");

    // ── 5. Flush wipes everything ──
    flsh = 1;
    @(posedge clk); #1;
    flsh = 0;
    check_miss(8'h20, "Post-flush-1");
    check_miss(8'h30, "Post-flush-2");
    check_miss(8'h40, "Post-flush-3");
    check_miss(8'h50, "Post-flush-4");

    // ── 6. Re-fill after flush works ──
    fill_entry(8'h77, 8'hC7);
    check_hit(8'h77, 8'hC7, "Post-flush-fill");

    if (fail_count == 0) $display("ALL PASS (%0d tests)", pass_count);
    else $display("FAIL: %0d/%0d failed", fail_count, pass_count + fail_count);
    $finish;
  end
endmodule
"""

tb_file = os.path.join(workdir, 'tb.sv')
open(tb_file, 'w').write(tb)

# Compile
r = subprocess.run(
    ['iverilog', '-o', os.path.join(workdir, 'sim.vvp'), '-s', 'tb', '-g2012',
     '-gsupported-assertions', sv_file, tb_file],
    capture_output=True, text=True, timeout=30
)
if r.returncode != 0:
    print(f"COMPILE FAIL: {r.stderr[:500]}")
    sys.exit(1)

# Run
r = subprocess.run(['vvp', os.path.join(workdir, 'sim.vvp')], capture_output=True, text=True, timeout=30)
print(r.stdout)
if 'ALL PASS' in r.stdout:
    sys.exit(0)
sys.exit(1)
