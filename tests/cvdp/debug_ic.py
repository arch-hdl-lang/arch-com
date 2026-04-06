#!/usr/bin/env python3
"""Debug script for interrupt_controller_apb - captures sim.log for analysis."""
import json, os, shutil, tempfile, subprocess, sys, re, glob

JSONL = os.path.expanduser("~/github/cvdp_benchmark/full_dataset/cvdp_v1.0.4_nonagentic_code_generation_no_commercial.jsonl")
CVDP_DIR = os.path.dirname(os.path.abspath(__file__))

def main():
    # Load problem
    with open(JSONL) as f:
        for line in f:
            d = json.loads(line)
            if 'interrupt_controller_0019' in d['id']:
                prob = d
                break

    module_name = 'interrupt_controller_apb'
    sv_file = os.path.join(CVDP_DIR, f"{module_name}.sv")
    wd = tempfile.mkdtemp(prefix=f'debug_{module_name}_')

    # Extract harness
    for fname, content in prob['harness']['files'].items():
        fpath = os.path.join(wd, fname)
        os.makedirs(os.path.dirname(fpath), exist_ok=True)
        open(fpath, 'w').write(content)

    rtl_dir = os.path.join(wd, 'rtl')
    os.makedirs(rtl_dir, exist_ok=True)
    shutil.copy(sv_file, os.path.join(rtl_dir, f"{module_name}.sv"))
    shutil.copy(sv_file, os.path.join(rtl_dir, f"{module_name}.v"))

    # Patch harness_library
    sv_src = open(sv_file).read()
    input_names = set(re.findall(r'input\s+(?:logic\s+)?(?:(?:signed|unsigned)\s+)?(?:\[[^\]]*\]\s*)?(\w+)', sv_src))
    hl_path = os.path.join(wd, 'src', 'harness_library.py')
    if os.path.exists(hl_path):
        hl = open(hl_path).read()
        hl = hl.replace('signal._type == "GPI_NET"', f'(signal._type == "GPI_NET" or signal._name in {repr(input_names)})')
        open(hl_path, 'w').write(hl)

    # Patch test files
    for pyfile in glob.glob(os.path.join(wd, 'src', '*.py')):
        content = open(pyfile).read()
        changed = False
        if 'cocotb.sim_time_utils' in content:
            content = content.replace('from cocotb.sim_time_utils import', 'from cocotb.utils import')
            changed = True
        if changed:
            open(pyfile, 'w').write(content)

    # Patch test_runner.py
    tr_path = os.path.join(wd, 'src', 'test_runner.py')
    tr = open(tr_path).read()
    tr = tr.replace('from cocotb.runner import', 'from cocotb_tools.runner import')
    tr = re.sub(r'\n*#?\s*if __name__\s*==.*', '', tr, flags=re.DOTALL)
    tr = tr.rstrip() + '\n\nif __name__ == "__main__":\n    import pytest; pytest.main([__file__, "-x", "-s", "-k", "8"])\n'
    open(tr_path, 'w').write(tr)

    env = dict(os.environ)
    env.update({
        'SIM': 'icarus',
        'TOPLEVEL': module_name,
        'MODULE': 'test_int_controller',
        'TOPLEVEL_LANG': 'verilog',
        'VERILOG_SOURCES': os.path.join(rtl_dir, f"{module_name}.sv"),
        'COCOTB_RESULTS_FILE': os.path.join(wd, 'results.xml'),
        'COCOTB_LOG_LEVEL': 'DEBUG',
    })

    result = subprocess.run(
        [sys.executable, tr_path],
        capture_output=True, text=True, env=env,
        cwd=os.path.join(wd, 'src'), timeout=120
    )

    print("=== STDOUT (last 3000) ===")
    print(result.stdout[-3000:] if len(result.stdout) > 3000 else result.stdout)
    print("=== STDERR (last 2000) ===")
    print(result.stderr[-2000:] if len(result.stderr) > 2000 else result.stderr)
    print(f"RC: {result.returncode}")

    # List all files in workdir
    print("\n=== FILES in workdir ===")
    for root, dirs, files in os.walk(wd):
        for f in files:
            fp = os.path.join(root, f)
            print(fp)

    # Print harness_library.py specifically
    hl_fp = os.path.join(wd, 'src', 'harness_library.py')
    if os.path.exists(hl_fp):
        print(f"\n=== {hl_fp} ===")
        content = open(hl_fp).read()
        print(content)

    # Also try to read sim.log and xml
    for root, dirs, files in os.walk(wd):
        for f in files:
            if f.endswith('.log') or f.endswith('.xml'):
                fp = os.path.join(root, f)
                print(f"\n=== {fp} ===")
                content = open(fp).read()
                print(content[-5000:] if len(content) > 5000 else content)

    shutil.rmtree(wd)

if __name__ == '__main__':
    main()
