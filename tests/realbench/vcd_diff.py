#!/usr/bin/env python3
"""VCD waveform diff tool — shows ref vs DUT signal values around first mismatch."""

import sys, re
from collections import defaultdict

def parse_vcd(path, max_time=None):
    """Parse VCD file, return (signals, changes).
    signals: {code: (scope, name, width)}
    changes: {code: [(time, value), ...]}
    """
    signals = {}
    changes = defaultdict(list)
    scope_stack = []
    in_defs = False
    current_time = 0

    with open(path) as f:
        for line in f:
            line = line.strip()
            if line.startswith('$scope'):
                parts = line.split()
                if len(parts) >= 3:
                    scope_stack.append(parts[2])
            elif line.startswith('$upscope'):
                if scope_stack:
                    scope_stack.pop()
            elif line.startswith('$var'):
                parts = line.split()
                if len(parts) >= 5:
                    width = int(parts[2])
                    code = parts[3]
                    name = parts[4]
                    scope = '.'.join(scope_stack)
                    signals[code] = (scope, name, width)
            elif line.startswith('#'):
                current_time = int(line[1:])
                if max_time and current_time > max_time:
                    break
            elif line and not line.startswith('$'):
                if line[0] in '01xzXZ':
                    code = line[1:]
                    val = line[0]
                    changes[code].append((current_time, val))
                elif line[0] == 'b' or line[0] == 'B':
                    parts = line.split()
                    if len(parts) >= 2:
                        val = parts[0][1:]
                        code = parts[1]
                        changes[code].append((current_time, val))

    return signals, changes

def find_ref_dut_pairs(signals):
    """Find matching ref/dut signal pairs."""
    by_name = defaultdict(list)
    for code, (scope, name, width) in signals.items():
        by_name[name].append((code, scope, width))

    pairs = []
    seen = set()
    for name, entries in by_name.items():
        if name.endswith('_ref') or name.endswith('_dut'):
            base = name[:-4]
            if base in seen:
                continue
            ref_code = dut_code = None
            ref_w = dut_w = 0
            for code, scope, width in by_name.get(base + '_ref', []):
                ref_code = code
                ref_w = width
            for code, scope, width in by_name.get(base + '_dut', []):
                dut_code = code
                dut_w = width
            if ref_code and dut_code:
                pairs.append((base, ref_code, dut_code, ref_w))
                seen.add(base)
    return pairs

def get_value_at(changes, code, time):
    """Get signal value at a given time."""
    val = 'x'
    for t, v in changes.get(code, []):
        if t > time:
            break
        val = v
    return val

def main():
    if len(sys.argv) < 2:
        print("Usage: python3 vcd_diff.py <wave.vcd> [max_time]")
        sys.exit(1)

    vcd_path = sys.argv[1]
    max_time = int(sys.argv[2]) if len(sys.argv) > 2 else None

    print(f"Parsing {vcd_path}...")
    signals, changes = parse_vcd(vcd_path, max_time)
    print(f"  {len(signals)} signals, {sum(len(v) for v in changes.values())} value changes")

    pairs = find_ref_dut_pairs(signals)
    if not pairs:
        print("  No ref/dut signal pairs found.")
        # Try alternate naming: look for signals in different scopes
        print("  Available signal names (sample):")
        for i, (code, (scope, name, width)) in enumerate(signals.items()):
            if i < 30:
                print(f"    {scope}.{name} [{width}]")
        sys.exit(0)

    print(f"  {len(pairs)} ref/dut pairs found")
    print()

    # Find first mismatch for each pair
    mismatches = []
    for base, ref_code, dut_code, width in pairs:
        # Build time-indexed values
        all_times = sorted(set(
            [t for t, _ in changes.get(ref_code, [])] +
            [t for t, _ in changes.get(dut_code, [])]
        ))
        for t in all_times:
            rv = get_value_at(changes, ref_code, t)
            dv = get_value_at(changes, dut_code, t)
            if rv != dv:
                mismatches.append((t, base, rv, dv, width))
                break

    if not mismatches:
        print("No mismatches found in VCD!")
        sys.exit(0)

    mismatches.sort()
    print(f"First mismatches ({len(mismatches)} signals diverge):")
    print(f"{'Time':>10}  {'Signal':30}  {'Ref':>20}  {'DUT':>20}")
    print("-" * 85)
    for t, name, rv, dv, w in mismatches[:20]:
        print(f"{t:>10}  {name:30}  {rv:>20}  {dv:>20}")

    if mismatches:
        first_t = mismatches[0][0]
        print(f"\n--- All signal values at first mismatch time t={first_t} ---")
        print(f"{'Signal':30}  {'Ref':>20}  {'DUT':>20}  {'Match':>6}")
        print("-" * 85)
        for base, ref_code, dut_code, width in sorted(pairs, key=lambda x: x[0]):
            rv = get_value_at(changes, ref_code, first_t)
            dv = get_value_at(changes, dut_code, first_t)
            match = '✓' if rv == dv else '✗'
            print(f"{base:30}  {rv:>20}  {dv:>20}  {match:>6}")

if __name__ == '__main__':
    main()
