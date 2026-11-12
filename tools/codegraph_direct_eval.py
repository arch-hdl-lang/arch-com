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

from codegraph_eval import answer_passes, load_tasks


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
SOURCE_TOKEN_BUDGET = 200
CROSS_TOKEN_BUDGET = 100
CROSS_RELATIONS = {
    "user_axi_dma_instance_targets": (
        ("tests/axi_dma/AxiDmaTop.arch", 131, "mm2s_fsm", "tests/axi_dma/FsmMm2s.arch", 3, "FsmMm2s"),
        ("tests/axi_dma/AxiDmaTop.arch", 139, "mm2s_fifo", "tests/axi_dma/Mm2sFifo.arch", 2, "Mm2sFifo"),
    ),
    "user_package_import_target": (
        ("examples/pkg_user.arch", 1, "TestPkg", "examples/TestPkg.arch", 1, "TestPkg"),
    ),
    "user_nic400_cdc_fifo_targets": (
        ("examples/nic400/Nic400CdcAxi4.arch", 57, "ar_fifo",
         "examples/nic400/Nic400ArCdcFifo.arch", 17, "Nic400ArCdcFifo"),
        ("examples/nic400/Nic400CdcAxi4.arch", 69, "r_fifo",
         "examples/nic400/Nic400RCdcFifo.arch", 16, "Nic400RCdcFifo"),
    ),
}
RELATION_MAX_RANK = {"user_axi_dma_instance_targets": 2,
                     "user_package_import_target": 4,
                     "user_nic400_cdc_fifo_targets": 2}


def run(command: list[str], root: Path, timeout: int = 120) -> tuple[str, float]:
    start = time.monotonic()
    result = subprocess.run(command, cwd=root, capture_output=True, text=True, timeout=timeout)
    seconds = round(time.monotonic() - start, 3)
    if result.returncode:
        raise RuntimeError(f"{' '.join(command)} failed: {result.stderr[-1000:]}")
    return result.stdout, seconds


def grade(output: str, expected: tuple[tuple[str, str], ...], max_rank: int,
          root: Path, include_source: bool = False) -> tuple[bool, list[int | None]]:
    lines = output.splitlines()
    ranks = []
    for path, symbol in expected:
        pattern = re.compile(rf"{re.escape(path)}:(\d+)\s+.*\b{re.escape(symbol)}\b")
        source = (root / path).read_text().splitlines()
        rank = None
        for position, line in enumerate(lines, 1):
            match = pattern.search(line)
            if not match or not 0 < int(match.group(1)) <= len(source):
                continue
            source_line = source[int(match.group(1)) - 1]
            excerpt = " ".join(source_line.split())[:120]
            if symbol in source_line and (not include_source or line.endswith(" | " + excerpt)):
                rank = position
                break
        ranks.append(rank)
    return all(rank is not None and rank <= max_rank for rank in ranks), ranks


def grade_relations(output: str, relations: tuple[tuple[str, int, str, str, int, str], ...],
                    root: Path, max_rank: int = 4) -> bool:
    for parent, parent_line, instance, child, child_line, target in relations:
        source_line = (root / parent).read_text().splitlines()[parent_line - 1]
        target_line = (root / child).read_text().splitlines()[child_line - 1]
        if instance not in source_line or target not in target_line:
            return False
        if not any(line.startswith(f"{parent}:{parent_line} ") and instance in line and
                   f"-> {child}:{child_line} {target}" in line
                   for line in output.splitlines()[:max_rank]):
            return False
    return True


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--arch-bin", type=Path, required=True)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    root, arch_bin = args.root.resolve(), args.arch_bin.resolve()
    if not arch_bin.is_file():
        parser.error(f"compiler binary not found: {arch_bin}")
    tasks = {task["id"]: task for task in load_tasks(root / "doc/codegraph_eval_tasks.json")}
    cross_tasks = {task["id"]: task for task in load_tasks(
        root / "doc/codegraph_eval_cross_file_tasks.json")}
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
            for include_source in (False, True):
                query = case.get("query") or tasks[case["id"]]["prompt"]
                budget = SOURCE_TOKEN_BUDGET if include_source else TOKEN_BUDGET
                if case["kind"] == "context":
                    command = [str(arch_bin), "graph", "context", query,
                               "--index", str(user_index), "--token-budget", str(budget)]
                else:
                    command = [str(arch_bin), "graph", "dev-query", query,
                               "--root", str(root), "--index", str(dev_index),
                               "--token-budget", str(budget)]
                if include_source:
                    command.append("--include-source")
                output, seconds = run(command, root)
                passed, ranks = grade(output, case["expected"], case["max_rank"],
                                      root, include_source)
                bounded = len(output) <= budget * 4
                records.append({"id": case["id"], "include_source": include_source,
                                "pass": passed and bounded, "ranks": ranks,
                                "seconds": seconds, "output_chars": len(output),
                                "bounded": bounded,
                                **({"output": output} if not passed else {})})
        for task_id, relations in CROSS_RELATIONS.items():
            output, seconds = run([str(arch_bin), "graph", "context",
                                   cross_tasks[task_id]["prompt"], "--index", str(user_index),
                                   "--token-budget", str(CROSS_TOKEN_BUDGET),
                                   "--include-source"], root)
            passed = grade_relations(output, relations, root, RELATION_MAX_RANK[task_id])
            bounded = len(output) <= CROSS_TOKEN_BUDGET * 4
            records.append({"id": task_id, "include_source": True,
                            "pass": passed and bounded, "seconds": seconds,
                            "output_chars": len(output), "bounded": bounded,
                            **({"output": output} if not passed else {})})
        compiler_task = cross_tasks["compiler_pipe_reg_change_map"]
        output, seconds = run([str(arch_bin), "graph", "dev-query", compiler_task["prompt"],
                               "--root", str(root), "--index", str(dev_index),
                               "--token-budget", str(SOURCE_TOKEN_BUDGET),
                               "--include-source"], root)
        bounded = len(output) <= SOURCE_TOKEN_BUDGET * 4
        records.append({"id": compiler_task["id"], "include_source": True,
                        "pass": answer_passes(compiler_task, output) and bounded,
                        "seconds": seconds, "output_chars": len(output), "bounded": bounded})
        print(json.dumps({"index_seconds": {"user": user_index_seconds,
                                            "developer": dev_index_seconds},
                          "cases": records}, indent=2))
        if not all(record["pass"] for record in records):
            raise SystemExit(1)


if __name__ == "__main__":
    main()
