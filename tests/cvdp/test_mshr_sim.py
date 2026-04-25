#!/usr/bin/env python3
"""Full test of cache_mshr matching cocotb test behavior."""
import subprocess, os, sys, tempfile, shutil

sv_file = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'cache_mshr.sv')
workdir = tempfile.mkdtemp(prefix='mshr_sim_')

tb = """
`timescale 1ns/1ns
module tb;
  parameter MSHR_SIZE = 8;
  localparam MSHR_ADDR_WIDTH = $clog2(MSHR_SIZE);
  localparam CS_LINE_ADDR_WIDTH = 10;
  localparam TAG_WIDTH = 32 - (CS_LINE_ADDR_WIDTH + $clog2(4) + 4);
  localparam CS_WORD_WIDTH = 32;
  localparam DATA_WIDTH = 4 + 4 + CS_WORD_WIDTH + TAG_WIDTH;

  reg clk = 0;
  reg reset_r = 0;
  reg allocate_valid = 0;
  reg [CS_LINE_ADDR_WIDTH-1:0] allocate_addr = 0;
  reg allocate_rw = 0;
  reg [DATA_WIDTH-1:0] allocate_data = 0;
  wire [MSHR_ADDR_WIDTH-1:0] allocate_id;
  wire allocate_pending;
  wire [MSHR_ADDR_WIDTH-1:0] allocate_previd;
  wire allocate_ready;
  reg finalize_valid = 0;
  reg [MSHR_ADDR_WIDTH-1:0] finalize_id = 0;

  cache_mshr #(.MSHR_SIZE(MSHR_SIZE)) dut (
    .clk(clk), .reset(reset_r),
    .allocate_valid(allocate_valid), .allocate_addr(allocate_addr),
    .allocate_rw(allocate_rw), .allocate_data(allocate_data),
    .allocate_id(allocate_id), .allocate_pending(allocate_pending),
    .allocate_previd(allocate_previd), .allocate_ready(allocate_ready),
    .finalize_valid(finalize_valid), .finalize_id(finalize_id)
  );

  always #5 clk = ~clk;

  integer cycles_to_full, i;
  integer pass_count = 0;
  integer fail_count = 0;
  reg [MSHR_ADDR_WIDTH-1:0] allocated_id_saved;
  reg [CS_LINE_ADDR_WIDTH-1:0] addr;

  task do_reset;
    begin
      reset_r = 0; #10;
      reset_r = 1; #10;
      reset_r = 0; #10;
      @(posedge clk); #1;
      @(posedge clk); #1;
    end
  endtask

  initial begin
    do_reset;

    // Check after reset
    if (allocate_id !== 0) begin $display("FAIL: reset id=%0d", allocate_id); fail_count=fail_count+1; end
    if (allocate_ready !== 1) begin $display("FAIL: reset ready=%0d", allocate_ready); fail_count=fail_count+1; end

    // Test 1: Fill
    @(negedge clk);
    allocate_valid = 1;
    cycles_to_full = 0;
    while (allocate_ready == 1) begin
      @(posedge clk);
      cycles_to_full = cycles_to_full + 1;
      @(negedge clk);
    end
    allocate_valid = 0;
    if (cycles_to_full == MSHR_SIZE) begin pass_count=pass_count+1; $display("Test1: PASS"); end
    else begin fail_count=fail_count+1; $display("FAIL T1: %0d!=%0d", cycles_to_full, MSHR_SIZE); end

    do_reset;

    // Test 2: Linked list
    @(negedge clk);
    allocate_valid = 1;
    allocate_addr = 10'h44;
    finalize_valid = 0;
    for (i = 0; i < MSHR_SIZE; i = i + 1) begin
      allocate_rw = $random % 2;
      allocate_data = $random;
      @(negedge clk);
      if (allocate_id !== i[MSHR_ADDR_WIDTH-1:0]) begin
        $display("FAIL T2 i=%0d: id=%0d", i, allocate_id); fail_count=fail_count+1;
      end
      if (i > 0) begin
        if (allocate_pending !== 1) begin $display("FAIL T2 i=%0d: pending=0", i); fail_count=fail_count+1; end
        if (allocate_previd !== i[MSHR_ADDR_WIDTH-1:0]-1) begin
          $display("FAIL T2 i=%0d: previd=%0d", i, allocate_previd); fail_count=fail_count+1;
        end
      end
    end
    allocate_valid = 0;
    $display("Test2: checked all entries");

    do_reset;

    // Test 3: Allocate then finalize, reuse
    @(negedge clk);
    allocate_valid = 1;
    addr = 10'h55;
    allocate_addr = addr;
    allocate_rw = 0;
    allocate_data = 0;

    @(negedge clk);
    allocate_valid = 0;
    allocated_id_saved = allocate_id;
    finalize_valid = 1;
    finalize_id = allocated_id_saved;
    @(negedge clk);
    finalize_valid = 0;
    allocate_valid = 0;
    @(negedge clk);
    allocate_valid = 1;
    allocate_addr = 10'h66;
    @(negedge clk);
    allocate_valid = 0;
    if (allocate_id == allocated_id_saved) begin pass_count=pass_count+1; $display("Test3: PASS"); end
    else begin fail_count=fail_count+1; $display("FAIL T3: %0d!=%0d", allocate_id, allocated_id_saved); end

    do_reset;

    // Test 4: Simultaneous allocate + finalize
    @(negedge clk);
    allocate_valid = 1;
    addr = 10'h77;
    allocate_addr = addr;
    allocate_rw = 0;
    allocate_data = 0;

    @(negedge clk);
    allocate_valid = 1;
    allocate_addr = addr % 4;
    allocate_rw = 0;
    allocate_data = 0;
    allocated_id_saved = allocate_id;
    finalize_valid = 1;
    finalize_id = allocated_id_saved;

    @(negedge clk);
    allocate_valid = 0;
    finalize_valid = 0;
    if (allocate_id == 1) begin pass_count=pass_count+1; $display("Test4: PASS"); end
    else begin fail_count=fail_count+1; $display("FAIL T4: id=%0d expected=1", allocate_id); end

    #20;
    $display("Results: %0d pass, %0d fail", pass_count, fail_count);
    if (fail_count > 0) $display("OVERALL: FAIL");
    else $display("OVERALL: PASS");
    $finish;
  end
endmodule
"""

tb_file = os.path.join(workdir, 'tb.sv')
with open(tb_file, 'w') as f:
    f.write(tb)

all_pass = True
for mshr_size in [4, 8, 12, 16, 20, 24, 28, 32]:
    r = subprocess.run(
        ['iverilog', '-o', os.path.join(workdir, 'sim.vvp'), '-s', 'tb', '-g2012',
         '-gsupported-assertions',
         f'-Ptb.MSHR_SIZE={mshr_size}', sv_file, tb_file],
        capture_output=True, text=True, timeout=30
    )
    if r.returncode != 0:
        print(f"MSHR_SIZE={mshr_size}: COMPILE FAIL: {r.stderr[:200]}")
        all_pass = False
        continue

    r2 = subprocess.run(
        ['vvp', os.path.join(workdir, 'sim.vvp')],
        capture_output=True, text=True, timeout=30
    )
    if 'OVERALL: PASS' in r2.stdout:
        print(f"MSHR_SIZE={mshr_size}: PASS")
    else:
        print(f"MSHR_SIZE={mshr_size}: FAIL")
        for line in r2.stdout.split('\n'):
            if 'FAIL' in line:
                print(f"  {line}")
        all_pass = False

print(f"\n{'ALL PASS' if all_pass else 'SOME FAILURES'}")
shutil.rmtree(workdir)
