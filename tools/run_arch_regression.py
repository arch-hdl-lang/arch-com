#!/usr/bin/env python3
"""Run broad ARCH compiler regression checks over tests/*.arch.

The runner treats "passing .arch files" as sources that pass `arch check`.
For each passing unit it runs:

  1. `arch build` to emit SystemVerilog
  2. Verilator lint on the emitted SystemVerilog
  3. `arch sim` with a real C++ TB when mapped in the sim manifest, otherwise
     `arch sim` model generation plus a generated C++ smoke compile

`arch build` writes `.archi` files next to inputs, so this script copies the
test tree into a scratch work directory before invoking the compiler.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import dataclasses
import fnmatch
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import time
from typing import Iterable


DEFAULT_VERILATOR_FLAGS = [
    "--lint-only",
    "--sv",
    "--assert",
    "--timing",
    "-Wno-fatal",
    "-Wno-DECLFILENAME",
    "-Wno-UNUSEDSIGNAL",
    "-Wno-UNUSEDPARAM",
    "-Wno-BLKANDNBLK",
    "-Wno-CASEINCOMPLETE",
    "-Wno-WIDTH",
]


@dataclasses.dataclass(frozen=True)
class Unit:
    name: str
    files: tuple[Path, ...]
    original_files: tuple[Path, ...]


@dataclasses.dataclass
class StepResult:
    name: str
    status: str
    seconds: float
    command: list[str]
    stdout: str = ""
    stderr: str = ""


@dataclasses.dataclass
class UnitResult:
    name: str
    files: list[str]
    status: str
    seconds: float
    steps: list[StepResult]


@dataclasses.dataclass(frozen=True)
class SimManifestEntry:
    name: str
    arch_files: tuple[Path, ...]
    tb_files: tuple[Path, ...]
    args: tuple[str, ...] = ()


def repo_root() -> Path:
    return Path(__file__).resolve().parents[1]


def run_cmd(
    command: list[str],
    cwd: Path,
    timeout: float | None,
    log_dir: Path,
    step_name: str,
) -> StepResult:
    start = time.monotonic()
    try:
        proc = subprocess.run(
            command,
            cwd=cwd,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout,
        )
        status = "pass" if proc.returncode == 0 else "fail"
        stdout = proc.stdout
        stderr = proc.stderr
    except subprocess.TimeoutExpired as exc:
        status = "timeout"
        stdout = exc.stdout or ""
        stderr = exc.stderr or ""
    seconds = time.monotonic() - start

    log_dir.mkdir(parents=True, exist_ok=True)
    (log_dir / f"{step_name}.cmd").write_text(" ".join(command) + "\n")
    (log_dir / f"{step_name}.stdout").write_text(stdout)
    (log_dir / f"{step_name}.stderr").write_text(stderr)
    return StepResult(step_name, status, seconds, command, stdout, stderr)


def ensure_arch_bin(root: Path, explicit: str | None, release: bool) -> Path:
    if explicit:
        arch_bin = Path(explicit).expanduser()
        if not arch_bin.exists():
            raise SystemExit(f"--arch-bin does not exist: {arch_bin}")
        return arch_bin.resolve()

    profile = "release" if release else "debug"
    arch_bin = root / "target" / profile / "arch"
    if arch_bin.exists():
        return arch_bin

    cargo_cmd = ["cargo", "build"]
    if release:
        cargo_cmd.append("--release")
    print(f"[setup] building compiler: {' '.join(cargo_cmd)}", flush=True)
    subprocess.run(cargo_cmd, cwd=root, check=True)
    return arch_bin


def copy_tests_tree(root: Path, work_dir: Path, tests_dir: Path) -> Path:
    source = root / tests_dir
    copied = work_dir / "src" / tests_dir
    if copied.exists():
        shutil.rmtree(copied)
    copied.parent.mkdir(parents=True, exist_ok=True)

    def ignore(_dir: str, names: list[str]) -> set[str]:
        return {
            name
            for name in names
            if name in {"obj_dir", "sim_build", "arch_sim_build", "__pycache__"}
            or name.endswith(".archi")
            or name.endswith(".vcd")
            or name.endswith(".fst")
        }

    shutil.copytree(source, copied, ignore=ignore)
    return copied


def safe_unit_name(path: Path) -> str:
    if str(path) in {"", "."}:
        return "tests_root"
    raw = "__".join(path.parts)
    return "".join(ch if ch.isalnum() or ch in "._-" else "_" for ch in raw)


def rel_tuple(paths: Iterable[Path], base: Path) -> tuple[Path, ...]:
    return tuple(sorted(p.relative_to(base) for p in paths))


def matches_any(path: Path, patterns: list[str]) -> bool:
    text = path.as_posix()
    return any(fnmatch.fnmatch(text, pat) for pat in patterns)


def discover_units(
    arch_bin: Path,
    root: Path,
    copied_tests: Path,
    original_tests: Path,
    patterns: list[str],
    timeout: float | None,
    logs_root: Path,
    no_group_dirs: bool,
) -> tuple[list[Unit], list[UnitResult]]:
    all_arch = sorted(copied_tests.rglob("*.arch"))
    if patterns:
        all_arch = [p for p in all_arch if matches_any(p.relative_to(copied_tests), patterns)]

    by_dir: dict[Path, list[Path]] = {}
    for path in all_arch:
        by_dir.setdefault(path.parent, []).append(path)

    units: list[Unit] = []
    skipped: list[UnitResult] = []

    for directory, files in sorted(by_dir.items()):
        rel_dir = directory.relative_to(copied_tests)
        if len(files) > 1 and not no_group_dirs:
            log_dir = logs_root / "_discover" / safe_unit_name(rel_dir)
            cmd = [str(arch_bin), "check", *[str(p) for p in sorted(files)]]
            step = run_cmd(cmd, root, timeout, log_dir, "check_group")
            if step.status == "pass":
                rel_files = rel_tuple(files, copied_tests)
                orig = tuple(original_tests / p for p in rel_files)
                units.append(Unit(safe_unit_name(rel_dir), tuple(sorted(files)), orig))
                continue

        for path in sorted(files):
            rel = path.relative_to(copied_tests)
            orig = original_tests / rel
            units.append(Unit(safe_unit_name(rel.with_suffix("")), (path,), (orig,)))

    return units, skipped


def write_sim_smoke_tb(sim_dir: Path) -> Path | None:
    headers = sorted(
        p for p in sim_dir.glob("V*.h")
        if p.name not in {"VStructs.h"} and not p.name.startswith("verilated")
    )
    if not headers:
        return None

    lines = [
        "#include <cstdint>",
        "",
    ]
    for header in headers:
        lines.append(f'#include "{header.name}"')
    lines.extend(["", "int main() {"])
    for idx, header in enumerate(headers):
        cls = header.stem
        lines.append(f"  {cls} dut_{idx};")
        lines.append(f"  dut_{idx}.eval();")
    lines.extend(["  return 0;", "}", ""])

    tb = sim_dir / "__arch_sim_smoke_tb.cpp"
    tb.write_text("\n".join(lines))
    return tb


def compile_sim_smoke(sim_dir: Path, log_dir: Path, timeout: float | None) -> StepResult:
    tb = write_sim_smoke_tb(sim_dir)
    if tb is None:
        return StepResult("sim_compile", "skip", 0.0, [], "", "no generated V*.h headers\n")

    cpp_files = [sim_dir / "verilated.cpp", *sorted(sim_dir.glob("V*.cpp")), tb]
    cmd = [
        "g++",
        "-std=c++17",
        "-O0",
        "-fbracket-depth=4096",
        "-Wno-unused-variable",
        "-Wno-unused-parameter",
        "-Wno-parentheses-equality",
        "-I",
        str(sim_dir),
        *[str(p) for p in cpp_files if p.exists()],
        "-o",
        str(sim_dir / "sim_smoke"),
    ]
    return run_cmd(cmd, sim_dir, timeout, log_dir, "sim_compile")


def sv_has_module(path: Path) -> bool:
    text = path.read_text(errors="ignore")
    return re.search(r"(?m)^\s*module\s+\w+", text) is not None


def sibling_sv_deps(unit: Unit) -> list[Path]:
    """Existing same-directory SV files needed to lint standalone ARCH tops.

    Some legacy tests keep submodules as checked-in `.sv` while only the top is
    represented as `.arch`. Exclude stems generated by this unit to avoid module
    redefinition when the unit itself contains the matching ARCH source.
    """
    generated_stems = {path.stem for path in unit.files}
    deps: set[Path] = set()
    for arch_file in unit.files:
        for sv in arch_file.parent.glob("*.sv"):
            if sv.stem not in generated_stems:
                deps.add(sv)
    return sorted(deps)


def load_baseline(path: Path) -> set[str]:
    data = json.loads(path.read_text())
    units = data.get("units", data if isinstance(data, list) else [])
    names: set[str] = set()
    for item in units:
        if isinstance(item, str):
            names.add(item)
        elif isinstance(item, dict) and "name" in item:
            names.add(str(item["name"]))
    return names


def resolve_scratch_path(root: Path, scratch_root: Path, rel_path: str) -> Path:
    path = Path(rel_path)
    if path.is_absolute():
        return path
    scratch_path = scratch_root / path
    if scratch_path.exists():
        return scratch_path
    return root / path


def load_sim_manifest(path: Path | None, root: Path, scratch_root: Path) -> dict[str, SimManifestEntry]:
    if path is None:
        return {}
    manifest_path = path if path.is_absolute() else root / path
    if not manifest_path.exists():
        return {}
    data = json.loads(manifest_path.read_text())
    entries = data.get("entries", data if isinstance(data, list) else [])
    manifest: dict[str, SimManifestEntry] = {}
    for item in entries:
        name = str(item["name"])
        arch_files = tuple(resolve_scratch_path(root, scratch_root, str(p)) for p in item.get("arch_files", []))
        tb_files = tuple(resolve_scratch_path(root, scratch_root, str(p)) for p in item.get("tb_files", []))
        args = tuple(str(arg) for arg in item.get("args", []))
        manifest[name] = SimManifestEntry(name, arch_files, tb_files, args)
    return manifest


def write_baseline(path: Path, results: list[UnitResult]) -> None:
    root = repo_root()

    def portable_files(files: list[str]) -> list[str]:
        portable: list[str] = []
        for file in files:
            file_path = Path(file)
            try:
                portable.append(str(file_path.resolve().relative_to(root)))
            except ValueError:
                portable.append(file)
        return portable

    passing = [
        {
            "name": result.name,
            "files": portable_files(result.files),
        }
        for result in sorted(results, key=lambda r: r.name)
        if result.status == "pass"
    ]
    payload = {
        "description": "ARCH regression units that passed check/build/verilator/arch-sim smoke when this baseline was refreshed.",
        "units": passing,
    }
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2) + "\n")


def run_unit(
    unit: Unit,
    arch_bin: Path,
    root: Path,
    work_dir: Path,
    timeout: float | None,
    verilator_flags: list[str],
    skip_verilator: bool,
    skip_sim: bool,
    skip_sim_compile: bool,
    sim_manifest: dict[str, SimManifestEntry],
) -> UnitResult:
    start = time.monotonic()
    unit_dir = work_dir / "units" / unit.name
    log_dir = unit_dir / "logs"
    unit_dir.mkdir(parents=True, exist_ok=True)

    steps: list[StepResult] = []
    files = [str(p) for p in unit.files]

    check = run_cmd([str(arch_bin), "check", *files], root, timeout, log_dir, "check")
    steps.append(check)
    if check.status != "pass":
        return UnitResult(unit.name, [str(p) for p in unit.original_files], "skip_check_failed", time.monotonic() - start, steps)

    sv_out = unit_dir / f"{unit.name}.sv"
    build = run_cmd([str(arch_bin), "build", *files, "-o", str(sv_out)], root, timeout, log_dir, "build")
    steps.append(build)
    if build.status != "pass":
        return UnitResult(unit.name, [str(p) for p in unit.original_files], "fail", time.monotonic() - start, steps)

    has_module = sv_has_module(sv_out)
    if not skip_verilator:
        if has_module:
            vl = run_cmd(
                ["verilator", *verilator_flags, str(sv_out), *[str(p) for p in sibling_sv_deps(unit)]],
                root,
                timeout,
                log_dir,
                "verilator_lint",
            )
        else:
            vl = StepResult("verilator_lint", "skip", 0.0, [], "", "generated SV has no module declarations\n")
        steps.append(vl)
        if vl.status not in {"pass", "skip"}:
            return UnitResult(unit.name, [str(p) for p in unit.original_files], "fail", time.monotonic() - start, steps)

    if not skip_sim:
        if has_module:
            sim_dir = unit_dir / "arch_sim"
            manifest_entry = sim_manifest.get(unit.name)
            if manifest_entry is not None:
                sim_cmd = [
                    str(arch_bin),
                    "sim",
                    *[str(path) for path in manifest_entry.arch_files],
                    "--tb",
                    *[str(path) for path in manifest_entry.tb_files],
                    "--outdir",
                    str(sim_dir),
                    *manifest_entry.args,
                ]
            else:
                sim_cmd = [str(arch_bin), "sim", *files, "--outdir", str(sim_dir)]
            sim = run_cmd(sim_cmd, root, timeout, log_dir, "arch_sim")
            steps.append(sim)
            if sim.status != "pass":
                return UnitResult(unit.name, [str(p) for p in unit.original_files], "fail", time.monotonic() - start, steps)

            if manifest_entry is None and not skip_sim_compile:
                sim_compile = compile_sim_smoke(sim_dir, log_dir, timeout)
                steps.append(sim_compile)
                if sim_compile.status not in {"pass", "skip"}:
                    return UnitResult(unit.name, [str(p) for p in unit.original_files], "fail", time.monotonic() - start, steps)
        else:
            steps.append(StepResult("arch_sim", "skip", 0.0, [], "", "generated SV has no module declarations\n"))

    return UnitResult(unit.name, [str(p) for p in unit.original_files], "pass", time.monotonic() - start, steps)


def result_to_json(result: UnitResult) -> dict:
    return {
        "name": result.name,
        "files": result.files,
        "status": result.status,
        "seconds": result.seconds,
        "steps": [
            {
                "name": step.name,
                "status": step.status,
                "seconds": step.seconds,
                "command": step.command,
            }
            for step in result.steps
        ],
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--arch-bin", help="Path to an existing arch binary. Defaults to target/debug/arch, building it if needed.")
    parser.add_argument("--release", action="store_true", help="Use/build target/release/arch by default.")
    parser.add_argument("--tests-dir", default="tests", type=Path, help="Tests tree to scan, relative to the repo root.")
    parser.add_argument("--work-dir", type=Path, help="Scratch directory. Defaults to a new /tmp directory.")
    parser.add_argument("--pattern", action="append", default=[], help="Only run units whose tests-dir-relative path matches this glob. Repeatable.")
    parser.add_argument("--jobs", type=int, default=max(1, min(4, (os.cpu_count() or 2) // 2)), help="Number of units to run concurrently.")
    parser.add_argument("--timeout", type=float, default=120.0, help="Per-step timeout in seconds. Use 0 for no timeout.")
    parser.add_argument("--limit", type=int, help="Run at most N discovered units.")
    parser.add_argument("--list", action="store_true", help="List discovered units and exit after check-based grouping.")
    parser.add_argument("--baseline", type=Path, help="Run only unit names listed in a JSON baseline.")
    parser.add_argument("--update-baseline", type=Path, help="Write a JSON baseline containing units that passed this run.")
    parser.add_argument("--sim-manifest", type=Path, default=Path("tests/arch_sim_manifest.json"), help="JSON manifest mapping unit names to C++ arch-sim testbenches.")
    parser.add_argument("--allow-failures", action="store_true", help="Exit 0 even if backend failures are found. Useful when refreshing a baseline.")
    parser.add_argument("--no-group-dirs", action="store_true", help="Do not collapse directories whose .arch files pass together into one unit.")
    parser.add_argument("--skip-verilator", action="store_true", help="Skip Verilator lint.")
    parser.add_argument("--skip-sim", action="store_true", help="Skip arch sim model generation.")
    parser.add_argument("--skip-sim-compile", action="store_true", help="Skip generated C++ smoke compile for arch sim models.")
    parser.add_argument("--verilator-flag", action="append", default=[], help="Append an extra Verilator flag. Repeatable.")
    parser.add_argument("--keep-work-dir", action="store_true", help="Keep the auto-created scratch directory after success.")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    root = repo_root()
    timeout = None if args.timeout == 0 else args.timeout
    arch_bin = ensure_arch_bin(root, args.arch_bin, args.release)

    auto_work_dir = args.work_dir is None
    work_dir = args.work_dir or Path(tempfile.mkdtemp(prefix="arch-regression-"))
    work_dir = work_dir.resolve()
    work_dir.mkdir(parents=True, exist_ok=True)
    logs_root = work_dir / "logs"

    copied_tests = copy_tests_tree(root, work_dir, args.tests_dir)
    scratch_root = work_dir / "src"
    sim_manifest = load_sim_manifest(args.sim_manifest, root, scratch_root)
    original_tests = root / args.tests_dir
    units, skipped = discover_units(
        arch_bin,
        root,
        copied_tests,
        original_tests,
        args.pattern,
        timeout,
        logs_root,
        args.no_group_dirs,
    )
    if args.limit is not None:
        units = units[: args.limit]
    if args.baseline:
        baseline_names = load_baseline(args.baseline)
        before = len(units)
        units = [unit for unit in units if unit.name in baseline_names]
        missing = sorted(baseline_names - {unit.name for unit in units})
        print(f"[setup] baseline={args.baseline} selected={len(units)}/{before}")
        if missing:
            print(f"[setup] warning: {len(missing)} baseline units were not discovered; first few: {', '.join(missing[:5])}")

    print(f"[setup] repo={root}")
    print(f"[setup] arch={arch_bin}")
    print(f"[setup] work_dir={work_dir}")
    print(f"[setup] sim_manifest_entries={len(sim_manifest)}")
    print(f"[setup] units={len(units)} jobs={args.jobs}")

    if args.list:
        for unit in units:
            print(f"{unit.name}:")
            for path in unit.original_files:
                print(f"  {path.relative_to(root)}")
        return 0

    verilator_flags = [*DEFAULT_VERILATOR_FLAGS, *args.verilator_flag]
    results: list[UnitResult] = [*skipped]
    start = time.monotonic()

    with concurrent.futures.ThreadPoolExecutor(max_workers=max(1, args.jobs)) as pool:
        future_map = {
            pool.submit(
                run_unit,
                unit,
                arch_bin,
                root,
                work_dir,
                timeout,
                verilator_flags,
                args.skip_verilator,
                args.skip_sim,
                args.skip_sim_compile,
                sim_manifest,
            ): unit
            for unit in units
        }
        done_count = 0
        for future in concurrent.futures.as_completed(future_map):
            unit = future_map[future]
            done_count += 1
            try:
                result = future.result()
            except Exception as exc:  # pragma: no cover - defensive runner guard.
                result = UnitResult(unit.name, [str(p) for p in unit.original_files], "error", 0.0, [
                    StepResult("runner", "error", 0.0, [], "", repr(exc)),
                ])
            results.append(result)
            print(f"[{done_count:4d}/{len(units):4d}] {result.status:18s} {result.name} ({result.seconds:.2f}s)", flush=True)

    summary = {
        "repo": str(root),
        "arch_bin": str(arch_bin),
        "work_dir": str(work_dir),
        "seconds": time.monotonic() - start,
        "counts": {},
        "results": [result_to_json(r) for r in results],
    }
    for result in results:
        summary["counts"][result.status] = summary["counts"].get(result.status, 0) + 1

    summary_path = work_dir / "summary.json"
    summary_path.write_text(json.dumps(summary, indent=2) + "\n")
    print(f"[summary] {summary['counts']}")
    print(f"[summary] wrote {summary_path}")
    if args.update_baseline:
        write_baseline(args.update_baseline, results)
        print(f"[summary] wrote baseline {args.update_baseline}")

    failures = [r for r in results if r.status not in {"pass", "skip_check_failed"}]
    if failures and not args.allow_failures:
        print("[failures]")
        for result in failures[:25]:
            failed_steps = [s for s in result.steps if s.status not in {"pass", "skip"}]
            step_name = failed_steps[-1].name if failed_steps else "runner"
            print(f"  {result.name}: {result.status} at {step_name}")
        return 1

    if auto_work_dir and not args.keep_work_dir:
        print(f"[cleanup] keeping {work_dir} for logs; remove it when no longer needed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
