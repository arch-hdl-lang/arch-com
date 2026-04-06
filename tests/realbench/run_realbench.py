#!/usr/bin/env python3
"""RealBench integration test runner for ARCH-generated SV modules."""

import os, sys, subprocess, tempfile, shutil, json, glob, re, argparse

ARCH_DIR = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
REALBENCH_DIR = os.path.join(ARCH_DIR, '..', 'RealBench')
ARCH_BIN = os.path.join(ARCH_DIR, 'target', 'release', 'arch')
DEBUG_DIR = os.path.join(ARCH_DIR, 'tests', 'realbench', 'debug')

VFLAGS_BASE = [
    '-cc', '--exe', '--binary', '--trace', '--assert', '--timing',
    '-j', '4', '-fno-table',
    '-Wno-SIDEEFFECT', '-Wno-CASEOVERLAP', '-Wno-LATCH', '-Wno-UNOPTFLAT',
    '-Wno-MULTIDRIVEN', '-Wno-ASCRANGE', '-Wno-COMBDLY', '-Wno-IMPLICIT',
    '-Wno-CASEINCOMPLETE', '-Wno-PINMISSING', '-Wno-WIDTHTRUNC',
    '-Wno-TIMESCALEMOD', '-Wno-INITIALDLY', '-Wno-EOFNEWLINE',
    '-Wno-DECLFILENAME', '-Wno-WIDTHEXPAND', '-Wno-WIDTHCONCAT',
]

PROBLEM_SETS = {
    'e203': ('e203_hbirdv2', 'tests/e203'),
    'sdc':  ('sdc',          'tests/sdc'),
    'aes':  ('aes',          'tests/aes'),
}

def detect_tb_top(work_dir, mod):
    """Detect testbench top module name from the testbench file."""
    tb_file = os.path.join(work_dir, f'{mod}_testbench.sv')
    if os.path.isfile(tb_file):
        with open(tb_file) as f:
            for line in f:
                m = re.match(r'\s*module\s+(\w+)', line)
                if m:
                    return m.group(1)
    return 'tb'

def get_modules(bench_dir, filter_mod=None):
    """Get list of modules with verification directories."""
    mods = []
    for d in sorted(os.listdir(bench_dir)):
        verif = os.path.join(bench_dir, d, 'verification')
        if os.path.isdir(verif):
            if filter_mod and d != filter_mod:
                continue
            mods.append(d)
    return mods

def run_one(mod, bench_dir, arch_src_dir, debug=False):
    """Run one RealBench integration test. Returns (status, detail, sim_output)."""
    verif_dir = os.path.join(bench_dir, mod, 'verification')
    arch_file = os.path.join(arch_src_dir, f'{mod}.arch')

    if not os.path.isfile(arch_file):
        return 'SKIP', 'no .arch file', ''

    work = tempfile.mkdtemp()
    try:
        # Copy verification files
        for f in os.listdir(verif_dir):
            src = os.path.join(verif_dir, f)
            if os.path.isfile(src):
                shutil.copy2(src, work)

        # Use pre-generated .sv (from arch build during implementation)
        gen_sv = os.path.join(arch_src_dir, f'{mod}.sv')
        top_sv = os.path.join(work, f'{mod}_top.sv')
        if not os.path.isfile(gen_sv):
            return 'FAIL', 'no .sv file (run arch build first)', ''
        shutil.copy2(gen_sv, top_sv)

        # Copy generated .sv files for submodule dependencies.
        for sv_file in glob.glob(os.path.join(arch_src_dir, '*.sv')):
            bn = os.path.basename(sv_file)
            mod_name = bn.replace('.sv', '')
            if mod_name == mod:
                continue
            dest = os.path.join(work, bn)
            if not os.path.exists(dest):
                shutil.copy2(sv_file, dest)
            v_path = os.path.join(work, mod_name + '.v')
            if os.path.exists(v_path):
                os.remove(v_path)

        # Detect testbench top module name
        tb_top = detect_tb_top(work, mod)
        vflags = VFLAGS_BASE + ['--top', tb_top]

        # Verilator compile
        src_files = glob.glob(os.path.join(work, '*.v')) + glob.glob(os.path.join(work, '*.sv'))
        r = subprocess.run(['verilator'] + vflags + src_files,
                          capture_output=True, text=True, cwd=work, timeout=120)
        if r.returncode != 0:
            errs = [l for l in r.stderr.split('\n') if '%Error' in l][:3]
            return 'FAIL', f'verilator: {"; ".join(errs)}', r.stderr

        # Run simulation
        vtb = os.path.join(work, 'obj_dir', f'V{tb_top}')
        if not os.path.isfile(vtb):
            return 'FAIL', f'no V{tb_top} binary', ''

        r = subprocess.run([vtb], capture_output=True, text=True, cwd=work, timeout=30)
        out = r.stdout + r.stderr

        # In debug mode, save artifacts
        if debug:
            mod_debug = os.path.join(DEBUG_DIR, mod)
            os.makedirs(mod_debug, exist_ok=True)
            # Save VCD
            vcd = os.path.join(work, 'wave.vcd')
            if os.path.isfile(vcd):
                shutil.copy2(vcd, os.path.join(mod_debug, 'wave.vcd'))
            # Save sim output
            with open(os.path.join(mod_debug, 'sim_output.txt'), 'w') as f:
                f.write(out)
            # Save work dir path
            with open(os.path.join(mod_debug, 'work_dir.txt'), 'w') as f:
                f.write(work + '\n')

        # Parse results
        if 'Total mismatched samples is 0' in out:
            m = re.search(r'(\d+) samples', out)
            samples = m.group(1) if m else '?'
            return 'PASS', f'{samples} samples', out
        elif 'Mismatches: 0' in out:
            m = re.search(r'(\d+) samples', out)
            samples = m.group(1) if m else '?'
            return 'PASS', f'{samples} samples', out
        elif 'mismatched' in out.lower() or 'Mismatches:' in out:
            m = re.search(r'(\d+)\s+mismatched', out) or re.search(r'Mismatches:\s*(\d+)', out)
            n = m.group(1) if m else '?'
            return 'FAIL', f'{n} mismatches', out
        elif r.returncode == 0:
            return 'PASS', 'completed', out
        else:
            return 'FAIL', f'exit code {r.returncode}', out

    except subprocess.TimeoutExpired:
        return 'FAIL', 'timeout', ''
    except Exception as e:
        return 'FAIL', str(e)[:100], ''
    finally:
        if not debug:
            shutil.rmtree(work, ignore_errors=True)

def main():
    parser = argparse.ArgumentParser(description='RealBench integration test runner')
    parser.add_argument('pset', nargs='?', default='e203', help='Problem set: e203, sdc, aes')
    parser.add_argument('module', nargs='?', default=None, help='Single module to test')
    parser.add_argument('--debug', action='store_true', help='Save VCD and sim output to debug/')
    args = parser.parse_args()

    if args.pset not in PROBLEM_SETS:
        print(f"Unknown: {args.pset}. Use: {', '.join(PROBLEM_SETS.keys())}")
        sys.exit(1)

    bench_name, arch_rel = PROBLEM_SETS[args.pset]
    bench_dir = os.path.join(REALBENCH_DIR, bench_name)
    arch_src_dir = os.path.join(ARCH_DIR, arch_rel)

    modules = get_modules(bench_dir, args.module)
    total = len(modules)

    print(f"Running {total} RealBench {args.pset} integration tests...")
    if args.debug:
        print(f"Debug mode: artifacts saved to {DEBUG_DIR}/")
    print("=" * 65)

    results = {'PASS': 0, 'FAIL': 0, 'SKIP': 0}
    failures = []

    for mod in modules:
        status, detail, sim_out = run_one(mod, bench_dir, arch_src_dir, debug=args.debug)
        results[status] = results.get(status, 0) + 1
        marker = {'PASS': '✓', 'FAIL': '✗', 'SKIP': '-'}[status]
        print(f"  {marker} {mod:45s} {status} ({detail})")

        # In debug mode for single module, print per-signal details
        if args.debug and args.module and status == 'FAIL' and sim_out:
            print()
            for line in sim_out.split('\n'):
                if 'mismatch' in line.lower() or 'Hint:' in line or 'Total' in line:
                    print(f"    {line.strip()}")
            print()

        if status == 'FAIL':
            failures.append((mod, detail))

    print("=" * 65)
    print(f"Results: {results['PASS']} PASS, {results['FAIL']} FAIL, {results['SKIP']} SKIP (of {total})")

    if failures:
        print("\nFailures:")
        for mod, detail in failures:
            print(f"  {mod}: {detail}")

    sys.exit(0 if results['FAIL'] == 0 else 1)

if __name__ == '__main__':
    main()
