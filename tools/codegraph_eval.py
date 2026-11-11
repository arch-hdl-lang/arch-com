#!/usr/bin/env python3
"""Run and analyze paired, unguided ARCH graph-discovery evaluations."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import random
import re
import subprocess
import tempfile
import time
from pathlib import Path


GRAPH_COMMAND = re.compile(r"(?:^|[\s/])(?:arch|cargo\s+run\s+--|target/(?:debug|release)/arch)\s+graph\b")
NO_GRAPH = "Graph commands and graph MCP tools are unavailable for this task. Use ordinary repository search."


def load_tasks(path: Path) -> list[dict]:
    data = json.loads(path.read_text())
    if data.get("schema_version") != 1 or not isinstance(data.get("tasks"), list):
        raise ValueError("unsupported task schema")
    ids = [task["id"] for task in data["tasks"]]
    if len(ids) != len(set(ids)):
        raise ValueError("duplicate task id")
    for task in data["tasks"]:
        if task["cohort"] not in ("user", "compiler") or task["kind"] not in (
            "discovery", "known_file", "coverage_miss", "edit"
        ):
            raise ValueError(f"invalid cohort or kind: {task['id']}")
    return data["tasks"]


def parse_events(raw: str) -> list[dict]:
    return [json.loads(line) for line in raw.splitlines() if line.startswith("{")]


def trace_result(events: list[dict]) -> dict:
    usage = [event["usage"] for event in events if event.get("type") == "turn.completed"]
    completed = [event["item"] for event in events if event.get("type") == "item.completed"]
    answers = [item.get("text", "") for item in completed if item.get("type") == "agent_message"]
    commands = [item.get("command", "") for item in completed if item.get("type") == "command_execution"]
    mcp_calls = [item for item in completed if item.get("type") == "mcp_tool_call"]
    graph_calls = sum(bool(GRAPH_COMMAND.search(command)) for command in commands)
    graph_calls += sum(
        bool(re.search(r"arch_(?:dev_)?graph_", json.dumps(item))) for item in mcp_calls
    )
    return {
        "answer": answers[-1] if answers else "",
        "commands": commands,
        "tool_calls": len(commands) + len(mcp_calls),
        "graph_calls": graph_calls,
        "input_tokens": sum(item["input_tokens"] for item in usage),
        "cached_input_tokens": sum(item.get("cached_input_tokens", 0) for item in usage),
        "output_tokens": sum(item["output_tokens"] for item in usage),
        "usage_present": bool(usage),
    }


def answer_passes(task: dict, answer: str) -> bool:
    haystack = answer.casefold()
    def matches(choice: str) -> bool:
        if choice in ("no", "none"):
            return bool(re.search(rf"\b{choice}\b", haystack))
        return choice.casefold() in haystack
    return all(any(matches(choice) for choice in group) for group in task.get("answer_any", []))


def edit_passes(task: dict, workspace: Path, env: dict[str, str]) -> tuple[bool, str]:
    changed = subprocess.check_output(
        ["git", "diff", "--name-only"], cwd=workspace, text=True
    ).splitlines()
    untracked = subprocess.check_output(
        ["git", "ls-files", "--others", "--exclude-standard"], cwd=workspace, text=True
    ).splitlines()
    changed.extend(untracked)
    if not changed or set(changed) != set(task["allowed_changes"]):
        return False, f"changed paths: {changed}"
    for relative, snippets in task.get("file_contains", {}).items():
        source = (workspace / relative).read_text()
        if not all(snippet in source for snippet in snippets):
            return False, f"missing required snippet in {relative}"
    diff = subprocess.check_output(
        ["git", "diff", "--unified=0", "--", *changed], cwd=workspace, text=True
    )
    added = [line[1:] for line in diff.splitlines() if line.startswith("+") and not line.startswith("+++")]
    removed = [line for line in diff.splitlines() if line.startswith("-") and not line.startswith("---")]
    if task.get("only_comments") and (removed or not added or any(not line.lstrip().startswith("//") for line in added)):
        return False, "edit changed non-comment source lines"
    if any(not any(term.casefold() in line.casefold() for line in added) for term in task.get("added_line_contains", [])):
        return False, "required comment was not added"
    verified = subprocess.run(task["verify"], cwd=workspace, env=env, capture_output=True, text=True, timeout=180)
    if verified.returncode:
        return False, f"validation failed: {verified.stderr[-500:]}"
    return True, "validated"


def run_command(command: list[str], cwd: Path, log: Path) -> float:
    start = time.monotonic()
    result = subprocess.run(command, cwd=cwd, capture_output=True, text=True, timeout=180)
    log.write_text(result.stdout + result.stderr)
    if result.returncode:
        raise RuntimeError(f"{' '.join(command)} failed; see {log}")
    return round(time.monotonic() - start, 3)


def prepare_indexes(workspace: Path, arch_bin: Path, cohort: str, log_dir: Path, label: str) -> float:
    if cohort == "user":
        inputs = [name for name in ("examples", "stdlib", "tests") if (workspace / name).is_dir()]
        command = [str(arch_bin), "graph", "index", *inputs, "--out", ".archgraph", "--clean"]
    else:
        command = [str(arch_bin), "graph", "dev-index", "--root", ".", "--out", ".archdevgraph"]
    return run_command(command, workspace, log_dir / f"{label}-index.log")


def make_mcp_config(workspace: Path, arch_bin: Path, mcp_python: Path) -> str:
    server = workspace / "mcp/arch_mcp_server.py"
    return (
        'mcp_servers.arch-hdl={command="' + str(mcp_python) + '",args=["' + str(server)
        + '"],env={ARCH_BIN="' + str(arch_bin) + '",ARCH_MCP_WORKSPACE_ROOTS="'
        + str(workspace) + '"}}'
    )


def run_attempt(task: dict, condition: str, repeat: int, repo: Path, output: Path,
                arch_bin: Path, mcp_python: Path, model: str, timeout: int,
                ignore_user_config: bool) -> dict:
    label = f"{task['id']}-r{repeat}-{condition}"
    if task["kind"] == "edit":
        temp = tempfile.TemporaryDirectory(prefix="arch-graph-eval-", dir=output)
        workspace = Path(temp.name) / "repo"
        subprocess.run(["git", "clone", "--shared", "--quiet", str(repo), str(workspace)], check=True)
        index_seconds = prepare_indexes(workspace, arch_bin, task["cohort"], output, label)
    else:
        temp = None
        workspace = repo
        index_seconds = 0.0
    try:
        prompt = task["prompt"]
        if condition == "no_graph":
            prompt += " " + NO_GRAPH
        if task["kind"] == "edit":
            prompt += " Work only inside this checkout."
        command = [
            "codex", "exec", "--json", "-m", model,
            "-c", 'model_reasoning_effort="low"', "-c", make_mcp_config(workspace, arch_bin, mcp_python),
            "-s", "workspace-write" if task["kind"] == "edit" else "read-only",
            "-C", str(workspace), "--ephemeral", prompt,
        ]
        if ignore_user_config:
            command.insert(3, "--ignore-user-config")
        env = os.environ.copy()
        env["PATH"] = str(arch_bin.parent) + os.pathsep + env.get("PATH", "")
        env["CARGO_TARGET_DIR"] = str(repo / "target")
        start = time.monotonic()
        try:
            result = subprocess.run(command, cwd=workspace, env=env, capture_output=True, text=True, timeout=timeout)
            exit_code, raw, stderr = result.returncode, result.stdout, result.stderr
        except subprocess.TimeoutExpired as exc:
            exit_code, raw, stderr = 124, (exc.stdout or b"").decode(errors="replace"), "attempt timed out"
        elapsed = round(time.monotonic() - start, 3)
        (output / f"{label}.jsonl").write_text(raw)
        (output / f"{label}.stderr.txt").write_text(stderr)
        trace = trace_result(parse_events(raw))
        if task["kind"] == "edit":
            outcome, note = edit_passes(task, workspace, env)
            (output / f"{label}.diff").write_text(
                subprocess.check_output(["git", "diff"], cwd=workspace, text=True)
            )
        else:
            outcome, note = answer_passes(task, trace["answer"]), "answer rubric"
        valid_method = condition != "no_graph" or trace["graph_calls"] == 0
        return {
            "task_id": task["id"], "cohort": task["cohort"], "kind": task["kind"],
            "condition": condition, "repeat": repeat, "exit_code": exit_code,
            "success": exit_code == 0 and trace["usage_present"] and valid_method and outcome,
            "valid_method": valid_method, "grader_note": note,
            "input_tokens": trace["input_tokens"], "cached_input_tokens": trace["cached_input_tokens"],
            "output_tokens": trace["output_tokens"], "tool_calls": trace["tool_calls"],
            "graph_calls": trace["graph_calls"], "agent_seconds": elapsed,
            "index_seconds": index_seconds, "answer": trace["answer"],
        }
    finally:
        if temp is not None:
            temp.cleanup()


def summarize(records: list[dict]) -> dict:
    groups = {}
    for cohort in ("user", "compiler", "all"):
        subset = [r for r in records if cohort == "all" or r["cohort"] == cohort]
        arms = {}
        for condition in ("no_graph", "default"):
            arm = [r for r in subset if r["condition"] == condition]
            arms[condition] = {
                "attempts": len(arm), "successes": sum(r["success"] for r in arm),
                "tokens": sum(r["input_tokens"] + r["output_tokens"] for r in arm),
                "uncached_tokens": sum(r["input_tokens"] - r["cached_input_tokens"] + r["output_tokens"] for r in arm),
                "graph_calls": sum(r["graph_calls"] for r in arm),
                "agent_seconds": round(sum(r["agent_seconds"] for r in arm), 3),
                "index_seconds": round(sum(r["index_seconds"] for r in arm), 3),
            }
        paired = {(r["task_id"], r["repeat"]): r for r in subset if r["condition"] == "no_graph"}
        pairs = [(base, r) for r in subset if r["condition"] == "default"
                 if (base := paired.get((r["task_id"], r["repeat"]))) is not None
                 and base["success"] and r["success"]]
        baseline_tokens = sum(a["input_tokens"] + a["output_tokens"] for a, _ in pairs)
        default_tokens = sum(b["input_tokens"] + b["output_tokens"] for _, b in pairs)
        groups[cohort] = {
            "arms": arms, "successful_pairs": len(pairs),
            "paired_token_saving_pct": round(100 * (1 - default_tokens / baseline_tokens), 2) if baseline_tokens else None,
        }
    return groups


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__)
    sub = p.add_subparsers(dest="action", required=True)
    run = sub.add_parser("run")
    run.add_argument("--repo", type=Path, default=Path(__file__).resolve().parents[1])
    run.add_argument("--tasks", type=Path, default=Path(__file__).resolve().parents[1] / "doc/codegraph_eval_tasks.json")
    run.add_argument("--output", type=Path, required=True)
    run.add_argument("--arch-bin", type=Path, required=True)
    run.add_argument("--mcp-python", type=Path, required=True)
    run.add_argument("--model", default="gpt-6-luna")
    run.add_argument("--repeats", type=int, default=1)
    run.add_argument("--seed", type=int, default=7)
    run.add_argument("--timeout", type=int, default=300)
    run.add_argument("--task", action="append")
    run.add_argument("--ignore-user-config", action="store_true")
    analyze = sub.add_parser("analyze")
    analyze.add_argument("records", type=Path)
    args = p.parse_args()
    if args.action == "analyze":
        print(json.dumps(summarize([json.loads(line) for line in args.records.read_text().splitlines()]), indent=2))
        return
    repo, arch_bin, mcp_python = args.repo.resolve(), args.arch_bin.resolve(), args.mcp_python.absolute()
    if not arch_bin.is_file() or not mcp_python.is_file():
        p.error("--arch-bin and --mcp-python must be existing files")
    tasks = load_tasks(args.tasks)
    if args.task:
        unknown = set(args.task) - {task["id"] for task in tasks}
        if unknown:
            p.error(f"unknown tasks: {sorted(unknown)}")
        tasks = [task for task in tasks if task["id"] in args.task]
    if args.repeats < 1 or not tasks:
        p.error("at least one task and repeat are required")
    args.output.mkdir(parents=True, exist_ok=True)
    records_path = args.output / "records.jsonl"
    if records_path.exists():
        p.error(f"refusing to overwrite {records_path}")
    metadata = {
        "repo_sha": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip(),
        "repo_status": subprocess.check_output(["git", "status", "--porcelain"], cwd=repo, text=True).strip(),
        "tasks_sha256": hashlib.sha256(args.tasks.read_bytes()).hexdigest(),
        "model": args.model, "seed": args.seed, "repeats": args.repeats,
        "ignore_user_config": args.ignore_user_config,
        "codex_version": subprocess.check_output(["codex", "--version"], text=True).strip(),
        "arch_version": subprocess.check_output([str(arch_bin), "--version"], text=True).strip(),
    }
    if metadata["repo_status"]:
        p.error("benchmark repository must be clean before running")
    setup = {}
    for cohort in {task["cohort"] for task in tasks if task["kind"] != "edit"}:
        setup[cohort] = prepare_indexes(repo, arch_bin, cohort, args.output, f"shared-{cohort}")
    metadata["shared_index_seconds"] = setup
    (args.output / "metadata.json").write_text(json.dumps(metadata, indent=2) + "\n")
    schedule = [(task, repeat, condition) for task in tasks
                for repeat in range(1, args.repeats + 1)
                for condition in ("no_graph", "default")]
    random.Random(args.seed).shuffle(schedule)
    with records_path.open("w") as records:
        for task, repeat, condition in schedule:
            record = run_attempt(task, condition, repeat, repo, args.output, arch_bin, mcp_python,
                                 args.model, args.timeout, args.ignore_user_config)
            records.write(json.dumps(record) + "\n")
            records.flush()
            print(f"{task['id']} r{repeat} {condition}: correct={record['success']} "
                  f"graph_calls={record['graph_calls']} "
                  f"tokens={record['input_tokens'] + record['output_tokens']}", flush=True)
    (args.output / "summary.json").write_text(json.dumps(summarize([
        json.loads(line) for line in records_path.read_text().splitlines()
    ]), indent=2) + "\n")


if __name__ == "__main__":
    main()
