import json
import tempfile
import unittest
from pathlib import Path

from codegraph_eval import answer_passes, load_tasks, parse_events, prompt_for, summarize, trace_result, verified_success


ROOT = Path(__file__).resolve().parents[1]


class CodegraphEvalTests(unittest.TestCase):
    def test_task_manifest_has_controls_and_edits(self):
        tasks = load_tasks(ROOT / "doc/codegraph_eval_tasks.json")
        self.assertEqual(len(tasks), 10)
        self.assertEqual({task["cohort"] for task in tasks}, {"user", "compiler"})
        self.assertEqual({task["kind"] for task in tasks},
                         {"discovery", "known_file", "coverage_miss", "edit"})

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

    def test_direct_index_read_counts_as_graph_use(self):
        events = [{"type": "item.completed", "item": {"type": "command_execution",
                   "command": "head .archdevgraph/nodes.jsonl", "exit_code": 0}}]
        self.assertEqual(trace_result(events)["graph_calls"], 1)

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

    def test_string_mcp_error_does_not_abort_trace(self):
        events = [{"type": "item.completed", "item": {"type": "mcp_tool_call",
                   "tool": "arch_graph_query", "status": "completed", "error": "boom"}}]
        self.assertEqual(trace_result(events)["failed_mcp_calls"], 1)

    def test_cargo_release_graph_command_counts(self):
        for command in ("cargo run --release -- graph query TxQueue",
                        "cargo run -q --release -- graph query TxQueue",
                        "cargo run --bin arch -- graph query TxQueue"):
            with self.subTest(command=command):
                events = [{"type": "item.completed", "item": {"type": "command_execution",
                           "command": command, "exit_code": 0}}]
                self.assertEqual(trace_result(events)["graph_calls"], 1)

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
        self.assertFalse(answer_passes(task, "Yes - NoSuchQueue is in examples/q.arch (no other copies)."))
        self.assertTrue(answer_passes(task, "**No.** NoSuchQueue is not declared."))
        self.assertTrue(answer_passes(task, "There is no FIFO named NoSuchQueue in examples/ or stdlib/."))
        self.assertFalse(answer_passes(task, "**Yes.** NoSuchQueue is declared; no other copies exist."))

    def test_answer_keys_require_token_boundaries(self):
        tasks = {task["id"]: task for task in load_tasks(ROOT / "doc/codegraph_eval_tasks.json")}
        self.assertFalse(answer_passes(tasks["user_fifo_lookup"], "examples/sync_fifo.arch:170"))
        self.assertFalse(answer_passes(tasks["user_fifo_lookup"], "examples/sync_fifo.arch:17.bak"))
        self.assertTrue(answer_passes(tasks["user_fifo_lookup"], "examples/sync_fifo.arch:17."))
        self.assertFalse(answer_passes(tasks["compiler_known_file"], "emit_fifo_sync_body_general"))

    def test_path_answers_reject_filename_suffixes(self):
        task = next(task for task in load_tasks(ROOT / "doc/codegraph_eval_tasks.json")
                    if task["id"] == "user_bus_relationship")
        for suffix in (".bak", "..bak", "/extra", "-copy", ":wrong"):
            with self.subTest(suffix=suffix):
                answer = f"stdlib/BusAxiLite.arch{suffix} tests/axi_dma/BusAxiLite.arch{suffix}"
                self.assertFalse(answer_passes(task, answer))
        self.assertTrue(answer_passes(
            task, "stdlib/BusAxiLite.arch:1, tests/axi_dma/BusAxiLite.arch."
        ))

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

    def test_duplicate_ids_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "tasks.json"
            task = {"id": "x", "cohort": "user", "kind": "discovery", "prompt": "x"}
            path.write_text(json.dumps({"schema_version": 1, "tasks": [task, task]}))
            with self.assertRaisesRegex(ValueError, "duplicate task"):
                load_tasks(path)


if __name__ == "__main__":
    unittest.main()
