#!/usr/bin/env python3
"""Structural pipeline-balance / uniform-latency check (Lemma B of the staged-op
equivalence proof — see tests/fp_v1/smt_proof/staged_ops_miter.sh).

A retimed staged operator (`op<pipelined, N>`, emitted by `arch build
--staged-ops`) must be a *balanced feed-forward* pipeline: every path from a
primary data input to the output crosses the SAME number of pipeline registers.
If one bit reaches the output through five registers and another through four,
the pipeline is skewed and samples a stale/early input on the short path — a
silent miscompile invisible to stable-input testing (this is exactly the class
of the scaled_dot scale-byte off-by-one bug, arch#955 follow-up). Balance is a
purely *structural* fact, so it needs no solver: we levelize the gate netlist
and check that the min and max register-depth from any input to every output bit
coincide at a single value L.

Together with the combinational arithmetic miter (Lemma A), balance at latency L
gives the full obligation `output[t+L] == op(input[t])` for all inputs: Lemma A
proves the register-shorted netlist computes `op`, and Lemma B proves the
pipeline delivers that function at a uniform L-cycle delay with no cross-talk.

Input: a Yosys `write_json` netlist (after `proc; flatten; opt_clean`) and the
top module name. Clock/reset ports are excluded from the data-input set, and for
flip-flop cells only the `D` data port is traversed (CLK/EN/reset/set are
control, not combinational-depth contributors — counting the clock net as an
input is the classic false "depth-1 leak" artifact).

Exit status: 0 if balanced to a single latency, 1 otherwise. Prints the latency.

Usage:
    yosys -p 'read_verilog -sv m.v; hierarchy -top M; proc; flatten; opt_clean; \
              write_json m.json'
    python3 pipeline_balance.py m.json M [--expect N]
"""
import json
import sys
import collections

CONTROL_PORTS = {"CLK", "C", "EN", "E", "R", "S", "SET", "CLR", "ARST", "SRST",
                 "ALOAD", "AD"}
CLOCKISH = {"clk", "clock", "rst", "reset", "rst_n", "resetn", "arst", "srst"}


def is_ff(cell_type: str) -> bool:
    t = cell_type.lower()
    return "dff" in t or t.endswith("_ff_") or "dffe" in t or "sdff" in t


def analyze(json_path: str, top: str):
    j = json.load(open(json_path))
    if top not in j["modules"]:
        raise SystemExit(f"module {top!r} not in {json_path}")
    mod = j["modules"][top]

    inbits, clkbits = set(), set()
    outbits = []
    for pname, pd in mod["ports"].items():
        bits = [b for b in pd["bits"] if isinstance(b, int)]
        if pd["direction"] == "input":
            (clkbits if pname in CLOCKISH else inbits).update(bits)
        elif pd["direction"] == "output":
            outbits += bits

    drv = {}                                    # net bit -> driving cell
    cin = collections.defaultdict(list)         # cell -> data-input bits
    cff = {}                                     # cell -> is flip-flop
    for cn, cd in mod["cells"].items():
        ff = is_ff(cd["type"])
        cff[cn] = ff
        pdirs = cd.get("port_directions", {})
        for port, bits in cd["connections"].items():
            bits = [b for b in bits if isinstance(b, int)]
            dirn = pdirs.get(port)
            if dirn == "output" or (dirn is None and port in ("Y", "Q")):
                for b in bits:
                    drv[b] = cn
                continue
            if ff and port != "D":
                continue                        # FF control ports: skip
            if port in CONTROL_PORTS:
                continue
            cin[cn] += bits

    sys.setrecursionlimit(1_000_000)

    def depth(bit, agg, memo, seen):
        if bit in inbits:
            return 0
        if bit in clkbits or bit not in drv:
            return None                          # constant / clock / undriven
        cn = drv[bit]
        if cn in memo:
            return memo[cn]
        if cn in seen:
            return None                          # combinational cycle guard
        seen = seen | {cn}
        vals = []
        for ib in cin[cn]:
            d = depth(ib, agg, memo, seen)
            if d is not None:
                vals.append(d + (1 if cff[cn] else 0))
        r = agg(vals) if vals else None
        memo[cn] = r
        return r

    maxd = [depth(b, max, {}, set()) for b in outbits]
    mind = [depth(b, min, {}, set()) for b in outbits]
    levels = {x for x in maxd + mind if x is not None}
    reachable = sum(1 for x in maxd if x is not None)
    return outbits, reachable, sorted(levels), maxd, mind


def main(argv):
    if len(argv) < 3:
        raise SystemExit(__doc__)
    json_path, top = argv[1], argv[2]
    expect = None
    if "--expect" in argv:
        expect = int(argv[argv.index("--expect") + 1])

    outbits, reachable, levels, maxd, mind = analyze(json_path, top)
    print(f"# {top}: {len(outbits)} output bits, "
          f"{reachable} reachable from a data input")
    print(f"#   max FF-depth (input->out): {sorted(set(x for x in maxd if x is not None))}")
    print(f"#   min FF-depth (input->out): {sorted(set(x for x in mind if x is not None))}")

    if len(levels) != 1:
        print(f"UNBALANCED: multiple register-latencies present {levels} "
              f"— pipeline is skewed (stale/early input on the short path)")
        return 1
    L = levels[0]
    if expect is not None and L != expect:
        print(f"BALANCED at latency {L}, but expected {expect}")
        return 1
    print(f"BALANCED: uniform pipeline latency = {L}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
