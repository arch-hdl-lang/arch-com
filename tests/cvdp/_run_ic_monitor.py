#!/usr/bin/env python3
"""Run with injected monitor coroutine to trace DUT state."""
import json, os, shutil, tempfile, subprocess, sys, re

JSONL = os.path.expanduser("~/github/cvdp_benchmark/full_dataset/cvdp_v1.0.4_nonagentic_code_generation_no_commercial.jsonl")
CVDP_DIR = os.path.dirname(os.path.abspath(__file__))

with open(JSONL) as f:
    for line in f:
        d = json.loads(line)
        if 'interrupt_controller_0019' in d['id']:
            prob = d
            break

module_name = 'interrupt_controller_apb'
sv_file = os.path.join(CVDP_DIR, f"{module_name}.sv")
wd = tempfile.mkdtemp(prefix=f'debug_{module_name}_')

for fname, content in prob['harness']['files'].items():
    fpath = os.path.join(wd, fname)
    os.makedirs(os.path.dirname(fpath), exist_ok=True)
    open(fpath, 'w').write(content)

rtl_dir = os.path.join(wd, 'rtl')
os.makedirs(rtl_dir, exist_ok=True)
shutil.copy(sv_file, os.path.join(rtl_dir, f"{module_name}.sv"))

sv_src = open(sv_file).read()
input_names = set(re.findall(r'input\s+(?:logic\s+)?(?:(?:signed|unsigned)\s+)?(?:\[[^\]]*\]\s*)?(\w+)', sv_src))
hl_path = os.path.join(wd, 'src', 'harness_library.py')
if os.path.exists(hl_path):
    hl = open(hl_path).read()
    hl = hl.replace('signal._type == "GPI_NET"', f'(signal._type == "GPI_NET" or signal._name in {repr(input_names)})')
    open(hl_path, 'w').write(hl)

# Inject a monitor into the test file
test_path = os.path.join(wd, 'src', 'test_int_controller.py')
test_src = open(test_path).read()

MONITOR_CODE = '''
async def monitor_signals(dut, test_done_ev):
    """Print key signals at every falling edge."""
    import cocotb
    from cocotb.triggers import RisingEdge, FallingEdge
    import cocotb.utils
    count = 0
    while not test_done_ev.is_set():
        await FallingEdge(dut.clk)
        count += 1
        try:
            sim_time = cocotb.utils.get_sim_time('ns')
            idx = int(dut.interrupt_idx.value)
            cpu_int = int(dut.cpu_interrupt.value)
            cpu_ack_v = int(dut.cpu_ack.value)
            svc = int(dut.interrupt_service.value)
            if cpu_int or idx or svc:
                cocotb.log.info(f"MON @{sim_time}ns: svc=0b{svc:08b} idx={idx} cpu_int={cpu_int} cpu_ack={cpu_ack_v}")
        except Exception as e:
            import cocotb.log as clog
            cocotb.log.warning(f"MON error @{count}: {e}")

'''

# Insert monitor before the test function
test_src = test_src.replace('@cocotb.test()', MONITOR_CODE + '@cocotb.test()')

# Inject monitor launch into the test
test_src = test_src.replace(
    't_check_int = cocotb.start_soon(hrs_lb.check_int_out(dut,interrupts_list,test_done))',
    't_monitor = cocotb.start_soon(monitor_signals(dut, test_done))\n    t_check_int = cocotb.start_soon(hrs_lb.check_int_out(dut,interrupts_list,test_done))'
)
open(test_path, 'w').write(test_src)

tr_path = os.path.join(wd, 'src', 'test_runner.py')
tr = open(tr_path).read()
tr = tr.replace('from cocotb.runner import', 'from cocotb_tools.runner import')
tr = re.sub(r'random_num_irq\s*=.*', 'random_num_irq = [8]', tr)
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
    'COCOTB_RANDOM_SEED': '12345',
})

result = subprocess.run(
    [sys.executable, tr_path],
    capture_output=True, text=True, env=env,
    cwd=os.path.join(wd, 'src'), timeout=120
)

# Print filtered output
lines = result.stdout.split('\n')
for line in lines:
    if 'MON' in line or 'Interrupt' in line or 'wrong' in line or 'WARNING' in line or 'FAIL' in line or 'TESTS=' in line:
        print(line)

shutil.rmtree(wd)
