#!/usr/bin/env python3
"""Run `cargo test` while guarding the expected Rust test inventory."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import re
import subprocess
import sys


def repo_root() -> Path:
    return Path(__file__).resolve().parents[1]


def run(command: list[str], cwd: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        command,
        cwd=cwd,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )


def list_tests(root: Path) -> list[dict[str, str]]:
    proc = subprocess.run(
        ["cargo", "test", "--", "--list"],
        cwd=root,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    if proc.returncode != 0:
        sys.stderr.write(proc.stdout)
        raise SystemExit(proc.returncode)

    tests: list[dict[str, str]] = []
    target = "unknown"
    for line in proc.stdout.splitlines():
        stripped = line.strip()
        if stripped.startswith("Running "):
            match = re.match(r"Running\s+(.+?)\s+\(", stripped)
            target = match.group(1) if match else stripped
            continue
        if stripped.startswith("Doc-tests "):
            target = stripped
            continue
        if stripped.endswith(": test"):
            name = stripped.removesuffix(": test")
            tests.append({"target": target, "name": name})
    return tests


def load_baseline(path: Path) -> set[tuple[str, str]]:
    data = json.loads(path.read_text())
    entries = data.get("tests", data if isinstance(data, list) else [])
    return {(str(item["target"]), str(item["name"])) for item in entries}


def write_baseline(path: Path, tests: list[dict[str, str]]) -> None:
    payload = {
        "description": "Rust tests discovered by `cargo test -- --list` when this baseline was refreshed.",
        "tests": sorted(tests, key=lambda item: (item["target"], item["name"])),
    }
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2) + "\n")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--baseline",
        type=Path,
        default=Path("tests/cargo_test_baseline.json"),
        help="JSON baseline to compare against.",
    )
    parser.add_argument(
        "--update-baseline",
        action="store_true",
        help="Refresh the baseline from the current `cargo test -- --list` output.",
    )
    parser.add_argument(
        "--allow-new",
        action="store_true",
        help="Allow tests not yet listed in the baseline. Missing baseline tests still fail.",
    )
    parser.add_argument(
        "--list-only",
        action="store_true",
        help="Only compare/update the inventory; do not run `cargo test`.",
    )
    parser.add_argument(
        "--",
        dest="cargo_args",
        nargs=argparse.REMAINDER,
        help="Extra arguments passed to `cargo test`.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    root = repo_root()
    baseline = (root / args.baseline).resolve() if not args.baseline.is_absolute() else args.baseline
    tests = list_tests(root)
    current = {(item["target"], item["name"]) for item in tests}

    if args.update_baseline:
        write_baseline(baseline, tests)
        print(f"[baseline] wrote {baseline} with {len(tests)} tests")
    elif baseline.exists():
        expected = load_baseline(baseline)
        missing = sorted(expected - current)
        new = sorted(current - expected)
        print(f"[baseline] expected={len(expected)} current={len(current)}")
        if missing:
            print("[baseline] missing tests:")
            for target, name in missing[:50]:
                print(f"  {target} :: {name}")
            if len(missing) > 50:
                print(f"  ... and {len(missing) - 50} more")
            return 1
        if new and not args.allow_new:
            print("[baseline] new tests not in baseline:")
            for target, name in new[:50]:
                print(f"  {target} :: {name}")
            if len(new) > 50:
                print(f"  ... and {len(new) - 50} more")
            print("[baseline] rerun with --update-baseline after reviewing the new tests")
            return 1
        if new:
            print(f"[baseline] warning: {len(new)} new tests are not in the baseline")
    else:
        print(f"[baseline] {baseline} does not exist; run with --update-baseline")
        return 1

    if args.list_only:
        return 0

    command = ["cargo", "test", *(args.cargo_args or [])]
    print(f"[run] {' '.join(command)}", flush=True)
    proc = subprocess.run(command, cwd=root)
    return proc.returncode


if __name__ == "__main__":
    raise SystemExit(main())
