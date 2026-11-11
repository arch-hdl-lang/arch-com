import json
import os
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from codegraph_eval import answer_passes, graph_command_kind, is_graph_command, load_tasks, parse_events, prompt_for, summarize, trace_result, verified_success, workspace_env
from codegraph_direct_eval import CROSS_RELATIONS, grade_relations


ROOT = Path(__file__).resolve().parents[1]


class CodegraphEvalTests(unittest.TestCase):
    def test_workspace_env_uses_local_stdlib(self):
        with patch.dict(os.environ, {"ARCH_STDLIB_PATH": "/other/checkout/stdlib", "EVAL_TEST": "kept"}):
            env = workspace_env(Path("/trial/repo"))
        self.assertEqual(env["ARCH_STDLIB_PATH"], "/trial/repo/stdlib")
        self.assertEqual(env["EVAL_TEST"], "kept")

    def test_task_manifest_has_controls_and_edits(self):
        tasks = load_tasks(ROOT / "doc/codegraph_eval_tasks.json")
        self.assertEqual(len(tasks), 10)
        self.assertEqual({task["cohort"] for task in tasks}, {"user", "compiler"})
        self.assertEqual({task["kind"] for task in tasks},
                         {"discovery", "known_file", "coverage_miss", "edit"})

    def test_cross_file_manifest_has_relationships_and_controls(self):
        tasks = load_tasks(ROOT / "doc/codegraph_eval_cross_file_tasks.json")
        self.assertEqual(len(tasks), 5)
        self.assertEqual(sum(task["kind"] == "known_file" for task in tasks), 2)
        relation = CROSS_RELATIONS["user_package_import_target"]
        good = "examples/pkg_user.arch:1 use TestPkg -> examples/TestPkg.arch:1 TestPkg"
        self.assertTrue(grade_relations(good, relation, ROOT))
        self.assertFalse(grade_relations(good.replace("TestPkg.arch:1", "Other.arch:1"), relation, ROOT))

    def test_trace_parses_usage_and_graph_calls(self):
        raw = "\n".join(json.dumps(event) for event in [
            {"type": "item.completed", "item": {"type": "command_execution", "command": "arch graph dev-query fifo", "exit_code": 0}},
            {"type": "item.completed", "item": {"type": "mcp_tool_call", "tool": "arch_graph_context", "status": "failed"}},
            {"type": "item.completed", "item": {"type": "agent_message", "text": "src/ast.rs"}},
            {"type": "turn.completed", "usage": {"input_tokens": 100, "cached_input_tokens": 60, "output_tokens": 10}},
        ])
        result = trace_result(parse_events(raw))
        self.assertEqual(result["graph_calls"], 2)
        self.assertEqual(result["successful_graph_calls"], 1)
        self.assertEqual(result["failed_mcp_calls"], 1)
        self.assertEqual(result["tool_calls"], 2)
        self.assertEqual(result["input_tokens"] - result["cached_input_tokens"] + result["output_tokens"], 50)
        self.assertEqual(result["answer"], "src/ast.rs")

    def test_partial_last_event_is_ignored(self):
        events = parse_events('{"type":"turn.started"}\n{"type":"item.completed","item":')
        self.assertEqual(events, [{"type": "turn.started"}])

    def test_trace_breaks_down_tool_outputs_without_token_attribution(self):
        events = [{"type": "item.completed", "item": {"type": "command_execution",
                   "command": "rg TxQueue /home/me/.codex/memories/MEMORY.md", "aggregated_output": "memo"}},
                  {"type": "item.completed", "item": {"type": "mcp_tool_call",
                   "tool": "arch_graph_context", "status": "completed",
                   "result": {"content": [{"type": "text", "text": "[OK] path:17 TxQueue"}]}}},
                  {"type": "item.completed", "item": {"type": "command_execution",
                   "command": "sed -n '17p' examples/sync_fifo.arch", "aggregated_output": "fifo TxQueue"}}]
        result = trace_result(events)
        self.assertEqual(result["tool_breakdown"]["memory"], {"calls": 1, "output_chars": 4})
        self.assertEqual(result["tool_breakdown"]["graph"]["calls"], 1)
        self.assertEqual(result["tool_breakdown"]["source_read"]["calls"], 1)
        self.assertEqual(result["after_graph_calls"], 1)

    def test_direct_index_read_counts_as_graph_use(self):
        events = [{"type": "item.completed", "item": {"type": "command_execution",
                   "command": "head .archdevgraph/nodes.jsonl", "exit_code": 0}}]
        self.assertEqual(trace_result(events)["graph_calls"], 1)
        self.assertEqual(trace_result(events)["successful_graph_index_reads"], 1)
        self.assertEqual(trace_result(events)["successful_graph_commands"], 0)

    def test_search_mention_is_not_a_graph_call(self):
        self.assertFalse(is_graph_command('/bin/zsh -lc \'rg -n "arch graph context" README.md\''))
        self.assertTrue(is_graph_command('/bin/zsh -lc \'/tmp/bin/arch graph context fifo --index .archgraph\''))
        self.assertTrue(is_graph_command("head .archdevgraph/nodes.jsonl"))
        self.assertEqual(graph_command_kind("head .archdevgraph/nodes.jsonl"), "index_read")

    def test_completed_mcp_error_is_not_a_successful_graph_call(self):
        events = [
            {"type": "item.completed", "item": {"type": "mcp_tool_call", "tool": "arch_dev_graph_query",
             "status": "completed", "result": {"content": [{"type": "text", "text": "[ERROR (exit 1)] unknown role"}]}}},
            {"type": "item.completed", "item": {"type": "mcp_tool_call", "tool": "arch_dev_graph_query",
             "status": "completed", "result": {"content": [{"type": "text", "text": "[OK]\nfeature arbiter"}]}}},
        ]
        result = trace_result(events)
        self.assertEqual(result["graph_calls"], 2)
        self.assertEqual(result["successful_graph_calls"], 1)
        self.assertEqual(result["failed_mcp_calls"], 1)

    def test_answer_rubric_accepts_alternative_owners(self):
        task = next(task for task in load_tasks(ROOT / "doc/codegraph_eval_tasks.json")
                    if task["id"] == "compiler_pipe_reg_owners")
        answer = "RegReset src/ast.rs; parse_pipe_reg_decl src/parser.rs; emit_pipe_reg src/codegen/module.rs"
        self.assertTrue(answer_passes(task, answer))
        self.assertFalse(answer_passes(task, answer.replace("src/codegen/module.rs", "src/codegen/fifo.rs")))

    def test_absence_rubric_does_not_match_symbol_name(self):
        task = next(task for task in load_tasks(ROOT / "doc/codegraph_eval_tasks.json")
                    if task["id"] == "user_missing_symbol")
        self.assertFalse(answer_passes(task, "NoSuchQueue is declared in examples/x.arch"))
        self.assertTrue(answer_passes(task, "No, NoSuchQueue was not found."))

    def test_unverified_answer_is_not_a_success(self):
        self.assertFalse(verified_success({"success": True, "kind": "coverage_miss", "tool_calls": 0}))
        self.assertTrue(verified_success({"success": True, "kind": "coverage_miss", "tool_calls": 1}))

    def test_both_arms_receive_same_completion_instruction(self):
        task = next(task for task in load_tasks(ROOT / "doc/codegraph_eval_tasks.json")
                    if task["id"] == "compiler_arbiter_owners")
        default = prompt_for(task, "default")
        baseline = prompt_for(task, "no_graph")
        self.assertIn("at most three targeted retrieval calls", default)
        self.assertIn("at most three targeted retrieval calls", baseline)
        self.assertNotIn("arch graph", default)
        self.assertIn("Graph commands", baseline)

    def test_guided_transport_prompts_keep_same_budget(self):
        task = next(task for task in load_tasks(ROOT / "doc/codegraph_eval_tasks.json")
                    if task["id"] == "compiler_arbiter_owners")
        cli = prompt_for(task, "graph_cli")
        mcp = prompt_for(task, "graph_mcp")
        self.assertIn("arch graph dev-query", cli)
        self.assertIn("arch_dev_graph_query", mcp)
        self.assertIn("100", cli)
        self.assertIn("100", mcp)
        self.assertIn("at most three targeted retrieval calls", cli)
        self.assertIn("at most three targeted retrieval calls", mcp)
        sourced = prompt_for(task, "graph_cli", "/tmp/arch", 200, True)
        self.assertIn("/tmp/arch graph dev-query", sourced)
        self.assertIn("--token-budget 200 --include-source", sourced)

    def test_summary_uses_only_successful_pairs(self):
        def record(condition, success, tokens, graph_calls=0):
            return {"task_id": "x", "repeat": 1, "cohort": "user", "kind": "discovery",
                    "condition": condition, "success": success, "input_tokens": tokens,
                    "cached_input_tokens": 0, "output_tokens": 0, "graph_calls": graph_calls,
                    "successful_graph_calls": graph_calls,
                    "tool_calls": 1, "agent_seconds": 1, "index_seconds": 0}
        summary = summarize([record("no_graph", True, 100), record("default", True, 70, 1)])
        self.assertEqual(summary["user"]["successful_pairs"], 1)
        self.assertEqual(summary["user"]["paired_default_vs_search_pct"], 30)
        self.assertEqual(summary["user"]["graph_used_pair_saving_pct"], 30)
        failed = summarize([record("no_graph", True, 100), record("default", False, 70)])
        self.assertIsNone(failed["user"]["paired_default_vs_search_pct"])
        self.assertIsNone(failed["user"]["graph_used_pair_saving_pct"])

    def test_guided_summary_compares_only_verified_pairs(self):
        def record(condition, success, tokens):
            return {"task_id": "x", "repeat": 1, "cohort": "compiler", "kind": "discovery",
                    "condition": condition, "success": success, "input_tokens": tokens,
                    "cached_input_tokens": 0, "output_tokens": 0, "graph_calls": 1,
                    "successful_graph_calls": 1, "tool_calls": 1,
                    "agent_seconds": 1, "index_seconds": 0}
        result = summarize([record("graph_mcp", True, 100), record("graph_cli", True, 70)])
        self.assertEqual(result["compiler"]["guided_pairs"], 1)
        self.assertEqual(result["compiler"]["cli_vs_mcp_saving_pct"], 30)

    def test_duplicate_ids_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "tasks.json"
            task = {"id": "x", "cohort": "user", "kind": "discovery", "prompt": "x"}
            path.write_text(json.dumps({"schema_version": 1, "tasks": [task, task]}))
            with self.assertRaisesRegex(ValueError, "duplicate task"):
                load_tasks(path)


if __name__ == "__main__":
    unittest.main()
