#!/usr/bin/env python3
"""Extract a CVDP problem from the JSONL, copy SV, and run cocotb test."""
import json, sys, os, subprocess, tempfile, shutil

JSONL = os.path.expanduser("~/github/cvdp_benchmark/full_dataset/cvdp_v1.0.4_nonagentic_code_generation_no_commercial.jsonl")
CVDP_DIR = os.path.dirname(os.path.abspath(__file__))

def load_problem(name_substr):
    with open(JSONL) as f:
        for line in f:
            entry = json.loads(line)
            if name_substr in entry['id']:
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
    run_env['VERILOG_SOURCES'] = os.path.join(rtl_dir, f"{module_name}.sv")

    # Fix test_runner.py import issues
    test_runner = os.path.join(workdir, 'src', 'test_runner.py')
    if os.path.exists(test_runner):
        content = open(test_runner).read()
        # Fix cocotb.runner -> cocotb_tools.runner
        content = content.replace('from cocotb.runner import', 'from cocotb_tools.runner import')
        # Find the actual runner function and ensure __main__ block calls it
        import re as _re
        # Look for function that calls get_runner (the build/test runner)
        func_match = _re.search(r'def (\w+)\([^)]*\):[^}]*?get_runner', content, _re.DOTALL)
        if not func_match:
            func_match = _re.search(r'def (test_\w+)\(\)', content)
        func_name = func_match.group(1) if func_match else 'test_runner'
        # Remove any existing __main__ block (commented or not)
        content = _re.sub(r'#?\s*if __name__.*?(?=\n\S|\Z)', '', content, flags=_re.DOTALL)
        # Check if runner needs args — if so, find a test function that calls it
        runner_args = _re.search(r'def ' + func_name + r'\(([^)]+)\)', content)
        if runner_args and runner_args.group(1).strip() and '=' not in runner_args.group(1):
            # Runner requires positional args, try to use pytest instead
            content = content.rstrip() + f'\n\nif __name__ == "__main__":\n    import pytest; pytest.main([__file__, "-x", "-v"])\n'
        else:
            content = content.rstrip() + f'\n\nif __name__ == "__main__":\n    {func_name}()\n'
        open(test_runner, 'w').write(content)

        result = subprocess.run(
            [sys.executable, test_runner],
            cwd=os.path.join(workdir, 'src'),
            env=run_env,
            capture_output=True, text=True, timeout=120
        )
    else:
        # Fallback: use makefiles
        result = subprocess.run(
            ['make', '-f', os.path.join(workdir, 'Makefile')],
            cwd=workdir,
            env=run_env,
            capture_output=True, text=True, timeout=120
        )

    print("STDOUT:", result.stdout[-1000:] if len(result.stdout) > 1000 else result.stdout)
    if result.stderr:
        print("STDERR:", result.stderr[-500:] if len(result.stderr) > 500 else result.stderr)
    print(f"Return code: {result.returncode}")

    passed = result.returncode == 0 and 'FAIL=0' in result.stdout
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
