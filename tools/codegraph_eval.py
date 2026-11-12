#!/usr/bin/env python3
"""Run and analyze paired, unguided ARCH graph-discovery evaluations."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import random
import re
import shlex
import signal
import subprocess
import tempfile
import time
from pathlib import Path


MEMORY_COMMAND = re.compile(r"/\.codex/memories/")
SEARCH_COMMAND = re.compile(r"(?:^|[\s'\"])(?:rg|grep|find)\b")
SOURCE_READ_COMMAND = re.compile(r"(?:^|[\s'\"])(?:sed|head|tail|cat|nl)\b")
NO_GRAPH = "Graph commands, graph MCP tools, and graph index files are unavailable for this task. Use ordinary repository search."
VERIFY = (
    "Verify with at most three targeted retrieval calls and bounded output, then answer. "
    "For source-location questions, do not run builds or tests. Complete the task rather than only describing a plan."
)


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
        if task.get("verdict") not in (None, "no"):
            raise ValueError(f"invalid verdict: {task['id']}")
    return data["tasks"]


def parse_events(raw: str) -> list[dict]:
    events = []
    for line in raw.splitlines():
        if line.startswith("{"):
            try:
                events.append(json.loads(line))
            except json.JSONDecodeError:
                continue
    return events


def graph_command_kind(command: str) -> str | None:
    try:
        outer = shlex.split(command)
        if outer and Path(outer[0]).name in ("sh", "bash", "zsh"):
            shell_arg = next((i + 1 for i, word in enumerate(outer[:-1])
                              if word in ("-c", "-lc")), None)
            if shell_arg is not None:
                command = outer[shell_arg]
        lexer = shlex.shlex(command, posix=True, punctuation_chars=";&|")
        lexer.whitespace_split = True
        words = list(lexer)
    except ValueError:
        return None
    segment = []
    saw_query = False
    saw_setup = False
    for word in words + [";"]:
        if word in (";", "&&", "||", "|"):
            while segment and re.match(r"^[A-Za-z_][A-Za-z_0-9]*=", segment[0]):
                segment.pop(0)
            if segment and Path(segment[0]).name == "env":
                segment.pop(0)
                while segment and re.match(r"^[A-Za-z_][A-Za-z_0-9]*=", segment[0]):
                    segment.pop(0)
            if len(segment) >= 3 and Path(segment[0]).name == "arch" and segment[1] == "graph":
                if "--help" not in segment and "-h" not in segment:
                    saw_query |= segment[2] in ("context", "dev-query", "query", "impact", "callers")
                    saw_setup |= segment[2] in ("index", "dev-index")
            if segment and Path(segment[0]).name == "cargo" and "--" in segment:
                pos = segment.index("--")
                if segment[pos + 1:pos + 2] == ["graph"] and "--help" not in segment:
                    saw_query |= segment[pos + 2:pos + 3] in (["context"], ["dev-query"], ["query"],
                                                               ["impact"], ["callers"])
                    saw_setup |= segment[pos + 2:pos + 3] in (["index"], ["dev-index"])
            segment = []
        else:
            segment.append(word)
    if saw_query:
        return "cli"
    if saw_setup:
        return "cli_setup"
    if any(re.search(r"(?:^|/)\.arch(?:dev)?graph(?:/|$)", word) for word in words):
        return "index_read"
    return None


def is_graph_command(command: str) -> bool:
    return graph_command_kind(command) is not None


def trace_result(events: list[dict]) -> dict:
    usage = [event["usage"] for event in events if event.get("type") == "turn.completed"]
    completed = [event["item"] for event in events if event.get("type") == "item.completed"]
    answers = [item.get("text", "") for item in completed if item.get("type") == "agent_message"]
    command_items = [item for item in completed if item.get("type") == "command_execution"]
    commands = [item.get("command", "") for item in command_items]
    mcp_calls = [item for item in completed if item.get("type") == "mcp_tool_call"]
    graph_commands = [item for item in command_items if
                      is_graph_command(item.get("command", ""))]
    graph_mcp_calls = [item for item in mcp_calls if re.search(r"arch_(?:dev_)?graph_", item.get("tool", ""))]
    graph_calls = len(graph_commands) + len(graph_mcp_calls)
    successful_graph_commands = sum(item.get("exit_code") == 0 and
                                    graph_command_kind(item.get("command", "")) == "cli"
                                    for item in graph_commands)
    successful_graph_index_reads = sum(item.get("exit_code") == 0 and
                                       graph_command_kind(item.get("command", "")) == "index_read"
                                       for item in graph_commands)
    def mcp_succeeded(item: dict) -> bool:
        if item.get("status") != "completed" or item.get("error"):
            return False
        result = item.get("result") or {}
        content = result.get("content") or []
        return any(part.get("type") == "text" and part.get("text", "").startswith("[OK]")
                   for part in content)

    successful_graph_mcp_calls = sum(mcp_succeeded(item) for item in graph_mcp_calls)
    successful_graph_calls = (successful_graph_commands + successful_graph_index_reads +
                              successful_graph_mcp_calls)
    tool_breakdown = {kind: {"calls": 0, "output_chars": 0} for kind in
                      ("memory", "graph", "search", "source_read", "other")}
    after_graph_calls = 0
    saw_graph = False
    for item in completed:
        if item.get("type") == "command_execution":
            command = item.get("command", "")
            if MEMORY_COMMAND.search(command):
                kind = "memory"
            elif is_graph_command(command):
                kind = "graph"
            elif SEARCH_COMMAND.search(command):
                kind = "search"
            elif SOURCE_READ_COMMAND.search(command):
                kind = "source_read"
            else:
                kind = "other"
            output = item.get("aggregated_output", "")
        elif item.get("type") == "mcp_tool_call":
            kind = "graph" if item in graph_mcp_calls else "other"
            output = "".join(part.get("text", "") for part in
                             (item.get("result") or {}).get("content", [])
                             if part.get("type") == "text")
        else:
            continue
        if saw_graph and kind != "graph":
            after_graph_calls += 1
        saw_graph |= kind == "graph"
        tool_breakdown[kind]["calls"] += 1
        tool_breakdown[kind]["output_chars"] += len(output)
    return {
        "answer": answers[-1] if answers else "",
        "commands": commands,
        "tool_calls": len(commands) + len(mcp_calls),
        "graph_calls": graph_calls,
        "successful_graph_calls": successful_graph_calls,
        "successful_graph_commands": successful_graph_commands,
        "successful_graph_index_reads": successful_graph_index_reads,
        "successful_graph_mcp_calls": successful_graph_mcp_calls,
        "failed_mcp_calls": sum(bool(item.get("status") != "completed" or item.get("error") or
                                (item in graph_mcp_calls and not mcp_succeeded(item)))
                                for item in mcp_calls),
        "input_tokens": sum(item["input_tokens"] for item in usage),
        "cached_input_tokens": sum(item.get("cached_input_tokens", 0) for item in usage),
        "output_tokens": sum(item["output_tokens"] for item in usage),
        "usage_present": bool(usage),
        "tool_breakdown": tool_breakdown,
        "after_graph_calls": after_graph_calls,
    }


def answer_passes(task: dict, answer: str) -> bool:
    haystack = answer.casefold()
    if task.get("verdict") == "no":
        leading = re.sub(r"^[\s*_`#>.:;!?-]+", "", haystack)
        if not re.match(
            r"(?:no\b|none\b|not found\b|there (?:is|are) no\b|"
            r"(?:the )?(?:symbol|fifo|nosuchqueue)\b.{0,100}\b(?:not found|does not exist|not declared)\b)",
            leading,
        ):
            return False
    def matches(choice: str) -> bool:
        if "/" in choice:
            line = "" if re.search(r":\d+$", choice) else r"(?::\d+)?"
            boundary = r"(?=$|[\s,;)\]}>!?`\"']|\.(?=$|[\s,;)\]}>!?`\"']))"
            suffix = line + boundary
        else:
            suffix = r"(?!\w)"
        return bool(re.search(rf"(?<!\w){re.escape(choice.casefold())}{suffix}", haystack))
    return all(any(matches(choice) for choice in group) for group in task.get("answer_any", []))


def verified_success(record: dict) -> bool:
    return bool(record["success"] and (record["kind"] == "edit" or record.get("tool_calls", 0) > 0))


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


def workspace_env(workspace: Path) -> dict[str, str]:
    env = os.environ.copy()
    env["ARCH_STDLIB_PATH"] = str(workspace / "stdlib")
    return env


def run_command(command: list[str], cwd: Path, log: Path) -> float:
    start = time.monotonic()
    result = subprocess.run(command, cwd=cwd, env=workspace_env(cwd),
                            capture_output=True, text=True, timeout=180)
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


def prompt_for(task: dict, condition: str, arch_command: str = "arch",
               graph_token_budget: int = 100, include_source: bool = False) -> str:
    prompt = task["prompt"] + " " + VERIFY
    if condition == "no_graph":
        prompt += " " + NO_GRAPH
    elif condition == "graph_cli":
        command = (f"{arch_command} graph context" if task["cohort"] == "user"
                   else f"{arch_command} graph dev-query")
        index = ".archgraph" if task["cohort"] == "user" else ".archdevgraph"
        source_flag = " --include-source" if include_source else ""
        prompt += (f" For the first retrieval, use `{command}` with the task wording, "
                   f"`--index {index} --token-budget {graph_token_budget}{source_flag}`. "
                   "Do not call graph MCP tools. Verify only missing details from source.")
    elif condition == "graph_mcp":
        name = "arch_graph_context" if task["cohort"] == "user" else "arch_dev_graph_query"
        arg = "task" if task["cohort"] == "user" else "query"
        source_arg = ", include_source=True" if include_source else ""
        tool = f"{name}({arg}=..., token_budget={graph_token_budget}{source_arg})"
        prompt += (f" For the first retrieval, call `{tool}` against the prebuilt index. "
                   "Do not run graph CLI commands. Verify only missing details from source.")
    if task["kind"] == "edit":
        prompt += " Work only inside this checkout."
    return prompt


def run_attempt(task: dict, condition: str, repeat: int, repo: Path, output: Path,
                arch_bin: Path, mcp_python: Path, model: str, timeout: int,
                ignore_user_config: bool, graph_token_budget: int = 100,
                include_source: bool = False) -> dict:
    label = f"{task['id']}-r{repeat}-{condition}"
    if task["kind"] == "edit":
        temp = tempfile.TemporaryDirectory(prefix="arch-graph-eval-", dir=output)
        workspace = Path(temp.name) / "repo"
        try:
            subprocess.run(["git", "clone", "--shared", "--quiet", str(repo), str(workspace)], check=True)
            index_seconds = prepare_indexes(workspace, arch_bin, task["cohort"], output, label)
        except Exception:
            temp.cleanup()
            raise
    else:
        temp = None
        workspace = repo
        index_seconds = 0.0
    try:
        prompt = prompt_for(task, condition, str(arch_bin), graph_token_budget, include_source)
        command = [
            "codex", "exec", "--json", "--approve-for-me", "-m", model,
            "-c", 'model_reasoning_effort="low"',
            "-c", 'sandbox_mode="workspace-write"' if task["kind"] == "edit" else 'sandbox_mode="read-only"',
            "-c", make_mcp_config(workspace, arch_bin, mcp_python),
            "-C", str(workspace), "--ephemeral", prompt,
        ]
        if ignore_user_config:
            command.insert(4, "--ignore-user-config")
        env = workspace_env(workspace)
        env["PATH"] = str(arch_bin.parent) + os.pathsep + env.get("PATH", "")
        env["CARGO_TARGET_DIR"] = str(repo / "target")
        start = time.monotonic()
        process = subprocess.Popen(command, cwd=workspace, env=env, stdout=subprocess.PIPE,
                                   stderr=subprocess.PIPE, text=True, start_new_session=True)
        try:
            raw, stderr = process.communicate(timeout=max(timeout, 180) if task["kind"] == "edit" else timeout)
            exit_code = process.returncode
        except subprocess.TimeoutExpired:
            try:
                os.killpg(process.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            try:
                raw, stderr = process.communicate(timeout=5)
            except subprocess.TimeoutExpired:
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                raw, stderr = process.communicate()
            exit_code = 124
            stderr += "\nattempt timed out"
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
        valid_method = (
            (condition != "no_graph" or trace["graph_calls"] == 0)
            and (condition != "graph_cli" or
                 (trace["successful_graph_commands"] > 0 and
                  trace["successful_graph_mcp_calls"] == 0))
            and (condition != "graph_mcp" or
                 (trace["successful_graph_mcp_calls"] > 0 and
                  trace["successful_graph_commands"] == 0))
        )
        return {
            "task_id": task["id"], "cohort": task["cohort"], "kind": task["kind"],
            "condition": condition, "repeat": repeat, "exit_code": exit_code,
            "success": exit_code == 0 and trace["usage_present"] and valid_method and outcome
            and (task["kind"] == "edit" or trace["tool_calls"] > 0),
            "valid_method": valid_method, "grader_note": note,
            "input_tokens": trace["input_tokens"], "cached_input_tokens": trace["cached_input_tokens"],
            "output_tokens": trace["output_tokens"], "tool_calls": trace["tool_calls"],
            "graph_calls": trace["graph_calls"], "agent_seconds": elapsed,
            "successful_graph_calls": trace["successful_graph_calls"],
            "successful_graph_commands": trace["successful_graph_commands"],
            "successful_graph_index_reads": trace["successful_graph_index_reads"],
            "successful_graph_mcp_calls": trace["successful_graph_mcp_calls"],
            "failed_mcp_calls": trace["failed_mcp_calls"],
            "tool_breakdown": trace["tool_breakdown"],
            "after_graph_calls": trace["after_graph_calls"],
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
                "attempts": len(arm), "successes": sum(verified_success(r) for r in arm),
                "tokens": sum(r["input_tokens"] + r["output_tokens"] for r in arm),
                "uncached_tokens": sum(r["input_tokens"] - r["cached_input_tokens"] + r["output_tokens"] for r in arm),
                "graph_calls": sum(r["graph_calls"] for r in arm),
                "successful_graph_calls": sum(r.get("successful_graph_calls", 0) for r in arm),
                "failed_mcp_calls": sum(r.get("failed_mcp_calls", 0) for r in arm),
                "agent_seconds": round(sum(r["agent_seconds"] for r in arm), 3),
                "index_seconds": round(sum(r["index_seconds"] for r in arm), 3),
            }
        paired = {(r["task_id"], r["repeat"]): r for r in subset if r["condition"] == "no_graph"}
        pairs = [(base, r) for r in subset if r["condition"] == "default"
                 if (base := paired.get((r["task_id"], r["repeat"]))) is not None
                 and verified_success(base) and verified_success(r)]
        baseline_tokens = sum(a["input_tokens"] + a["output_tokens"] for a, _ in pairs)
        default_tokens = sum(b["input_tokens"] + b["output_tokens"] for _, b in pairs)
        graph_pairs = [(a, b) for a, b in pairs if b.get("successful_graph_calls", 0) > 0]
        graph_baseline_tokens = sum(a["input_tokens"] + a["output_tokens"] for a, _ in graph_pairs)
        graph_default_tokens = sum(b["input_tokens"] + b["output_tokens"] for _, b in graph_pairs)
        groups[cohort] = {
            "arms": arms, "successful_pairs": len(pairs),
            "paired_default_vs_search_pct": round(100 * (1 - default_tokens / baseline_tokens), 2) if baseline_tokens else None,
            "graph_used_successful_pairs": len(graph_pairs),
            "graph_used_pair_saving_pct": round(100 * (1 - graph_default_tokens / graph_baseline_tokens), 2)
            if graph_baseline_tokens else None,
            "default_discovery_graph_uses": sum(r["graph_calls"] > 0 for r in subset
                                                if r["condition"] == "default" and r["kind"] == "discovery"),
            "default_discovery_attempts": sum(r["condition"] == "default" and r["kind"] == "discovery"
                                              for r in subset),
            "known_file_graph_uses": sum(r["graph_calls"] > 0 for r in subset
                                         if r["condition"] == "default" and r["kind"] == "known_file"),
        }
        guided_arms = {}
        for condition in ("graph_cli", "graph_mcp"):
            arm = [r for r in subset if r["condition"] == condition]
            guided_arms[condition] = {
                "attempts": len(arm), "successes": sum(verified_success(r) for r in arm),
                "tokens": sum(r["input_tokens"] + r["output_tokens"] for r in arm),
                "uncached_tokens": sum(r["input_tokens"] - r["cached_input_tokens"]
                                       + r["output_tokens"] for r in arm),
                "successful_graph_calls": sum(r.get("successful_graph_calls", 0) for r in arm),
            }
        cli = {(r["task_id"], r["repeat"]): r for r in subset
               if r["condition"] == "graph_cli" and verified_success(r)}
        guided_pairs = [(r, cli[(r["task_id"], r["repeat"])]) for r in subset
                        if r["condition"] == "graph_mcp" and verified_success(r)
                        and (r["task_id"], r["repeat"]) in cli]
        mcp_tokens = sum(m["input_tokens"] + m["output_tokens"] for m, _ in guided_pairs)
        cli_tokens = sum(c["input_tokens"] + c["output_tokens"] for _, c in guided_pairs)
        groups[cohort]["guided_arms"] = guided_arms
        groups[cohort]["guided_pairs"] = len(guided_pairs)
        groups[cohort]["cli_vs_mcp_saving_pct"] = (
            round(100 * (1 - cli_tokens / mcp_tokens), 2) if mcp_tokens else None)
        search = {(r["task_id"], r["repeat"]): r for r in subset
                  if r["condition"] == "no_graph" and verified_success(r)}
        search_pairs = [(search[(r["task_id"], r["repeat"])], r) for r in subset
                        if r["condition"] == "graph_cli" and verified_success(r)
                        and (r["task_id"], r["repeat"]) in search]
        def pair_totals(pairs: list[tuple[dict, dict]]) -> dict:
            def totals(side: int) -> tuple[int, int]:
                return (sum(pair[side]["input_tokens"] + pair[side]["output_tokens"] for pair in pairs),
                        sum(pair[side]["input_tokens"] - pair[side]["cached_input_tokens"]
                            + pair[side]["output_tokens"] for pair in pairs))
            baseline, baseline_uncached = totals(0)
            graph, graph_uncached = totals(1)
            return {"pairs": len(pairs), "search_tokens": baseline, "graph_tokens": graph,
                    "search_uncached_tokens": baseline_uncached,
                    "graph_uncached_tokens": graph_uncached,
                    "total_saving_pct": round(100 * (1 - graph / baseline), 2) if baseline else None,
                    "uncached_saving_pct": round(100 * (1 - graph_uncached / baseline_uncached), 2)
                    if baseline_uncached else None}
        groups[cohort]["cli_vs_search"] = pair_totals(search_pairs)
        groups[cohort]["cli_vs_search_by_task"] = {
            task_id: pair_totals([pair for pair in search_pairs if pair[0]["task_id"] == task_id])
            for task_id in sorted({pair[0]["task_id"] for pair in search_pairs})
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
    run.add_argument("--timeout", type=int, default=90)
    run.add_argument("--task", action="append")
    run.add_argument("--condition", action="append",
                     choices=("no_graph", "default", "graph_cli", "graph_mcp"))
    run.add_argument("--graph-token-budget", type=int, default=100)
    run.add_argument("--include-source", action="store_true")
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
    if args.graph_token_budget < 1:
        p.error("--graph-token-budget must be positive")
    args.output.mkdir(parents=True, exist_ok=True)
    records_path = args.output / "records.jsonl"
    if records_path.exists():
        p.error(f"refusing to overwrite {records_path}")
    metadata = {
        "repo_sha": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip(),
        "repo_status": subprocess.check_output(["git", "status", "--porcelain"], cwd=repo, text=True).strip(),
        "tasks_sha256": hashlib.sha256(args.tasks.read_bytes()).hexdigest(),
        "selected_task_ids": [task["id"] for task in tasks],
        "model": args.model, "seed": args.seed, "repeats": args.repeats,
        "ignore_user_config": args.ignore_user_config, "approval_mode": "approve-for-me",
        "conditions": args.condition or ["no_graph", "default"],
        "graph_token_budget": args.graph_token_budget,
        "include_source": args.include_source,
        "codex_version": subprocess.check_output(["codex", "--version"], text=True).strip(),
        "arch_version": subprocess.check_output([str(arch_bin), "--version"], text=True).strip(),
        "arch_stdlib_path": str(repo / "stdlib"),
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
                for condition in (args.condition or ["no_graph", "default"])]
    random.Random(args.seed).shuffle(schedule)
    with records_path.open("w") as records:
        for task, repeat, condition in schedule:
            record = run_attempt(task, condition, repeat, repo, args.output, arch_bin, mcp_python,
                                 args.model, args.timeout, args.ignore_user_config,
                                 args.graph_token_budget, args.include_source)
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
