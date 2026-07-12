#!/usr/bin/env python3
"""Nightly dual-backend equivalence sweep glue (closes #244).

Reuses the existing broad backend gate (`tools/run_arch_regression.py`,
which for every unit runs `arch check` -> `arch build` -> Verilator lint on
the emitted SV -> `arch sim` — a real C++ testbench run when the unit has an
entry in `tests/arch_sim_manifest.json`, giving true behavioral coverage of
both the SV and native-sim backends for those units) plus the dedicated
`tests/backend_equiv/` torture-fixture suite (Vec<Bus>+thread+generate+param
interaction fixtures, each a `check`/`build`/`sim` pass across both
backends).

This script does not invent a new comparison framework; it schedules the
existing ones and adds flake-hardening on top of `run_arch_regression.py`:
bounded parallelism for the main sweep, then one automatic *serial* re-run
of anything that failed before it is reported as a real divergence. The
known failure mode (documented in harc-com, same class applies here) is
transient Verilator failures ("Broken pipe" and friends) under parallel
load — a single random fixture going red per run. A serial re-run absorbs
that class; anything still red after the retry is a real regression.

Writes a GitHub Actions job summary (if GITHUB_STEP_SUMMARY is set) listing
which fixtures diverged for real vs. which were flaky and passed on retry,
so a red night is actionable without downloading logs.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path


def repo_root() -> Path:
    return Path(__file__).resolve().parents[1]


def run(cmd: list[str], **kwargs) -> subprocess.CompletedProcess:
    print(f"[run] {' '.join(cmd)}", flush=True)
    return subprocess.run(cmd, **kwargs)


def load_summary(work_dir: Path) -> dict:
    return json.loads((work_dir / "summary.json").read_text())


def failed_unit_names(summary: dict) -> list[str]:
    return sorted(
        r["name"]
        for r in summary["results"]
        if r["status"] not in {"pass", "skip_check_failed"}
    )


def write_retry_baseline(path: Path, names: list[str]) -> None:
    payload = {"units": [{"name": n} for n in names]}
    path.write_text(json.dumps(payload, indent=2) + "\n")


def append_summary(lines: list[str]) -> None:
    text = "\n".join(lines) + "\n"
    print(text)
    summary_path = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary_path:
        with open(summary_path, "a") as f:
            f.write(text)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--jobs", type=int, default=2, help="Parallelism for the main sweep (bounded to survive Verilator flakiness under load).")
    parser.add_argument("--work-dir", type=Path, default=None, help="Scratch dir; defaults to a fresh tempdir.")
    parser.add_argument("--baseline", type=Path, default=Path("tests/arch_regression_baseline.json"))
    parser.add_argument("--sim-manifest", type=Path, default=Path("tests/arch_sim_manifest.json"))
    parser.add_argument("--skip-backend-equiv", action="store_true", help="Skip tests/backend_equiv/run.sh (for quick local smoke checks).")
    parser.add_argument("--pattern", action="append", default=[], help="Restrict run_arch_regression.py to matching unit paths (repeatable). Useful for a fast local smoke run.")
    args = parser.parse_args()

    root = repo_root()
    work_dir = args.work_dir or Path(tempfile.mkdtemp(prefix="arch-nightly-equiv-"))
    work_dir.mkdir(parents=True, exist_ok=True)
    arch_bin = root / "target" / "release" / "arch"

    summary_lines = ["# Nightly dual-backend equivalence sweep", ""]

    # --- Main sweep: bounded parallelism ---
    main_cmd = [
        sys.executable,
        str(root / "tools" / "run_arch_regression.py"),
        "--arch-bin", str(arch_bin),
        "--work-dir", str(work_dir / "main"),
        "--jobs", str(args.jobs),
        "--sim-manifest", str(args.sim_manifest),
        "--allow-failures",  # we triage failures ourselves below
    ]
    # --pattern narrows discovery for a fast local smoke run; the baseline
    # (grouped-directory unit names) doesn't line up with raw file globs, so
    # skip it in that mode. Full nightly runs use the baseline as intended.
    if args.pattern:
        for pat in args.pattern:
            main_cmd += ["--pattern", pat]
    else:
        main_cmd += ["--baseline", str(args.baseline)]
    proc = run(main_cmd, cwd=root)
    main_summary = load_summary(work_dir / "main")
    total_units = len(main_summary["results"])
    first_failures = failed_unit_names(main_summary)

    real_failures: list[str] = []
    flaky_passed: list[str] = []

    if first_failures:
        print(f"[nightly] {len(first_failures)}/{total_units} unit(s) failed on first pass; retrying serially to filter flakes", flush=True)
        retry_baseline = work_dir / "retry_baseline.json"
        write_retry_baseline(retry_baseline, first_failures)
        retry_cmd = [
            sys.executable,
            str(root / "tools" / "run_arch_regression.py"),
            "--arch-bin", str(arch_bin),
            "--work-dir", str(work_dir / "retry"),
            "--jobs", "1",
            "--baseline", str(retry_baseline),
            "--sim-manifest", str(args.sim_manifest),
            "--allow-failures",
        ]
        run(retry_cmd, cwd=root)
        retry_summary = load_summary(work_dir / "retry")
        still_failing = set(failed_unit_names(retry_summary))
        for name in first_failures:
            if name in still_failing:
                real_failures.append(name)
            else:
                flaky_passed.append(name)
    else:
        print(f"[nightly] all {total_units} units passed on first pass, no retry needed", flush=True)

    # --- Dedicated equivalence torture fixtures ---
    backend_equiv_ok = True
    backend_equiv_out = ""
    if not args.skip_backend_equiv:
        be = run(
            ["bash", "./run.sh"],
            cwd=root / "tests" / "backend_equiv",
            env={**os.environ, "ARCH": str(arch_bin)},
            capture_output=True,
            text=True,
        )
        backend_equiv_out = be.stdout + be.stderr
        print(backend_equiv_out)
        backend_equiv_ok = be.returncode == 0

    # --- Summary ---
    summary_lines.append(f"Main sweep: {total_units} unit(s), jobs={args.jobs}, baseline={args.baseline}")
    summary_lines.append("")
    if real_failures:
        summary_lines.append(f"## Real divergences ({len(real_failures)}) — failed on first pass AND serial retry")
        summary_lines.append("")
        summary_lines.append("| Unit | Failed step |")
        summary_lines.append("|---|---|")
        by_name = {r["name"]: r for r in load_summary(work_dir / "retry")["results"]}
        for name in real_failures:
            result = by_name.get(name, {})
            steps = [s for s in result.get("steps", []) if s["status"] not in {"pass", "skip"}]
            step_name = steps[-1]["name"] if steps else "runner"
            summary_lines.append(f"| `{name}` | {step_name} |")
        summary_lines.append("")
    else:
        summary_lines.append("No real divergences in the main sweep.")
        summary_lines.append("")

    if flaky_passed:
        summary_lines.append(f"## Flaky (passed on serial retry) ({len(flaky_passed)})")
        summary_lines.append("")
        summary_lines.append("These failed under `--jobs {}` on the first pass but passed when re-run serially — ".format(args.jobs))
        summary_lines.append("consistent with known Verilator-under-parallel-load flakiness, not a real regression.")
        summary_lines.append("")
        for name in flaky_passed:
            summary_lines.append(f"- `{name}`")
        summary_lines.append("")

    summary_lines.append("## backend_equiv torture fixtures")
    summary_lines.append("")
    summary_lines.append("PASS" if backend_equiv_ok else "FAIL — see job log for `tests/backend_equiv/run.sh` output")
    summary_lines.append("")

    append_summary(summary_lines)

    ok = not real_failures and backend_equiv_ok
    if not ok:
        print("[nightly] FAILED: real divergence(s) found or backend_equiv fixtures broken", file=sys.stderr)
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
