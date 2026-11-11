#!/usr/bin/env python3
"""Check graph retrieval and output bounds without launching an agent."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import tempfile
import time
from pathlib import Path

from codegraph_eval import load_tasks


CASES = (
    {
        "id": "user_fifo_lookup",
        "kind": "context",
        "expected": (("examples/sync_fifo.arch", "TxQueue"),),
        "max_rank": 1,
    },
    {
        "id": "user_bus_relationship",
        "kind": "context",
        "expected": (("stdlib/BusAxiLite.arch", "BusAxiLite"),
                     ("tests/axi_dma/BusAxiLite.arch", "BusAxiLite")),
        "max_rank": 4,
    },
    {
        "id": "user_arbiter_reset",
        "query": "Which arbiter grants requests in round-robin order and what happens to the grant on reset?",
        "kind": "context",
        "expected": (("tests/cvdp/round_robin_arbiter.arch", "round_robin_arbiter"),),
        "max_rank": 2,
    },
    {
        "id": "compiler_arbiter_owners",
        "kind": "dev-query",
        "expected": (("src/ast.rs", "ArbiterDecl"), ("src/parser.rs", "parse_arbiter"),
                     ("src/codegen/arbiter.rs", "emit_arbiter_round_robin")),
        "max_rank": 4,
    },
    {
        "id": "compiler_pipe_reg_owners",
        "kind": "dev-query",
        "expected": (("src/ast.rs", "PipeRegDecl"), ("src/parser.rs", "parse_module"),
                     ("src/codegen/module.rs", "emit_pipe_reg")),
        "max_rank": 4,
    },
)
TOKEN_BUDGET = 100


def run(command: list[str], root: Path, timeout: int = 120) -> tuple[str, float]:
    start = time.monotonic()
    result = subprocess.run(command, cwd=root, capture_output=True, text=True, timeout=timeout)
    seconds = round(time.monotonic() - start, 3)
    if result.returncode:
        raise RuntimeError(f"{' '.join(command)} failed: {result.stderr[-1000:]}")
    return result.stdout, seconds


def grade(output: str, expected: tuple[tuple[str, str], ...], max_rank: int,
          root: Path) -> tuple[bool, list[int | None]]:
    lines = output.splitlines()
    ranks = []
    for path, symbol in expected:
        pattern = re.compile(rf"{re.escape(path)}:(\d+)\s+.*\b{re.escape(symbol)}\b")
        source = (root / path).read_text().splitlines()
        rank = None
        for position, line in enumerate(lines, 1):
            match = pattern.search(line)
            if match and 0 < int(match.group(1)) <= len(source) and symbol in source[int(match.group(1)) - 1]:
                rank = position
                break
        ranks.append(rank)
    return all(rank is not None and rank <= max_rank for rank in ranks), ranks


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--arch-bin", type=Path, required=True)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    root, arch_bin = args.root.resolve(), args.arch_bin.resolve()
    if not arch_bin.is_file():
        parser.error(f"compiler binary not found: {arch_bin}")
    tasks = {task["id"]: task for task in load_tasks(root / "doc/codegraph_eval_tasks.json")}
    missing = {case["id"] for case in CASES if "query" not in case} - tasks.keys()
    if missing:
        parser.error(f"missing discovery tasks: {sorted(missing)}")
    with tempfile.TemporaryDirectory(prefix="arch-graph-direct-") as directory:
        user_index = Path(directory) / "user"
        dev_index = Path(directory) / "developer"
        _, user_index_seconds = run([str(arch_bin), "graph", "index", "examples", "stdlib", "tests",
                                     "--root", str(root), "--out", str(user_index), "--clean"], root, 180)
        _, dev_index_seconds = run([str(arch_bin), "graph", "dev-index", "--root", str(root),
                                    "--out", str(dev_index)], root)
        records = []
        for case in CASES:
            query = case.get("query") or tasks[case["id"]]["prompt"]
            if case["kind"] == "context":
                command = [str(arch_bin), "graph", "context", query,
                           "--index", str(user_index), "--token-budget", str(TOKEN_BUDGET)]
            else:
                command = [str(arch_bin), "graph", "dev-query", query,
                           "--root", str(root), "--index", str(dev_index),
                           "--token-budget", str(TOKEN_BUDGET)]
            output, seconds = run(command, root)
            passed, ranks = grade(output, case["expected"], case["max_rank"], root)
            bounded = len(output) <= TOKEN_BUDGET * 4
            records.append({"id": case["id"], "pass": passed and bounded, "ranks": ranks,
                            "seconds": seconds, "output_chars": len(output), "bounded": bounded,
                            **({"output": output} if not passed else {})})
        print(json.dumps({"index_seconds": {"user": user_index_seconds,
                                            "developer": dev_index_seconds},
                          "cases": records}, indent=2))
        if not all(record["pass"] for record in records):
            raise SystemExit(1)


if __name__ == "__main__":
    main()
