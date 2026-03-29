#!/usr/bin/env python3
"""Run interrupt controller test with verbose cocotb log output."""
import json, os, shutil, tempfile, subprocess, sys, re, glob

JSONL = os.path.expanduser("~/github/cvdp_benchmark/full_dataset/cvdp_v1.0.4_nonagentic_code_generation_no_commercial.jsonl")
CVDP_DIR = os.path.dirname(os.path.abspath(__file__))
OUT_LOG = os.path.join(CVDP_DIR, '_ic_sim.log')

with open(JSONL) as f:
    for line in f:
        d = json.loads(line)
        if 'interrupt_controller_0019' in d['id']:
            prob = d
            break

module_name = 'interrupt_controller_apb'
sv_file = os.path.join(CVDP_DIR, f"{module_name}.sv")
wd = tempfile.mkdtemp(prefix=f'debug_{module_name}_')
print(f"Working dir: {wd}", flush=True)

for fname, content in prob['harness']['files'].items():
    fpath = os.path.join(wd, fname)
    os.makedirs(os.path.dirname(fpath), exist_ok=True)
    open(fpath, 'w').write(content)

rtl_dir = os.path.join(wd, 'rtl')
os.makedirs(rtl_dir, exist_ok=True)
shutil.copy(sv_file, os.path.join(rtl_dir, f"{module_name}.sv"))
shutil.copy(sv_file, os.path.join(rtl_dir, f"{module_name}.v"))

sv_src = open(sv_file).read()
input_names = set(re.findall(r'input\s+(?:logic\s+)?(?:(?:signed|unsigned)\s+)?(?:\[[^\]]*\]\s*)?(\w+)', sv_src))
hl_path = os.path.join(wd, 'src', 'harness_library.py')
if os.path.exists(hl_path):
    hl = open(hl_path).read()
    hl = hl.replace('signal._type == "GPI_NET"', f'(signal._type == "GPI_NET" or signal._name in {repr(input_names)})')
    open(hl_path, 'w').write(hl)

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
    'COCOTB_LOG_LEVEL': 'INFO',
})

result = subprocess.run(
    [sys.executable, tr_path],
    capture_output=True, text=True, env=env,
    cwd=os.path.join(wd, 'src'), timeout=120
)

# Copy all interesting files
for root, dirs, files in os.walk(wd):
    for f in files:
        if f.endswith('.xml') or f.endswith('.log'):
            src = os.path.join(root, f)
            dst = os.path.join(CVDP_DIR, '_ic2_' + f)
            shutil.copy(src, dst)
            print(f"Saved: {dst}")

print("=== STDOUT (last 8000) ===")
print(result.stdout[-8000:] if len(result.stdout) > 8000 else result.stdout)
print("=== STDERR (last 4000) ===")
print(result.stderr[-4000:] if len(result.stderr) > 4000 else result.stderr)

# Don't delete wd - keep for inspection
print(f"\nTemp dir kept at: {wd}")
