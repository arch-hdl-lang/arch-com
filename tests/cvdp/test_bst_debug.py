#!/usr/bin/env python3
"""Standalone test for binary_search_tree_sort using cocotb runner."""
import os
import sys
import random

# Set env vars
os.environ['SIM'] = 'icarus'
os.environ['TOPLEVEL_LANG'] = 'verilog'

from cocotb_tools.runner import get_runner

sv_path = os.path.abspath(os.path.join(os.path.dirname(__file__), 'binary_search_tree_sort_manual.sv'))

DATA_WIDTH = 6
ARRAY_SIZE = 4

runner = get_runner('icarus')
runner.build(
    sources=[sv_path],
    hdl_toplevel='binary_search_tree_sort',
    parameters={'DATA_WIDTH': DATA_WIDTH, 'ARRAY_SIZE': ARRAY_SIZE},
    always=True,
    clean=True,
    waves=True,
    timescale=("1ns", "1ns"),
)

# Use inline test module
runner.test(
    hdl_toplevel='binary_search_tree_sort',
    test_module='test_bst_inline',
)
