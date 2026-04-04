#!/usr/bin/env python3
"""Extract a CVDP problem from the JSONL, copy SV, and run cocotb test."""
import json, sys, os, subprocess, tempfile, shutil, glob

JSONL = os.path.expanduser("~/github/cvdp_benchmark/full_dataset/cvdp_v1.0.4_nonagentic_code_generation_no_commercial.jsonl")
CVDP_DIR = os.path.dirname(os.path.abspath(__file__))

def load_problem(name_substr):
    with open(JSONL) as f:
        entries = [json.loads(line) for line in f]
    # Try matching problem ID first
    for entry in entries:
        if name_substr in entry['id']:
            return entry
    # Fall back to matching module name in output context
    for entry in entries:
        ctx = entry['output'].get('context', {})
        for fname in ctx:
            if fname.startswith('rtl/'):
                mod = fname.replace('rtl/', '').replace('.sv', '').replace('.v', '')
                if name_substr == mod or name_substr in mod:
                    return entry
    raise ValueError(f"No problem matching '{name_substr}'")

def extract_and_run(name_substr, sv_file=None):
    prob = load_problem(name_substr)
    pid = prob['id']
    print(f"Problem: {pid}")
    print(f"Categories: {prob['categories']}")

    # Find the module name from the output context
    rtl_files = prob['output'].get('context', {})
    module_name = None
    for fname in rtl_files:
        if fname.startswith('rtl/') and (fname.endswith('.sv') or fname.endswith('.v')):
            module_name = fname.replace('rtl/', '').replace('.sv', '')
            break

    if not module_name:
        print("Could not determine module name")
        return False

    # Strip .v extension if present
    if module_name.endswith('.v'):
        module_name = module_name[:-2]

    print(f"Module: {module_name}")

    # If no SV file provided, look for it in cvdp dir
    if sv_file is None:
        sv_file = os.path.join(CVDP_DIR, f"{module_name}.sv")

    if not os.path.exists(sv_file):
        print(f"SV file not found: {sv_file}")
        return False

    # Create temp directory for test
    workdir = tempfile.mkdtemp(prefix=f"cvdp_{module_name}_")

    # Extract harness files
    harness_files = prob['harness']['files']
    for fname, content in harness_files.items():
        fpath = os.path.join(workdir, fname)
        os.makedirs(os.path.dirname(fpath), exist_ok=True)
        with open(fpath, 'w') as f:
            f.write(content)

    # Copy SV file - also create .v symlink if needed
    rtl_dir = os.path.join(workdir, 'rtl')
    os.makedirs(rtl_dir, exist_ok=True)
    shutil.copy(sv_file, os.path.join(rtl_dir, f"{module_name}.sv"))
    # Some problems expect .v extension
    v_path = os.path.join(rtl_dir, f"{module_name}.v")
    if not os.path.exists(v_path):
        shutil.copy(sv_file, v_path)

    # Find the test python file
    test_file = None
    for fname in harness_files:
        if fname.startswith('src/test_') and fname.endswith('.py'):
            test_file = fname
            break

    if not test_file:
        print("No test file found in harness")
        shutil.rmtree(workdir)
        return False

    # Read .env for SIM and TOPLEVEL
    env_content = harness_files.get('src/.env', '')
    env_vars = {}
    for line in env_content.strip().split('\n'):
        if '=' in line and not line.startswith('#'):
            k, v = line.split('=', 1)
            env_vars[k.strip()] = v.strip()

    sim = env_vars.get('SIM', 'icarus')
    toplevel = env_vars.get('TOPLEVEL', module_name)
    module = env_vars.get('MODULE', test_file.replace('src/', '').replace('.py', ''))
    toplevel_lang = env_vars.get('TOPLEVEL_LANG', 'verilog')

    # Run with cocotb-runner style or Makefile
    # Use direct cocotb invocation
    run_env = os.environ.copy()
    run_env['SIM'] = sim
    run_env['TOPLEVEL'] = toplevel
    run_env['MODULE'] = module
    run_env['TOPLEVEL_LANG'] = toplevel_lang
    run_env['COCOTB_RESULTS_FILE'] = os.path.join(workdir, 'results.xml')
    # Build VERILOG_SOURCES: include all SV files listed in the harness env
    raw_sources = env_vars.get('VERILOG_SOURCES', f'/code/rtl/{module_name}.sv')
    sv_sources = []
    for src in raw_sources.split():
        # Map /code/rtl/NAME.sv -> our local rtl_dir/NAME.sv
        import posixpath
        fname = posixpath.basename(src)
        local = os.path.join(rtl_dir, fname)
        # Copy the matching arch-generated SV if available
        stem = fname.replace('.sv', '').replace('.v', '')
        for ext in ['.sv', '.v']:
            arch_sv = os.path.join(CVDP_DIR, f'{stem}{ext}')
            if os.path.exists(arch_sv):
                import shutil as _sh
                _sh.copy(arch_sv, local)
                break
        if os.path.exists(local):
            sv_sources.append(local)
        else:
            sv_sources.append(os.path.join(rtl_dir, f"{module_name}.sv"))
    # Deduplicate: some harness .env files list the same source twice
    sv_sources = list(dict.fromkeys(sv_sources))
    run_env['VERILOG_SOURCES'] = ' '.join(sv_sources) if sv_sources else os.path.join(rtl_dir, f"{module_name}.sv")

    # Patch harness_library dut_init: icarus exposes inputs as GPI_LOGIC/GPI_LOGIC_ARRAY, not GPI_NET,
    # so the original dut_init never initializes inputs on icarus, leaving them X.
    # Fix: parse input port names from the SV file and patch dut_init to only zero those.
    import re as _re
    hl_path = os.path.join(workdir, 'src', 'harness_library.py')
    sv_path = os.path.join(rtl_dir, f"{module_name}.sv")
    if os.path.exists(hl_path) and os.path.exists(sv_path):
        sv_src = open(sv_path).read()
        input_names = set(_re.findall(r'input\s+(?:logic\s+)?(?:(?:signed|unsigned)\s+)?(?:\[[^\]]*\]\s*)?(\w+)', sv_src))
        if input_names:
            hl = open(hl_path).read()
            if 'dut_init' in hl:
                names_str = repr(input_names)
                hl = hl.replace(
                    'signal._type == "GPI_NET"',
                    f'(signal._type == "GPI_NET" or signal._name in {names_str})'
                )
                open(hl_path, 'w').write(hl)

    # Fix import issues in all Python files
    for pyfile in glob.glob(os.path.join(workdir, 'src', '*.py')):
        pycontent = open(pyfile).read()
        changed = False
        if 'cocotb.sim_time_utils' in pycontent:
            pycontent = pycontent.replace('from cocotb.sim_time_utils import', 'from cocotb.utils import')
            changed = True
        # Fix Logic vs LogicArray: .integer on single-bit Logic (cocotb 2.0)
        if '.value.integer' in pycontent:
            fix = "\n# Monkey-patch cocotb Logic to support .integer\ntry:\n    from cocotb.types import Logic\n    if not hasattr(Logic, 'integer'):\n        Logic.integer = property(lambda self: int(self))\nexcept Exception:\n    pass\n"
            pycontent = fix + pycontent
            changed = True
        # Fix Logic vs LogicArray: .to_signed()/.to_unsigned() on single-bit Logic
        if '.to_signed()' in pycontent or '.to_unsigned()' in pycontent:
            fix = "\n# Monkey-patch cocotb Logic to support to_signed/to_unsigned\ntry:\n    from cocotb.types import Logic\n    if not hasattr(Logic, 'to_unsigned'):\n        Logic.to_unsigned = lambda self: int(self)\n    if not hasattr(Logic, 'to_signed'):\n        Logic.to_signed = lambda self: int(self)\nexcept Exception:\n    pass\n"
            pycontent = fix + pycontent
            changed = True
        # Fix odd Clock periods: cocotb 2.0 requires period divisible by 2
        import re as _re2
        def _fix_odd_clock(m):
            val = int(m.group(2))
            if val % 2 != 0:
                val += 1
            return f'Clock({m.group(1)}, {val},'
        pycontent_new = _re2.sub(r'Clock\(([^,]+),\s*(\d+),', _fix_odd_clock, pycontent)
        if pycontent_new != pycontent:
            pycontent = pycontent_new
            changed = True
        # Fix variable clock periods: ensure randint ranges produce even values
        if 'random.randint(2, 20)' in pycontent and 'clock' in pycontent.lower():
            pycontent = pycontent.replace('random.randint(2, 20)', 'random.randint(1, 10) * 2')
            changed = True
        # Fix variable-based odd clock period assignments (e.g., r_clk_period=15 → r_clk_period=16)
        def _fix_odd_kw(m):
            val = int(m.group(2))
            if val % 2 != 0:
                val += 1
            return f'{m.group(1)}={val}'
        pycontent_kw = _re2.sub(r'(\w*[Cc]lk\w*_period)\s*=\s*(\d+)', _fix_odd_kw, pycontent)
        if pycontent_kw != pycontent:
            pycontent = pycontent_kw
            changed = True
        # Also fix randint ranges that may produce odd values for clock variables
        if 'random.randint(5, 50)' in pycontent and 'clk' in pycontent.lower():
            pycontent = pycontent.replace('random.randint(5, 50)', 'random.randint(3, 25) * 2')
            changed = True
        # Fix cocotb 2.0 defines={...: None} — None not serializable as SV literal
        if ': None}' in pycontent or ': None,' in pycontent:
            pycontent = pycontent.replace(': None}', ': 1}').replace(': None,', ': 1,')
            changed = True
        # Fix cocotb 2.0: packed arrays cannot be subscript-indexed (dut.sig[i])
        # Replace int(dut.X[N]) patterns with bit-extract from int value
        if _re2.search(r'int\(dut\.\w+\[\d+\]\)', pycontent):
            pycontent = _re2.sub(
                r'int\(dut\.(\w+)\[(\d+)\]\)',
                lambda m: f'((int(dut.{m.group(1)}.value) >> {m.group(2)}) & 1)',
                pycontent
            )
            changed = True
        if changed:
            open(pyfile, 'w').write(pycontent)

    # Fix test_runner.py import issues
    test_runner = os.path.join(workdir, 'src', 'test_runner.py')
    if os.path.exists(test_runner):
        content = open(test_runner).read()
        # Normalize runner import for cocotb 2.0
        content = content.replace('from cocotb.runner import', 'from cocotb_tools.runner import')
        import re as _re
        # Remove any existing __main__ block
        content = _re.sub(r'\n*#?\s*if __name__\s*==.*', '', content, flags=_re.DOTALL)
        # Find function that calls get_runner
        func_match = _re.search(r'def (\w+)\([^)]*\).*?get_runner', content, _re.DOTALL)
        if not func_match:
            func_match = _re.search(r'def (test_\w+)\(\)', content)
        func_name = func_match.group(1) if func_match else 'test_runner'
        # If pytest.mark.parametrize is used, always use pytest
        if '@pytest.mark.parametrize' in content:
            content = content.rstrip() + f'\n\nif __name__ == "__main__":\n    import pytest; pytest.main([__file__, "-x", "-v"])\n'
        else:
            # Check if runner needs positional args
            runner_args = _re.search(r'def ' + func_name + r'\(([^)]+)\)', content)
            if runner_args and runner_args.group(1).strip() and '=' not in runner_args.group(1):
                content = content.rstrip() + f'\n\nif __name__ == "__main__":\n    import pytest; pytest.main([__file__, "-x", "-v"])\n'
            else:
                content = content.rstrip() + f'\n\nif __name__ == "__main__":\n    {func_name}()\n'
        open(test_runner, 'w').write(content)

        result = subprocess.run(
            [sys.executable, test_runner],
            cwd=os.path.join(workdir, 'src'),
            env=run_env,
            capture_output=True, text=True, timeout=600
        )
    else:
        # Fallback: use makefiles
        result = subprocess.run(
            ['make', '-f', os.path.join(workdir, 'Makefile')],
            cwd=workdir,
            env=run_env,
            capture_output=True, text=True, timeout=600
        )

    print("STDOUT:", result.stdout[-1000:] if len(result.stdout) > 1000 else result.stdout)
    if result.stderr:
        print("STDERR:", result.stderr[-2000:] if len(result.stderr) > 2000 else result.stderr)
    print(f"Return code: {result.returncode}")

    passed = result.returncode == 0 and ('FAIL=0' in result.stdout or ('passed' in result.stdout and 'failed' not in result.stdout.lower()))
    print(f"{'PASS' if passed else 'FAIL'}: {module_name}")

    # Cleanup
    if passed:
        shutil.rmtree(workdir)
    else:
        print(f"Workdir preserved: {workdir}")

    return passed

if __name__ == '__main__':
    if len(sys.argv) < 2:
        print("Usage: run_cvdp.py <problem_name_substr> [sv_file]")
        sys.exit(1)
    sv = sys.argv[2] if len(sys.argv) > 2 else None
    ok = extract_and_run(sys.argv[1], sv)
    sys.exit(0 if ok else 1)
