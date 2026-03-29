#!/usr/bin/env python3
"""Minimal test to check if cache_mshr.sv compiles and simulates."""
import subprocess, os, sys

sv_file = os.path.join(os.path.dirname(__file__), 'cache_mshr.sv')

# Test 1: compile with default params
r = subprocess.run(
    ['iverilog', '-o', '/tmp/mshr_test.vvp', '-s', 'cache_mshr', '-g2012', sv_file],
    capture_output=True, text=True, timeout=30
)
print(f"Compile (default params): rc={r.returncode}")
if r.stdout: print("STDOUT:", r.stdout[:500])
if r.stderr: print("STDERR:", r.stderr[:1000])

# Test 2: compile with MSHR_SIZE=4
r2 = subprocess.run(
    ['iverilog', '-o', '/tmp/mshr_test2.vvp', '-s', 'cache_mshr', '-g2012',
     '-Pcache_mshr.MSHR_SIZE=4', sv_file],
    capture_output=True, text=True, timeout=30
)
print(f"\nCompile (MSHR_SIZE=4): rc={r2.returncode}")
if r2.stdout: print("STDOUT:", r2.stdout[:500])
if r2.stderr: print("STDERR:", r2.stderr[:1000])

# Test 3: compile with MSHR_SIZE=28
r3 = subprocess.run(
    ['iverilog', '-o', '/tmp/mshr_test3.vvp', '-s', 'cache_mshr', '-g2012',
     '-Pcache_mshr.MSHR_SIZE=28', sv_file],
    capture_output=True, text=True, timeout=30
)
print(f"\nCompile (MSHR_SIZE=28): rc={r3.returncode}")
if r3.stdout: print("STDOUT:", r3.stdout[:500])
if r3.stderr: print("STDERR:", r3.stderr[:1000])
