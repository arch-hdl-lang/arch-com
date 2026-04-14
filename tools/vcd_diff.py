#!/usr/bin/env python3
"""vcd_diff — Compare two VCD waveform files and report signal divergences.

Usage:
    vcd_diff.py reference.vcd actual.vcd [--signals SIG1,SIG2,...] [--from T] [--to T] [--context N]

Compares two VCD files cycle-by-cycle and reports the first divergence per signal,
plus optional context (N cycles before/after). Useful for debugging timing mismatches
between arch sim and Icarus/Verilator.

Examples:
    # Compare all signals
    vcd_diff.py arch_sim.vcd icarus.vcd

    # Compare specific signals only
    vcd_diff.py arch_sim.vcd icarus.vcd --signals o_heat_water,state_ff,o_error

    # Show 3 cycles of context around each divergence
    vcd_diff.py arch_sim.vcd icarus.vcd --context 3

    # Limit to time range 80ns-200ns
    vcd_diff.py arch_sim.vcd icarus.vcd --from 80 --to 200
"""

import argparse
import sys
from collections import defaultdict


def parse_vcd(path):
    """Parse a VCD file into {signal_name: [(time, value), ...]}.

    Returns:
        timescale: str (e.g. "1ns")
        signals: dict mapping signal name -> sorted list of (time_int, value_str)
        id_to_name: dict mapping VCD short id -> signal name
    """
    signals = defaultdict(list)
    id_to_name = {}
    scope_stack = []
    timescale = "1ns"
    current_time = 0
    in_defs = True

    with open(path) as f:
        for line in f:
            line = line.strip()
            if not line:
                continue

            if line.startswith("$timescale"):
                # May be on same line or next
                parts = line.split()
                if len(parts) >= 2 and parts[1] != "$end":
                    timescale = parts[1]
                continue

            if line.startswith("$scope"):
                parts = line.split()
                if len(parts) >= 3:
                    scope_stack.append(parts[2])
                continue

            if line.startswith("$upscope"):
                if scope_stack:
                    scope_stack.pop()
                continue

            if line.startswith("$var"):
                parts = line.split()
                # $var wire WIDTH ID NAME $end
                if len(parts) >= 5:
                    var_id = parts[3]
                    var_name = parts[4]
                    # Build hierarchical name
                    if scope_stack:
                        full_name = ".".join(scope_stack) + "." + var_name
                    else:
                        full_name = var_name
                    id_to_name[var_id] = full_name
                continue

            if line.startswith("$enddefinitions"):
                in_defs = False
                continue

            if line.startswith("$"):
                continue

            if in_defs:
                continue

            # Time stamp
            if line.startswith("#"):
                try:
                    current_time = int(line[1:])
                except ValueError:
                    pass
                continue

            # Value change: single-bit "0ID" or "1ID" or multi-bit "bVALUE ID"
            if line.startswith("b") or line.startswith("B"):
                parts = line.split()
                if len(parts) >= 2:
                    val = parts[0][1:]  # strip 'b'
                    var_id = parts[1]
                    if var_id in id_to_name:
                        signals[id_to_name[var_id]].append((current_time, val))
            elif len(line) >= 2 and line[0] in "01xXzZ":
                val = line[0]
                var_id = line[1:]
                if var_id in id_to_name:
                    signals[id_to_name[var_id]].append((current_time, val))

    return timescale, dict(signals), id_to_name


def normalize_value(val):
    """Normalize a VCD value string for comparison."""
    val = val.lower().strip()
    # Remove leading zeros from binary
    if all(c in "01" for c in val):
        val = val.lstrip("0") or "0"
    return val


def build_timeline(changes, t_from, t_to):
    """Build a dict {time: value} from a list of (time, value) changes."""
    timeline = {}
    current_val = "x"
    for t, v in sorted(changes):
        if t > t_to:
            break
        current_val = v
        if t >= t_from:
            timeline[t] = v
    # Fill initial value if first change is before t_from
    for t, v in sorted(changes):
        if t <= t_from:
            current_val = v
        else:
            break
    return timeline, current_val


def value_at_time(changes, target_time):
    """Get the signal value at a specific time."""
    val = "x"
    for t, v in sorted(changes):
        if t > target_time:
            break
        val = v
    return val


def main():
    parser = argparse.ArgumentParser(
        description="Compare two VCD files and report signal divergences.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__,
    )
    parser.add_argument("reference", help="Reference VCD file (e.g., arch sim output)")
    parser.add_argument("actual", help="Actual VCD file (e.g., Icarus output)")
    parser.add_argument("--signals", "-s", help="Comma-separated signal names to compare (default: all common)")
    parser.add_argument("--from", dest="t_from", type=int, default=0, help="Start time (default: 0)")
    parser.add_argument("--to", dest="t_to", type=int, default=None, help="End time (default: max)")
    parser.add_argument("--context", "-C", type=int, default=0, help="Cycles of context around divergences")
    parser.add_argument("--timescale", type=int, default=None, help="Override time unit in ns (auto-detected from VCD)")
    parser.add_argument("--max-diffs", type=int, default=20, help="Max divergences to report per signal (default: 20)")
    parser.add_argument("--summary", action="store_true", help="Only show summary, not per-signal details")

    args = parser.parse_args()

    # Parse both VCDs
    print(f"Parsing {args.reference}...", file=sys.stderr)
    ts_ref, sig_ref, _ = parse_vcd(args.reference)
    print(f"  {len(sig_ref)} signals", file=sys.stderr)

    print(f"Parsing {args.actual}...", file=sys.stderr)
    ts_act, sig_act, _ = parse_vcd(args.actual)
    print(f"  {len(sig_act)} signals", file=sys.stderr)

    # Find common signals
    ref_names = set(sig_ref.keys())
    act_names = set(sig_act.keys())

    # Also try matching by leaf name (ignore hierarchy prefix)
    ref_leaf = {n.rsplit(".", 1)[-1]: n for n in ref_names}
    act_leaf = {n.rsplit(".", 1)[-1]: n for n in act_names}

    if args.signals:
        requested = [s.strip() for s in args.signals.split(",")]
    else:
        # Match by leaf name
        common_leaves = set(ref_leaf.keys()) & set(act_leaf.keys())
        requested = sorted(common_leaves)

    # Build signal pairs (ref_full_name, act_full_name, display_name)
    pairs = []
    for name in requested:
        ref_full = ref_leaf.get(name, name) if name not in ref_names else name
        act_full = act_leaf.get(name, name) if name not in act_names else name
        if ref_full in sig_ref and act_full in sig_act:
            pairs.append((ref_full, act_full, name))

    if not pairs:
        print("No common signals found to compare.", file=sys.stderr)
        if ref_names and act_names:
            print(f"\nReference signals: {sorted(ref_names)[:10]}...", file=sys.stderr)
            print(f"Actual signals: {sorted(act_names)[:10]}...", file=sys.stderr)
        return 1

    # Determine time range
    all_times = set()
    for ref_full, act_full, _ in pairs:
        for t, _ in sig_ref[ref_full]:
            all_times.add(t)
        for t, _ in sig_act[act_full]:
            all_times.add(t)

    t_from = args.t_from
    t_to = args.t_to if args.t_to is not None else (max(all_times) if all_times else 0)

    print(f"\nComparing {len(pairs)} signals over [{t_from}, {t_to}]")
    print("=" * 70)

    total_diffs = 0
    diff_signals = []

    for ref_full, act_full, display_name in pairs:
        ref_changes = sig_ref[ref_full]
        act_changes = sig_act[act_full]

        # Collect all time points where either signal changes
        change_times = sorted(set(
            [t for t, _ in ref_changes if t_from <= t <= t_to] +
            [t for t, _ in act_changes if t_from <= t <= t_to]
        ))

        diffs = []
        for t in change_times:
            rv = normalize_value(value_at_time(ref_changes, t))
            av = normalize_value(value_at_time(act_changes, t))
            if rv != av:
                diffs.append((t, rv, av))

        if not diffs:
            continue

        total_diffs += len(diffs)
        diff_signals.append(display_name)

        if args.summary:
            continue

        print(f"\n  {display_name}: {len(diffs)} divergence(s)")
        print(f"  {'Time':>10}  {'Reference':>15}  {'Actual':>15}")
        print(f"  {'-'*10}  {'-'*15}  {'-'*15}")

        shown = 0
        for t, rv, av in diffs:
            if shown >= args.max_diffs:
                print(f"  ... ({len(diffs) - shown} more)")
                break

            # Context before
            if args.context > 0:
                context_times = sorted([
                    ct for ct in change_times
                    if ct < t and ct >= t - args.context * 10  # rough heuristic
                ])[-args.context:]
                for ct in context_times:
                    crv = normalize_value(value_at_time(ref_changes, ct))
                    cav = normalize_value(value_at_time(act_changes, ct))
                    marker = " " if crv == cav else "!"
                    print(f"  {ct:>10}  {crv:>15}  {cav:>15}  {marker}")

            # The divergence
            print(f"  {t:>10}  {rv:>15}  {av:>15}  <<<")

            # Context after
            if args.context > 0:
                context_times = sorted([
                    ct for ct in change_times
                    if ct > t and ct <= t + args.context * 10
                ])[:args.context]
                for ct in context_times:
                    crv = normalize_value(value_at_time(ref_changes, ct))
                    cav = normalize_value(value_at_time(act_changes, ct))
                    marker = " " if crv == cav else "!"
                    print(f"  {ct:>10}  {crv:>15}  {cav:>15}  {marker}")

            shown += 1

    # Summary
    print(f"\n{'=' * 70}")
    if total_diffs == 0:
        print(f"MATCH: All {len(pairs)} signals agree over [{t_from}, {t_to}]")
        return 0
    else:
        print(f"DIVERGE: {len(diff_signals)}/{len(pairs)} signals differ ({total_diffs} total divergences)")
        print(f"  Signals: {', '.join(diff_signals)}")
        return 1


if __name__ == "__main__":
    sys.exit(main())
