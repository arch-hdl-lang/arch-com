#!/usr/bin/env python3
"""Check if icarus $clog2 works with -P override."""
import subprocess, tempfile, os, shutil

workdir = tempfile.mkdtemp(prefix='mshr_param_')

tb = """
`timescale 1ns/1ns
module tb;
  parameter MSHR_SIZE = 32;
  localparam MSHR_ADDR_WIDTH = $clog2(MSHR_SIZE);

  initial begin
    $display("MSHR_SIZE=%0d MSHR_ADDR_WIDTH=%0d", MSHR_SIZE, MSHR_ADDR_WIDTH);
    $finish;
  end
endmodule
"""

sv_file = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'cache_mshr.sv')
tb_file = os.path.join(workdir, 'tb.sv')
with open(tb_file, 'w') as f:
    f.write(tb)

for size in [4, 8, 12, 16, 20, 24, 28, 32]:
    r = subprocess.run(
        ['iverilog', '-o', os.path.join(workdir, 'sim.vvp'), '-s', 'tb', '-g2012',
         f'-Ptb.MSHR_SIZE={size}', tb_file],
        capture_output=True, text=True, timeout=10
    )
    if r.returncode == 0:
        r2 = subprocess.run(['vvp', os.path.join(workdir, 'sim.vvp')],
                           capture_output=True, text=True, timeout=10)
        print(r2.stdout.strip())
    else:
        print(f"MSHR_SIZE={size}: compile error: {r.stderr[:200]}")

# Also check parameter int version
print("\n--- With parameter int ---")
tb2 = """
`timescale 1ns/1ns
module tb;
  parameter int MSHR_SIZE = 32;
  parameter int MSHR_ADDR_WIDTH = $clog2(MSHR_SIZE);

  initial begin
    $display("MSHR_SIZE=%0d MSHR_ADDR_WIDTH=%0d", MSHR_SIZE, MSHR_ADDR_WIDTH);
    $finish;
  end
endmodule
"""
with open(tb_file, 'w') as f:
    f.write(tb2)

for size in [4, 8, 12, 16, 20, 24, 28, 32]:
    r = subprocess.run(
        ['iverilog', '-o', os.path.join(workdir, 'sim.vvp'), '-s', 'tb', '-g2012',
         f'-Ptb.MSHR_SIZE={size}', tb_file],
        capture_output=True, text=True, timeout=10
    )
    if r.returncode == 0:
        r2 = subprocess.run(['vvp', os.path.join(workdir, 'sim.vvp')],
                           capture_output=True, text=True, timeout=10)
        print(r2.stdout.strip())
    else:
        print(f"MSHR_SIZE={size}: compile error: {r.stderr[:200]}")

shutil.rmtree(workdir)
