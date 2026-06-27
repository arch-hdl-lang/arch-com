#!/usr/bin/env python3
"""Hoist function-local declarations so yosys's Verilog frontend accepts the
arch-emitted FP helper library.

`arch build` emits each FP operator as a library of SystemVerilog
`function automatic`s whose bodies are pure SSA:

    function automatic logic [31:0] arch_f32_mul(input logic [31:0] a, ...);
      logic [7:0] _t0 = a[30:23];      // decl-with-initializer
      ...
      arch_f32_mul = _t330;            // return assignment
    endfunction

Verilator/iverilog accept decl-with-initializer inside a function, but yosys
0.x's built-in Verilog parser rejects it ("Invalid nesting of always blocks
and/or initializations"). This pass rewrites each function body to put all bare
declarations first, then the initializers as ordinary blocking assignments in
their original order — semantically identical, and yosys-friendly. The helper
bodies contain no control flow (verified: no for/if/case/begin), so a flat
hoist is sound.

Usage:  hoist_decls.py < in.sv > out.v
"""
import re
import sys

_decl_init = re.compile(r'^(\s*)logic(\s*(\[[^\]]*\])?\s*)([A-Za-z_]\w*)\s*=\s*(.*);\s*$')
_decl_bare = re.compile(r'^(\s*)logic(\s*(\[[^\]]*\])?\s*)([A-Za-z_]\w*)\s*;\s*$')


def convert(text: str) -> str:
    lines = text.splitlines()
    out, i, n = [], 0, len(lines)
    while i < n:
        line = lines[i]
        if line.lstrip().startswith("function automatic"):
            out.append(line)
            i += 1
            decls, stmts = [], []
            while i < n and lines[i].strip() != "endfunction":
                b = lines[i]
                m = _decl_init.match(b)
                if m:
                    ind, _, wid, name, rhs = m.groups()
                    wid = (wid + " ") if wid else ""
                    decls.append(f"{ind}logic {wid}{name};")
                    stmts.append(f"{ind}{name} = {rhs};")
                elif _decl_bare.match(b):
                    decls.append(b)
                else:
                    stmts.append(b)
                i += 1
            out.extend(decls)
            out.extend(stmts)
            out.append("endfunction")
            i += 1  # skip the endfunction line
        else:
            out.append(line)
            i += 1
    return "\n".join(out) + "\n"


if __name__ == "__main__":
    sys.stdout.write(convert(sys.stdin.read()))
