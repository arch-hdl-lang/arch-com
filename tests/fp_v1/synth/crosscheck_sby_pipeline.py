#!/usr/bin/env python3
"""Independent Yosys/SBY cross-check of archived pipeline RTL, with concrete arithmetic.

No imports from the Python RTL parser, UF abstraction, or unroller. Python only
assembles SV fixtures/contracts and invokes SBY; Yosys owns all RTL semantics.
"""
import argparse
import gzip
import hashlib
import json
from pathlib import Path
import re
import subprocess
import time

HERE = Path(__file__).resolve().parent
INPUTS = HERE / 'evidence/all_cycles_2026-11-21/inputs'
FAULTS = ('skew', 'latency', 'reset-polarity', 'reset-value', 'valid-reset', 'early-valid')


def sha(data):
    return hashlib.sha256(data).hexdigest()


def once(text, old, new):
    if text.count(old) != 1:
        raise ValueError(f'expected exactly one mutation/observation site: {old!r}')
    return text.replace(old, new)


def load(key):
    meta = json.loads((INPUTS / (key+'.json')).read_text())
    result = {}
    for suffix in ('comb', 'staged'):
        name = key+'_'+suffix+'.sv.gz'
        data = gzip.decompress((INPUTS/name).read_bytes())
        if sha(data) != meta['uncompressed_sha256'][name]:
            raise ValueError(f'archive integrity mismatch: {name}')
        result[suffix] = data.decode()
    return meta, result['comb'], result['staged']


def mutate(rtl, meta, fault):
    n, rv = meta['latency'], meta['reset_value']
    if fault == 'baseline':
        return rtl
    if fault == 'reset-polarity':
        old = 'if ((!rst))' if meta['active_low'] else 'if (rst)'
        return once(rtl, old, 'if (rst)' if meta['active_low'] else 'if (!rst)')
    if fault == 'reset-value':
        return once(rtl, f"y <= 32'h{rv:08X};", f"y <= 32'h{rv^1:08X};")
    if fault == 'valid-reset':
        return once(rtl, "y_stg1 <= 1'b0;", "y_stg1 <= 1'b1;")
    if fault == 'early-valid':
        return once(rtl, f'y <= y_stg{n-1} ?', f'y <= y_stg{n-2} ?')
    if fault == 'latency':
        match = re.search(r'y <= y_stg\d+ \? (\w+) :', rtl)
        if not match:
            raise ValueError('missing wrapper output mux')
        core_y = match[1]
        rtl = once(rtl, f'logic [31:0] {core_y};', f'logic [31:0] {core_y};\nlogic [31:0] extra_delay;')
        rtl = once(rtl, f"y <= 32'h{rv:08X};", f"y <= 32'h{rv:08X};\nextra_delay <= 32'h{rv:08X};")
        return once(rtl, match[0], f'extra_delay <= {core_y};\ny <= y_stg{n-1} ? extra_delay :')
    if fault == 'skew':
        target, expr = ('r1__t16', 's1__t16') if meta['family'].startswith('FMA') else ('r0_0', 'pp0')
        rtl = once(rtl, f'{target} <= {expr};', '')
        # Replace a sequential driver with a combinational driver; leave consumers intact.
        start = rtl.index('module '+meta['core']+' ')
        end = rtl.index('endmodule', start)
        return rtl[:end]+f'always_comb {target} = {expr};\n'+rtl[end:]
    raise ValueError(fault)


def dot_invariants(meta):
    """Independent balanced-tree specification and read-only stage observations.

    All generated relationships are ASSERTIONS, never cutpoint assumptions.
    Names identify observation sites only; expressions come from the element
    format, block length, pairwise-tree algorithm and scale contract.
    """
    if meta['family'].startswith('FMA'):
        return [], '', ''
    fmt, count = meta['family'].split('/')
    count = int(count)
    bits = {'e2m1':4, 'e2m3':6, 'e3m2':6, 'e4m3':8, 'e5m2':8}[fmt]
    width = count*bits+8
    funcs, taps, layer = [], [], list(range(count))
    def function(idx, expr):
        funcs.append(f"function automatic [31:0] spec_node_{idx}(input [{width-1}:0] x, z);\n"
                     f"spec_node_{idx} = {expr};\nendfunction")
    def expected(idx, depth):
        return f'spec_node_{idx}(a_queue[{depth-1}], b_queue[{depth-1}])'
    for i in layer:
        function(i, f'arch_f32_mul(arch_{fmt}_to_f32(x[{i*bits} +: {bits}]), arch_{fmt}_to_f32(z[{i*bits} +: {bits}]))')
        taps.append((f'r0_{i}', 32, 1, expected(i,1)))
    idx, level = count, 0
    while len(layer)>1:
        level += 1
        nxt = []
        for j in range(0,len(layer),2):
            if j+1<len(layer):
                a,b = layer[j:j+2]
                function(idx, f'arch_f32_add(spec_node_{a}(x,z), spec_node_{b}(x,z))')
                taps.append((f'r{level}_{idx}',32,level+1,expected(idx,level+1)))
                nxt.append(idx); idx += 1
            else:
                a = layer[j]
                taps.append((f'r{level}_{a}_d',32,level+1,expected(a,level+1)))
                nxt.append(a)
        layer = nxt
    for j in range(1, meta['latency']):
        for operand in ('a','b'):
            taps.append((f'sd{operand}{j}',8,j,f'{operand}_queue[{j-1}][{width-1}:{width-8}]'))
    depth = meta['latency']-1
    taps.append(('m1r',32,depth,
                 f'arch_f32_mul({expected(layer[0],depth)}, arch_e8m0_to_f32(a_queue[{depth-1}][{width-1}:{width-8}]))'))
    checks, offset = [], 0
    for name, size, depth, expr in taps:
        checks.append(f'if (age >= {depth}) assert(observed_state[{offset} +: {size}] == {expr});')
        offset += size
    return taps, '\n'.join(funcs), '\n'.join(checks)


def combine(comb, staged, meta, profile="strengthened"):
    # Global helper definitions are emitted twice for block-dot. Drop only exact
    # duplicates from the staged prefix; never rewrite arithmetic expressions.
    start = staged.index('module '+meta['core']+' ')
    prefix, body = staged[:start], staged[start:]
    def dedup(m):
        if m[0] not in comb:
            raise ValueError('non-identical duplicated helper')
        return ''
    prefix = re.sub(r'function automatic\b.*?endfunction', dedup, prefix, flags=re.S)
    body = re.sub(r'package DF;\s*endpackage', '', body)
    # Expose actual fill-valid bits as read-only observation ports.
    taps, _, _ = dot_invariants(meta) if profile == "strengthened" else ([], "", "")
    if taps:
        total = sum(t[1] for t in taps)
        end = body.index('endmodule')
        core, rest = body[:end], body[end:]
        for name, size, _, _ in taps:
            if not re.search(r'logic \[{}:0\] '.format(size-1)+name+r'\s*;', core):
                raise ValueError('missing core observation: '+name)
        core = once(core, 'output logic [31:0] y', f'output logic [{total-1}:0] formal_state, output logic [31:0] y')
        core += 'assign formal_state = {'+', '.join(t[0] for t in reversed(taps))+'};\n'
        body = core+rest
    p = body.index('module Pipe (')
    head, wrapper = body[:p], body[p:]
    wrapper = once(wrapper, 'output logic [31:0] y',
                   f'output logic [{meta["latency"]-2}:0] observed_valid,\n  output logic [31:0] y')
    if taps:
        wrapper = once(wrapper, 'output logic [31:0] y', f'output logic [{total-1}:0] observed_state, output logic [31:0] y')
        wrapper = once(wrapper, '.clk(clk)', '.formal_state(observed_state), .clk(clk)')
    taps = ', '.join(f'y_stg{j}' for j in range(meta['latency']-1, 0, -1))
    wrapper = once(wrapper, 'endmodule', 'assign observed_valid = {'+taps+'};\nendmodule')
    return comb+'\n'+prefix+head+wrapper


def harness(comb, meta, window=False, profile="strengthened"):
    if profile == "history":
        return history_harness(comb, meta, window)
    n, rv = meta['latency'], meta['reset_value']
    header = re.search(r'module Ref\s*\((.*?)\);', comb, re.S)[1]
    ports = re.findall(r'input logic\s*(\[\d+:0\])?\s*(\w+)', header)
    if [name for _, name in ports] not in (['a','b'], ['a','b','c']):
        raise ValueError('unexpected reference interface')
    inputs = ', '.join('input '+width+' '+name for width,name in ports)
    connections = ', '.join(f'.{name}({name})' for _,name in ports)
    reset = '!rst' if meta['active_low'] else 'rst'
    gate = f'age >= {n}' if window else 'started'
    startup = '' if window else 'if (!started) assume(reset_active);'
    taps, functions, stage_checks = dot_invariants(meta)
    queue_decls, queue_updates, observations = '', '', ''
    ref_connections = connections
    data_expected = f'history[{n-1}]'
    if taps:
        total = sum(t[1] for t in taps)
        observations = f', .observed_state(observed_state)'
        queue_decls = f'wire [{total-1}:0] observed_state;\n'
        for width,name in ports:
            queue_decls += f'reg {width} {name}_queue[0:{n-1}];\n'
            queue_updates += f'{name}_queue[0] <= {name};\nfor (integer q=1; q<{n}; q=q+1) {name}_queue[q] <= {name}_queue[q-1];\n'
        ref_connections = ', '.join(f'.{name}({name}_queue[{n-1}])' for _,name in ports)
        data_expected = 'ref_y'
    startup_invariants = '' if window else '\n'.join(
        f'if (started) assert(!clean[{j}] || age >= {j+1});' for j in range(n))
    valid_checks = '\n'.join(f'assert(observed_valid[{j}] == clean[{j}]);' for j in range(n-1))
    return f'''
module top(input clk, input rst, {inputs});
  wire [31:0] y, ref_y;
  wire [{n-2}:0] observed_valid;
  wire reset_active = {reset};
  Pipe dut(.clk(clk), .rst(rst), {connections}, .y(y), .observed_valid(observed_valid){observations});
  Ref reference_dut({ref_connections}, .y(ref_y));
  {queue_decls}
  {functions}
  // Independent queue specification, not a copy of the implementation schedule.
  reg [31:0] history [0:{n-1}];
  reg [{n-1}:0] clean;
  reg started = 0;
  reg [31:0] age = 0;
  reg saw_live = 0, saw_reset_after_live = 0;
  integer i;
  always @(posedge clk) begin
    started <= 1;
    if (age < {n+1}) age <= age + 1;
    {startup}
    {queue_updates}
    assert(age <= {n+1});
    assert(!started || age >= 1);
    {startup_invariants}
    {stage_checks}
    history[0] <= ref_y;
    clean[0] <= !reset_active;
    for (i=1; i<{n}; i=i+1) begin
      history[i] <= history[i-1];
      clean[i] <= !reset_active && clean[i-1];
    end
    if ({gate}) begin
      assert(y == (clean[{n-1}] ? {data_expected} : 32'h{rv:08X}));
      {valid_checks}
    end
    if (age >= {n} && clean[{n-1}]) saw_live <= 1;
    if (saw_live && reset_active) saw_reset_after_live <= 1;
    cover(saw_reset_after_live && clean[{n-1}]);
  end
endmodule
'''


def history_harness(comb, meta, window=False):
    """Original output/fill-valid contract, used by the historical 32-element run."""
    n, rv = meta['latency'], meta['reset_value']
    header = re.search(r'module Ref\s*\((.*?)\);', comb, re.S)[1]
    ports = re.findall(r'input logic\s*(\[\d+:0\])?\s*(\w+)', header)
    if [name for _, name in ports] not in (['a','b'], ['a','b','c']):
        raise ValueError('unexpected reference interface')
    inputs = ', '.join('input '+width+' '+name for width,name in ports)
    connections = ', '.join(f'.{name}({name})' for _,name in ports)
    reset = '!rst' if meta['active_low'] else 'rst'
    gate = f'age >= {n}' if window else 'started'
    startup = '' if window else 'if (!started) assume(reset_active);'
    valid_checks = '\n'.join(f'assert(observed_valid[{j}] == clean[{j}]);' for j in range(n-1))
    return f"""
module top(input clk, input rst, {inputs});
  wire [31:0] y, ref_y;
  wire [{n-2}:0] observed_valid;
  wire reset_active = {reset};
  Pipe dut(.clk(clk), .rst(rst), {connections}, .y(y), .observed_valid(observed_valid));
  Ref reference_dut({connections}, .y(ref_y));
  // Independent queue specification, not a copy of the implementation schedule.
  reg [31:0] history [0:{n-1}];
  reg [{n-1}:0] clean;
  reg started = 0;
  reg [31:0] age = 0;
  reg saw_live = 0, saw_reset_after_live = 0;
  integer i;
  always @(posedge clk) begin
    started <= 1;
    if (age < {n+1}) age <= age + 1;
    {startup}
    history[0] <= ref_y;
    clean[0] <= !reset_active;
    for (i=1; i<{n}; i=i+1) begin
      history[i] <= history[i-1];
      clean[i] <= !reset_active && clean[i-1];
    end
    if ({gate}) begin
      assert(y == (clean[{n-1}] ? history[{n-1}] : 32'h{rv:08X}));
      {valid_checks}
    end
    if (age >= {n} && clean[{n-1}]) saw_live <= 1;
    if (saw_live && reset_active) saw_reset_after_live <= 1;
    cover(saw_reset_after_live && clean[{n-1}]);
  end
endmodule
"""


def config(meta, task, timeout, profile="strengthened"):
    depth = meta['latency']+3
    if profile == 'strengthened' and task == 'induct' and not meta['family'].startswith('FMA'):
        depth = 3
    mode = 'prove' if task == 'induct' else 'cover' if task == 'cover' else 'bmc'
    skip = f'skip {depth-1}\n' if task == 'induct' else ''
    if task == 'cover':
        depth = 2*meta['latency']+6
    return f'''[options]
mode {mode}
depth {depth}
{skip}timeout {timeout}
[engines]
smtbmc bitwuzla
[script]
read_verilog -formal -sv design.sv
prep -top top
[files]
design.sv
'''


def run_case(root, key, meta, comb, rtl, fault, task, timeout, profile):
    dest = root/key/(fault+'_'+task)
    dest.mkdir(parents=True, exist_ok=True)
    design = combine(comb, mutate(rtl, meta, fault), meta, profile)+harness(comb, meta, task == 'window', profile)
    (dest/'design.sv').write_text(design)
    cfg = config(meta, task, timeout, profile)
    (dest/'check.sby').write_text(cfg)
    t = time.monotonic()
    with (dest/'console.log').open('w') as log:
        process = subprocess.run(['sby', '-f', 'check.sby'], cwd=dest, stdout=log, stderr=subprocess.STDOUT)
    status_path = dest/'check/status'
    status = status_path.read_text().split()[0] if status_path.exists() else 'ERROR'
    expected = 'PASS' if fault == 'baseline' else 'FAIL'
    # Fault FAIL must be a bounded assertion counterexample, not an induction failure.
    witnesses = list((dest/'check/engine_0').glob('trace*.vcd'))
    ok = (status == expected and process.returncode == (0 if fault == 'baseline' else 2)
          and (fault == 'baseline' or bool(witnesses)))
    result = dict(key=key, fault=fault, task=task, status=status, expected=expected,
                  ok=ok, returncode=process.returncode, seconds=round(time.monotonic()-t,3),
                  design_sha256=sha(design.encode()), config_sha256=sha(cfg.encode()),
                  traces=[str(x.relative_to(root)) for x in witnesses])
    print(f'{key} {fault}/{task}: {status} ({result["seconds"]}s)', flush=True)
    return result


def main():
    global INPUTS
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--output', type=Path, required=True)
    ap.add_argument('--inputs', type=Path, default=INPUTS, help='archived SV inputs and hash manifests')
    ap.add_argument('--keys', nargs='+', default=['FMA6_high','FMA7_low','e2m1_3_high','e5m2_3_low'])
    ap.add_argument('--all', action='store_true', help='all configurations present in --inputs (five supplied)')
    ap.add_argument('--timeout', type=int, default=180)
    ap.add_argument('--profile', choices=('strengthened', 'history'), default='strengthened',
                    help='history reproduces the original 32-element output/fill-valid contract')
    args = ap.parse_args()
    if args.timeout <= 0:
        ap.error('timeout must be positive')
    INPUTS = args.inputs.resolve()
    root = args.output.resolve()
    if root.exists():
        ap.error('output directory must not exist; use a new directory for each run')
    root.mkdir(parents=True)
    (root/'runner.py').write_bytes(Path(__file__).read_bytes())
    keys = sorted(p.stem for p in INPUTS.glob('*.json')) if args.all else args.keys
    if not keys or len(set(keys)) != len(keys):
        ap.error('select at least one distinct configuration')
    if any(not re.fullmatch(r'[A-Za-z0-9_]+', key) for key in keys):
        ap.error('keys must be configuration names, not paths')
    receipt = dict(schema=1, source=str(INPUTS), concrete_arithmetic=True,
                   profile=args.profile, timeout_seconds_per_task=args.timeout,
                   script_sha256=sha(Path(__file__).read_bytes()), results=[], inputs={},
                   all_cycles_rule='Require both unskipped startup BMC and induction PASS; never accept skipped-base proof alone.')
    receipt['versions'] = {tool: subprocess.run(cmd, capture_output=True, text=True).stdout.strip()
                           for tool,cmd in [('yosys',['yosys','-V']),('sby',['sby','--version']),('bitwuzla',['bitwuzla','--version'])]}
    for key in keys:
        meta, comb, rtl = load(key)
        receipt['inputs'][key] = meta
        for fault, task in [('baseline',t) for t in ('startup','induct','window','cover')]+[(f,'startup') for f in FAULTS]:
            receipt['results'].append(run_case(root,key,meta,comb,rtl,fault,task,args.timeout,args.profile))
            receipt['complete'] = False
            (root/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
    receipt['complete'] = True
    receipt['passed'] = bool(receipt['results']) and all(r['ok'] for r in receipt['results'])
    receipt['all_cycles_proved'] = all(
        rows['startup']['ok'] and rows['induct']['ok']
        and rows['startup']['design_sha256'] == rows['induct']['design_sha256']
        for key in keys
        for rows in [{r['task']: r for r in receipt['results']
                      if r['key'] == key and r['fault'] == 'baseline'}])
    receipt['passed'] = receipt['passed'] and receipt['all_cycles_proved']
    (root/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
    return 0 if receipt['passed'] else 1

if __name__ == '__main__':
    raise SystemExit(main())
