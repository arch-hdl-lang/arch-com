import hashlib
import io
import json
import os
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from unittest.mock import Mock, patch

from codegraph_eval import ANSWER_EVIDENCE_POLICY, answer_passes, graph_command_kind, is_graph_command, known_file_answer_evidence, known_file_source_read, load_tasks, main, parse_events, prefetch_source, prompt_for, run_attempt, summarize, trace_result, validate_source_pins, verified_success, workspace_env
from codegraph_direct_eval import CROSS_RELATIONS, PIPE_REG_OWNERS, grade_pipe_reg_owners, grade_relations


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

    def test_pipe_reg_change_tasks_distinguish_declarations_from_ports(self):
        tasks = {task["id"]: task for task in load_tasks(ROOT / "doc/codegraph_eval_cross_file_tasks.json")}
        declaration = tasks["compiler_pipe_reg_change_map"]
        port = tasks["compiler_pipe_reg_port_reset_map"]
        self.assertIn("pipe_reg name: source stages N", declaration["prompt"])
        self.assertIn("output-port", port["prompt"])
        self.assertTrue(answer_passes(declaration, "PipeRegDecl src/ast.rs; parse_pipe_reg_decl src/parser.rs; emit_pipe_reg src/codegen/module.rs; examples/pipe_reg_test.arch"))
        self.assertTrue(answer_passes(port, "PortRegInfo src/ast.rs; parse_port_decl src/parser.rs; emit_reg_block src/codegen/module.rs; tests/verilog_eval/Prob047_dff8ar.arch"))
        self.assertFalse(answer_passes(port, "PipeRegDecl src/ast.rs; parse_pipe_reg_decl src/parser.rs; emit_pipe_reg src/codegen/module.rs; examples/pipe_reg_test.arch"))

    def test_pipe_reg_direct_audit_requires_ranked_source_ownership(self):
        for task_id, (feature, owners) in PIPE_REG_OWNERS.items():
            lines = [f"feature {feature}"]
            for role, path, symbol in owners:
                source = (ROOT / path).read_text().splitlines()
                line_number = (1 if role == "fixture" else
                               next(i for i, line in enumerate(source, 1) if symbol in line))
                excerpt = ("" if role == "fixture" else
                           " | " + " ".join(source[line_number - 1].split())[:120])
                lines.append(f"  {role} {path}:{line_number} {symbol} [curated]{excerpt}")
            output = "\n".join(lines)
            self.assertTrue(grade_pipe_reg_owners(output, task_id, ROOT))
            self.assertFalse(grade_pipe_reg_owners(output.replace("src/ast.rs", "src/parser.rs"), task_id, ROOT))
            self.assertFalse(grade_pipe_reg_owners(output.replace("[curated] | ", "[curated] | wrong ", 1), task_id, ROOT))
            self.assertFalse(grade_pipe_reg_owners(output.replace("  fixture ", "  parser "), task_id, ROOT))
            self.assertFalse(grade_pipe_reg_owners(output.replace("feature ", "feature wrong ", 1), task_id, ROOT))
            self.assertFalse(grade_pipe_reg_owners(output.replace("\n  fixture ", "\n  extra unrelated line\n  fixture "), task_id, ROOT))

    def test_cross_file_manifest_has_relationships_and_controls(self):
        tasks = load_tasks(ROOT / "doc/codegraph_eval_cross_file_tasks.json")
        self.assertEqual(len(tasks), 7)
        self.assertEqual(sum(task["kind"] == "known_file" for task in tasks), 2)
        relation = CROSS_RELATIONS["user_package_import_target"]
        good = "examples/pkg_user.arch:1 use TestPkg -> examples/TestPkg.arch:1 TestPkg | decl package TestPkg"
        self.assertTrue(grade_relations(good, relation, ROOT))
        self.assertFalse(grade_relations(good.replace("TestPkg.arch:1", "Other.arch:1"), relation, ROOT))
        self.assertFalse(grade_relations(good.replace(" | decl package TestPkg", ""), relation, ROOT))

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
        self.assertFalse(is_graph_command("arch graph context --help"))
        self.assertEqual(graph_command_kind("arch graph dev-index --root ."), "cli_setup")

    def test_index_directory_reads_count_as_graph_use(self):
        for command in ("rg TxQueue .archgraph",
                        'bash -lc "rg -n TxQueue .archdevgraph"',
                        "cd .archgraph && head nodes.jsonl"):
            with self.subTest(command=command):
                self.assertEqual(graph_command_kind(command), "index_read")
        self.assertIsNone(graph_command_kind("rg TxQueue src"))

    def test_wrapped_and_combined_graph_queries_are_detected(self):
        for command in ("env ARCH_STDLIB_PATH=stdlib arch graph context TxQueue",
                        "env -u ARCH_STDLIB_PATH arch graph context TxQueue",
                        "timeout 60 arch graph context TxQueue",
                        "timeout -k 5 -s INT 60s arch graph context TxQueue",
                        "nice arch graph query TxQueue",
                        "nice -n 10 arch graph query TxQueue",
                        "command arch graph query TxQueue",
                        "timeout 60 env ARCH_STDLIB_PATH=stdlib arch graph context TxQueue",
                        "arch graph dev-index --root . && arch graph dev-query fifo",
                        "arch graph context --help; arch graph context TxQueue"):
            with self.subTest(command=command):
                self.assertEqual(graph_command_kind(command), "cli")
                events = [{"type": "item.completed", "item": {"type": "command_execution",
                           "command": command, "exit_code": 0}}]
                self.assertEqual(trace_result(events)["graph_calls"], 1)
        self.assertEqual(graph_command_kind("arch graph dev-index --root ."), "cli_setup")
        self.assertEqual(graph_command_kind("timeout 60 arch graph dev-index --root ."), "cli_setup")
        for command in ("timeout 60 rg TxQueue src", "nice rg arch",
                        "timeout 60 arch graph context --help"):
            self.assertIsNone(graph_command_kind(command))

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
        self.assertTrue(answer_passes(task, "NoSuchQueue is not declared in examples/ or stdlib/."))
        for answer in ("No in examples/; yes in stdlib/q.arch.",
                       "There is no NoSuchQueue in examples/, but stdlib/q.arch declares it.",
                       "The FIFO NoSuchQueue is declared in stdlib/q.arch; it does not exist in examples/.",
                       "No in examples/, but found in stdlib/q.arch."):
            with self.subTest(answer=answer):
                self.assertFalse(answer_passes(task, answer))

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
        self.assertTrue(answer_passes(task, "stdlib/BusAxiLite.arch and tests/axi_dma/BusAxiLite.arch:"))

    def test_path_answers_accept_colon_forms(self):
        task = next(task for task in load_tasks(ROOT / "doc/codegraph_eval_tasks.json")
                    if task["id"] == "user_fifo_lookup")
        for answer in ("examples/sync_fifo.arch:17:1",
                       "examples/sync_fifo.arch:17: fifo TxQueue",
                       "examples/sync_fifo.arch:17:"):
            with self.subTest(answer=answer):
                self.assertTrue(answer_passes(task, answer))
        self.assertFalse(answer_passes(task, "examples/sync_fifo.arch:170: fifo TxQueue"))

    def test_unverified_answer_is_not_a_success(self):
        self.assertFalse(verified_success({"success": True, "kind": "coverage_miss", "tool_calls": 0}))
        self.assertTrue(verified_success({"success": True, "kind": "coverage_miss", "tool_calls": 1}))
        self.assertFalse(verified_success({"success": True, "kind": "known_file", "tool_calls": 1,
                                          "known_file_source_verified": False}))

    def test_both_arms_receive_same_completion_instruction(self):
        task = next(task for task in load_tasks(ROOT / "doc/codegraph_eval_tasks.json")
                    if task["id"] == "compiler_arbiter_owners")
        default = prompt_for(task, "default")
        baseline = prompt_for(task, "no_graph")
        self.assertIn("at most three targeted retrieval calls", default)
        self.assertIn("at most three targeted retrieval calls", baseline)
        self.assertNotIn("arch graph", default)
        self.assertIn("Graph commands", baseline)

    def test_known_file_controls_require_direct_verification_in_both_arms(self):
        tasks = load_tasks(ROOT / "doc/codegraph_eval_cross_file_tasks.json")
        for task in tasks:
            for condition in ("no_graph", "default"):
                prompt = prompt_for(task, condition)
                if task["kind"] == "known_file":
                    self.assertIn("execute one targeted source read or `rg -n`", prompt)
                    self.assertIn("Do not use a graph for an in-file fact", prompt)
                else:
                    self.assertNotIn("Do not use a graph for an in-file fact", prompt)
            if task["kind"] == "known_file":
                self.assertNotIn("Do not use a graph for an in-file fact",
                                 prompt_for(task, "graph_cli"))

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

    def test_guided_cli_vs_search_reports_full_and_task_totals(self):
        def record(task, condition, total, cached):
            return {"task_id": task, "repeat": 1, "cohort": "user", "kind": "discovery",
                    "condition": condition, "success": True, "input_tokens": total,
                    "cached_input_tokens": cached, "output_tokens": 0, "graph_calls": 1,
                    "successful_graph_calls": 1, "tool_calls": 1, "agent_seconds": 1,
                    "index_seconds": 0}
        rows = [record("instance", "no_graph", 100, 60),
                record("instance", "graph_cli", 50, 30),
                record("import", "no_graph", 100, 60),
                record("import", "graph_cli", 110, 30)]
        result = summarize(rows)["user"]
        self.assertEqual(result["cli_vs_search"]["total_saving_pct"], 20)
        self.assertEqual(result["cli_vs_search"]["uncached_saving_pct"], -25)
        self.assertEqual(result["cli_vs_search_by_task"]["instance"]["total_saving_pct"], 50)

    def test_duplicate_ids_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "tasks.json"
            task = {"id": "x", "cohort": "user", "kind": "discovery", "prompt": "x"}
            path.write_text(json.dumps({"schema_version": 1, "tasks": [task, task]}))
            with self.assertRaisesRegex(ValueError, "duplicate task"):
                load_tasks(path)

    def test_known_file_read_requires_successful_exact_source_command(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "design.arch").write_text("module Design\n")
            def events(command, output="1:module Design", code=0):
                return [{"type": "item.completed", "item": {
                    "type": "command_execution", "command": command,
                    "exit_code": code, "aggregated_output": output}}]
            for command in ("rg -n 'module' design.arch", "rg -n -C 2 module ./design.arch",
                            "rg -n 'Design$' design.arch",
                            "rg -n -m5 -- module design.arch", "rg --line-number --fixed-strings module design.arch",
                            "sed -n '1,10p' design.arch", "cat -n -- design.arch",
                            "zsh -lc \"rg -n module design.arch\"", f"cat {root}/design.arch"):
                with self.subTest(command=command):
                    self.assertTrue(known_file_source_read(events(command), root, "design.arch"))
            for command in ("echo 'cat design.arch'", "rg design.arch README.md",
                            "rg -n module design.arch.bak", "cat ../design.arch",
                            "rg -l module design.arch", "rg --count module design.arch",
                            "rg --files design.arch", "rg --pre echo module design.arch",
                            "sed -n '1e echo fake' design.arch", "cat design.arch > out",
                            "false; cat design.arch", "cat $(echo design.arch)",
                            "rg module design.arch | head", "cat 'design.arch",
                            "rg -n module design.arch other.arch"):
                with self.subTest(command=command):
                    self.assertFalse(known_file_source_read(events(command), root, "design.arch"))
            self.assertFalse(known_file_source_read(events("cat design.arch", code=1), root, "design.arch"))
            self.assertFalse(known_file_source_read(events("cat design.arch", output=""), root, "design.arch"))
            self.assertFalse(known_file_source_read(events("cat missing.arch"), root, "missing.arch"))
            (root / "nested").mkdir()
            (root / "nested/link.arch").symlink_to(root / "design.arch")
            self.assertFalse(known_file_source_read(events("cat link.arch"), root / "nested", "link.arch"))

    def run_mock_recovery(self, stages, enabled=True, condition="default", kind="known_file",
                          prefetch=False, byte_budget=8192, verify_answer=False, answer_groups=None,
                          excerpts=False, source="// Engine\nfn emit_ready_body() {}\n", source_pin=None,
                          expected_citation=None):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "engine.rs").write_text(source)
            task = {"id": "held_out", "kind": kind, "cohort": "compiler",
                    "prompt": "In engine.rs, identify the function that emits the ready body.",
                    "source_path": "engine.rs", "answer_any": answer_groups or [["emit_ready_body"]]}
            if source_pin is not None:
                task["source_sha256"] = source_pin
            if expected_citation is not None:
                task["expected_citation"] = expected_citation
                del task["answer_any"]
            processes = []
            for stage in stages:
                events = []
                if stage.get("command"):
                    events.append({"type": "item.completed", "item": {
                        "type": "command_execution", "command": stage["command"],
                        "exit_code": stage.get("tool_exit", 0),
                        "aggregated_output": stage.get("tool_output", "2:fn emit_ready_body() {}")}})
                if "answer" in stage:
                    events.append({"type": "item.completed", "item": {
                        "type": "agent_message", "text": stage["answer"]}})
                if stage.get("usage", True):
                    events.append({"type": "turn.completed", "usage": {
                        "input_tokens": 100, "cached_input_tokens": 60, "output_tokens": 10}})
                process = Mock(returncode=stage.get("exit", 0))
                process.communicate.return_value = ("\n".join(json.dumps(e) for e in events), "")
                processes.append(process)
            commands = []
            def popen(command, **kwargs):
                commands.append(list(command))
                if "source_after" in stages[len(commands) - 1]:
                    (root / "engine.rs").write_text(stages[len(commands) - 1]["source_after"])
                if stages[len(commands) - 1].get("remove_source"):
                    (root / "engine.rs").unlink()
                return processes[len(commands) - 1]
            with patch("codegraph_eval.subprocess.Popen", side_effect=popen):
                result = run_attempt(task, condition, 1, root, root, root / "arch", root / "python",
                                     "test-model", 10, True, recover_known_file=enabled,
                                     prefetch_known_file=prefetch, prefetch_byte_budget=byte_budget,
                                     verify_known_file_answer=verify_answer, prefetch_excerpts=excerpts)
            if (prefetch or condition == "source_prefetch") and kind == "known_file":
                receipt = json.loads((root / result["prefetch"]["artifact"]).read_text())
                self.assertEqual({k: v for k, v in receipt.items() if k != "payload"},
                                 {k: v for k, v in result["prefetch"].items() if k != "artifact"})
            files = {p.name: p.read_text() for p in root.glob("*.jsonl")}
            return result, commands, files

    @staticmethod
    def ready_citation(**changes):
        return json.dumps({"symbol": "emit_ready_body", "path": "engine.rs", "line": 2,
                           "quote": "fn emit_ready_body() {}", **changes})

    def test_answer_evidence_binds_name_path_line_and_quote_without_gold(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "engine.rs").write_bytes(b"// Engine\r\n  fn emit_ready_body() {}\r\n")
            result = known_file_answer_evidence(self.ready_citation(), root, "engine.rs")
            self.assertTrue(result["verified"])
            self.assertEqual(result["quote_match"], "full_line")
            self.assertEqual(result["policy"], ANSWER_EVIDENCE_POLICY)
            self.assertEqual(result["graded_answer"], "emit_ready_body engine.rs:2")
            self.assertEqual(result["read_calls"], 1)
            for answer in ("No function exists", "null", "[]", self.ready_citation(line=True),
                           self.ready_citation(line=0), self.ready_citation(line=1),
                           self.ready_citation(line=999), self.ready_citation(path="other.rs"),
                           self.ready_citation(symbol="emit_body_ready"), self.ready_citation(symbol="emit"),
                           self.ready_citation(quote="fn emit_ready_body() { changed() }"),
                           self.ready_citation(explanation="No matching function exists"),
                           self.ready_citation()[:-1] + ', "symbol": "emit_ready_body"}',
                           " " * 8193 + self.ready_citation()):
                with self.subTest(answer=answer):
                    self.assertFalse(known_file_answer_evidence(answer, root, "engine.rs")["verified"])

    def test_answer_evidence_accepts_verbatim_declaration_prefixes(self):
        cases = (
            ("engine.rs", "emit_ready_body", "fn emit_ready_body() {}", "fn emit_ready_body()"),
            ("engine.rs", "load_cache", "pub(crate) fn load_cache(root: &Path) -> io::Result<PathBuf> {",
             "pub(crate) fn load_cache(root: &Path) -> io::Result<PathBuf>"),
            ("engine.rs", "load_cache", "pub(crate) fn load_cache(root: &Path) -> io::Result<PathBuf> {",
             "pub(crate) fn load_cache"),
            ("engine.rs", "Holder", "struct Holder<T> {", "struct Holder<T>"),
            ("unit.arch", "PacketQueue", "fifo PacketQueue // packet storage", "fifo PacketQueue"),
        )
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for path, symbol, source, quote in cases:
                with self.subTest(source=source, quote=quote):
                    (root / path).write_text("// Header\n  " + source + "\n")
                    result = known_file_answer_evidence(
                        self.ready_citation(path=path, symbol=symbol, quote=quote), root, path)
                    self.assertTrue(result["verified"])
                    self.assertEqual(result["quote_match"], "line_prefix")
                    self.assertEqual(result["graded_answer"], f"{symbol} {path}:2")

    def test_answer_evidence_prefix_rejects_mutated_text_and_split_identifiers(self):
        source = "pub fn load_cache(root: &Path) -> io::Result<PathBuf> {"
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "engine.rs").write_text("// Header\n" + source + "\n")
            for changes in (
                    {"quote": source.replace("&Path", "&str")[:-2]},
                    {"quote": source.replace("PathBuf", "String")[:-2]},
                    {"quote": source.replace("pub fn", "pub  fn")[:-2]},
                    {"quote": "load_cache"},
                    {"quote": "pub fn load_cache(root: &Path) -> io::Res"},
                    {"quote": "pub fn load_cache(root: &Path) -> io::Result<Path"},
                    {"quote": "pub fn load_cache", "symbol": "load"},
                    {"quote": "pub fn load", "symbol": "load"},
                    {"quote": "pub fn load_cache", "line": 1},
                    {"quote": "pub fn load_cache", "path": "other.rs"},
                    {"quote": "pub fn load_cache", "symbol": "other"},
                    {"quote": "pub fn\nload_cache"},
            ):
                with self.subTest(changes=changes):
                    answer = self.ready_citation(**({"symbol": "load_cache", **changes}))
                    self.assertFalse(known_file_answer_evidence(answer, root, "engine.rs")["verified"])
            # A second occurrence of the short name cannot validate a split declaration name.
            (root / "engine.rs").write_text("// Header\nfn load_cache(load: usize) {\n")
            self.assertFalse(known_file_answer_evidence(
                self.ready_citation(symbol="load", quote="fn load"), root, "engine.rs")["verified"])

    def test_answer_evidence_prefix_avoids_format_only_recovery(self):
        for condition in ("no_graph", "default", "source_prefetch"):
            stage = {"answer": self.ready_citation(quote="fn emit_ready_body()")}
            if condition != "source_prefetch":
                stage["command"] = "cat engine.rs"
            result, commands, _ = self.run_mock_recovery(
                [stage], condition=condition, verify_answer=True)
            self.assertTrue(result["success"])
            self.assertEqual(result["model_attempts"], 1)
            self.assertEqual(result["recovery_attempts"], 0)
            self.assertEqual(len(commands), 1)
            self.assertEqual(result["stages"][0]["answer_evidence"]["quote_match"], "line_prefix")
            self.assertIn("verbatim prefix", commands[0][-1])

    def test_answer_evidence_rejects_unavailable_oversized_and_external_source(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for content in (b"\xff", b"\0", b" " * (1024 * 1024 + 1)):
                (root / "engine.rs").write_bytes(content)
                self.assertFalse(known_file_answer_evidence(self.ready_citation(), root, "engine.rs")["verified"])
            (root / "engine.rs").unlink()
            self.assertFalse(known_file_answer_evidence(self.ready_citation(), root, "engine.rs")["verified"])
            (root / "outside.rs").write_text("// Engine\nfn emit_ready_body() {}\n")
            (root / "repo").mkdir()
            (root / "repo/engine.rs").symlink_to(root / "outside.rs")
            self.assertFalse(known_file_answer_evidence(self.ready_citation(), root / "repo", "engine.rs")["verified"])
            (root / ".git").mkdir()
            (root / ".git/source").write_text("// Engine\nfn emit_ready_body() {}\n")
            self.assertFalse(known_file_answer_evidence(self.ready_citation(path=".git/source"), root, ".git/source")["verified"])

    def test_answer_evidence_recovers_false_absence_after_successful_source_read(self):
        for condition in ("no_graph", "default", "source_prefetch"):
            result, commands, files = self.run_mock_recovery([
                {"command": "rg -n ready engine.rs", "answer": "No matching function exists in engine.rs."},
                {"command": "cat engine.rs", "answer": self.ready_citation()},
            ], enabled=False, condition=condition, verify_answer=True)
            self.assertTrue(result["success"])
            self.assertEqual([s["source_verified"] for s in result["stages"]], [True, True])
            self.assertEqual([s["answer_evidence"]["verified"] for s in result["stages"]], [False, True])
            self.assertTrue(result["known_file_answer_verified"])
            self.assertEqual(result["model_attempts"], 2)
            self.assertEqual(result["input_tokens"], 200)
            self.assertEqual(result["tool_calls"], 2)
            self.assertEqual(len(files), 2)
            self.assertIn("did not provide a source-bound answer", commands[1][-1])
            self.assertNotIn("emit_ready_body", commands[1][-1].split("Source evidence")[0])
            arm = summarize([result])["all"]["arms"][condition]
            self.assertEqual(arm["answer_verified_controls"], 1)
            self.assertEqual(arm["answer_evidence_reads"], 1)
            self.assertEqual((arm["tokens"], arm["uncached_tokens"]), (220, 100))

    def test_answer_evidence_never_retries_a_valid_but_semantically_wrong_citation(self):
        result, commands, _ = self.run_mock_recovery([
            {"command": "cat engine.rs", "answer": self.ready_citation(symbol="other", line=3, quote="fn other() { emit_ready_body(); }"),
             "source_after": "// Engine\nfn emit_ready_body() {}\nfn other() { emit_ready_body(); }\n"},
        ], verify_answer=True)
        self.assertTrue(result["known_file_answer_verified"])
        self.assertFalse(result["success"])
        self.assertEqual(len(commands), 1)

    def test_answer_evidence_grades_location_and_does_not_change_discovery(self):
        result, commands, _ = self.run_mock_recovery([
            {"command": "cat engine.rs", "answer": self.ready_citation()}],
            verify_answer=True, answer_groups=[["engine.rs:2"]])
        self.assertTrue(result["success"])
        self.assertEqual(len(commands), 1)
        result, commands, _ = self.run_mock_recovery([
            {"command": "cat engine.rs", "answer": "emit_ready_body"}],
            kind="discovery", verify_answer=True)
        self.assertTrue(result["success"])
        self.assertNotIn("known_file_answer_verified", result)
        self.assertNotIn("Return only a JSON object", commands[0][-1])

    def test_answer_evidence_keeps_retry_bound_and_infrastructure_guards(self):
        for last in ({"command": "cat engine.rs", "answer": "null"},
                     {"command": "cat engine.rs", "answer": self.ready_citation(), "usage": False},
                     {"answer": self.ready_citation()}):
            result, commands, _ = self.run_mock_recovery([
                {"command": "cat engine.rs", "answer": "No function exists"}, last,
            ], verify_answer=True)
            self.assertFalse(result["success"])
            self.assertEqual(len(commands), 2)
        for first in ({"answer": "null", "exit": 1}, {"answer": "null", "exit": 124},
                      {"answer": "null", "usage": False},
                      {"command": "arch graph dev-query ready", "answer": "null"}):
            result, commands, _ = self.run_mock_recovery([first], condition="no_graph", verify_answer=True)
            self.assertFalse(result["success"])
            self.assertEqual(len(commands), 1)

    def test_answer_evidence_accepts_first_pass_prefetch_and_is_opt_in(self):
        result, commands, _ = self.run_mock_recovery([
            {"answer": self.ready_citation()}], prefetch=True, verify_answer=True)
        self.assertTrue(result["success"])
        self.assertEqual(result["tool_calls"], 0)
        self.assertEqual(len(commands), 1)
        result["known_file_answer_verified"] = False
        self.assertFalse(verified_success(result))
        result, commands, _ = self.run_mock_recovery([
            {"command": "cat engine.rs", "answer": "No matching function exists"}])
        self.assertNotIn("known_file_answer_verified", result)
        self.assertEqual(len(commands), 1)

    def test_answer_evidence_cli_rejects_invalid_policy_before_model_launch(self):
        for changes, condition, message in (
                ({}, "graph_cli", "known-file policies support only"),
                ({"source_path": "absent.rs"}, "default", "requires an existing"),
                ({"verdict": "no", "symbol": "missing"}, "default", "positive known-file")):
            with tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                (root / "engine.rs").write_text("fn ready() {}\n")
                exe = root / "fake-bin"
                exe.touch()
                task = {"id": "control", "kind": "known_file", "cohort": "compiler",
                        "source_path": "engine.rs", "prompt": "Identify a function in engine.rs.", **changes}
                manifest = root / "tasks.json"
                manifest.write_text(json.dumps({"schema_version": 1, "tasks": [task]}))
                argv = ["codegraph_eval.py", "run", "--repo", str(root), "--tasks", str(manifest),
                        "--output", str(root / "output"), "--arch-bin", str(exe),
                        "--mcp-python", str(exe), "--condition", condition, "--verify-known-file-answer"]
                err = io.StringIO()
                with patch("sys.argv", argv), patch("codegraph_eval.subprocess.Popen") as launch, \
                        redirect_stderr(err), self.assertRaises(SystemExit) as raised:
                    main()
                self.assertEqual(raised.exception.code, 2)
                self.assertIn(message, err.getvalue())
                launch.assert_not_called()
                self.assertFalse((root / "output").exists())

    def test_recovery_counts_both_stages_and_does_not_supply_answer_key(self):
        for condition in ("no_graph", "default"):
            result, commands, files = self.run_mock_recovery([
                {"answer": "emit_body_ready"},
                {"command": "rg -n '^fn ' engine.rs", "answer": "emit_ready_body"},
            ], condition=condition)
            self.assertTrue(result["success"])
            self.assertEqual(result["input_tokens"], 200)
            self.assertEqual(result["cached_input_tokens"], 120)
            self.assertEqual(result["output_tokens"], 20)
            self.assertEqual(result["tool_calls"], 1)
            self.assertEqual(result["model_attempts"], 2)
            self.assertEqual(result["recovery_attempts"], 1)
            self.assertEqual([s["source_verified"] for s in result["stages"]], [False, True])
            self.assertEqual(len(files), 2)
            self.assertIn("did not provide a recognized successful direct read", commands[1][-1])
            self.assertNotIn("emit_ready_body", commands[1][-1])
            self.assertEqual(commands[0][:-1], commands[1][:-1])
            arm = summarize([result])["all"]["arms"][condition]
            self.assertEqual((arm["tokens"], arm["uncached_tokens"]), (220, 100))
            self.assertEqual((arm["model_attempts"], arm["recovery_attempts"]), (2, 1))

    def test_recovery_stops_after_one_retry_and_rejects_unrelated_evidence(self):
        for command in (None, "echo engine.rs", "rg -n ready other.rs", "arch graph dev-query ready"):
            result, commands, _ = self.run_mock_recovery([
                {"answer": "emit_ready_body"},
                {"answer": "emit_ready_body", "command": command},
            ])
            self.assertFalse(result["success"])
            self.assertFalse(result["known_file_source_verified"])
            self.assertEqual(len(commands), 2)
            self.assertEqual(result["input_tokens"], 200)

    def test_recovery_uses_final_answer_and_usage_not_rejected_stage(self):
        for last in ({"command": "cat engine.rs"},
                     {"command": "cat engine.rs", "answer": "emit_body_ready"},
                     {"command": "cat engine.rs", "answer": "emit_ready_body", "usage": False},
                     {"command": "cat engine.rs", "answer": "emit_ready_body", "exit": 1}):
            result, _, _ = self.run_mock_recovery([{"answer": "emit_ready_body"}, last])
            self.assertFalse(result["success"])
            self.assertEqual(result["answer"], last.get("answer", ""))

    def test_recovery_does_not_retry_verified_reads_or_infrastructure_failures(self):
        for stage in ({"command": "cat engine.rs", "answer": "emit_ready_body"},
                      {"command": "cat engine.rs", "answer": "emit_body_ready"},
                      {"answer": "emit_ready_body", "exit": 1},
                      {"answer": "emit_ready_body", "exit": 124},
                      {"answer": "emit_ready_body", "usage": False}):
            result, commands, _ = self.run_mock_recovery([stage])
            self.assertEqual(len(commands), 1)
            self.assertEqual(result["recovery_attempts"], 0)
            self.assertEqual(result["success"], stage.get("command") is not None and stage["answer"] == "emit_ready_body")

    def test_recovery_is_opt_in_and_does_not_change_discovery(self):
        for enabled, kind in ((False, "known_file"), (True, "discovery")):
            result, commands, _ = self.run_mock_recovery([{"answer": "emit_ready_body"}], enabled, kind=kind)
            self.assertEqual(len(commands), 1)
            self.assertNotIn("stages", result)
            self.assertNotIn("recovery_attempts", result)
            self.assertNotIn("model_attempts", summarize([result])["all"]["arms"]["default"])

    def test_recovery_does_not_spend_another_call_on_forbidden_graph_use(self):
        result, commands, _ = self.run_mock_recovery([
            {"command": "arch graph dev-query ready", "answer": "emit_ready_body"},
        ], condition="no_graph")
        self.assertEqual(len(commands), 1)
        self.assertEqual(result["recovery_attempts"], 0)
        self.assertFalse(result["known_file_source_verified"])
        self.assertFalse(result["valid_method"])
        self.assertFalse(result["success"])

    def test_source_metadata_rejects_invalid_paths_and_cohorts(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "tasks.json"
            for source, kind in (("../secret", "known_file"), ("/tmp/secret", "known_file"),
                                 (None, "known_file"), ("", "known_file"), ("file.rs", "discovery")):
                task = {"id": "x", "cohort": "compiler", "kind": kind, "source_path": source}
                path.write_text(json.dumps({"schema_version": 1, "tasks": [task]}))
                with self.assertRaisesRegex(ValueError, "invalid known-file source_path"):
                    load_tasks(path)

    def test_recovery_cli_rejects_unsupported_conditions_and_missing_sources_before_launch(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            exe = root / "fake-bin"
            exe.touch()
            for conditions, expected in ((["--condition", "graph_cli"], "only no_graph/default"),
                                         ([], "existing in-repository source_path")):
                argv = ["codegraph_eval.py", "run", "--repo", str(root),
                        "--tasks", str(ROOT / "doc/codegraph_eval_cross_file_tasks.json"),
                        "--output", str(root / "output"), "--arch-bin", str(exe),
                        "--mcp-python", str(exe), "--recover-known-file", *conditions]
                err = io.StringIO()
                with patch("sys.argv", argv), patch("codegraph_eval.subprocess.Popen") as launch, \
                        redirect_stderr(err), self.assertRaises(SystemExit) as raised:
                    main()
                self.assertEqual(raised.exception.code, 2)
                self.assertIn(expected, err.getvalue())
                launch.assert_not_called()
                self.assertFalse((root / "output").exists())

    def test_recovery_cli_records_maximum_model_budget_and_keeps_schedule(self):
        template, _, _ = self.run_mock_recovery([
            {"command": "cat engine.rs", "answer": "emit_ready_body"}], enabled=False)
        for enabled, prefetched, evidence, maximum in (
                (False, False, False, 28), (False, True, False, 28),
                (True, False, False, 36), (True, True, False, 36),
                (False, False, True, 36), (True, False, True, 36), (False, True, True, 36)):
            with tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                exe = root / "fake-bin"
                exe.touch()
                argv = ["codegraph_eval.py", "run", "--repo", str(ROOT),
                        "--tasks", str(ROOT / "doc/codegraph_eval_cross_file_tasks.json"),
                        "--output", str(root / "output"), "--arch-bin", str(exe),
                        "--mcp-python", str(exe), "--repeats", "2"]
                if enabled:
                    argv.append("--recover-known-file")
                if prefetched:
                    argv.append("--prefetch-known-file")
                if evidence:
                    argv.append("--verify-known-file-answer")
                def attempt(task, condition, repeat, *args, **kwargs):
                    return {**template, "task_id": task["id"], "condition": condition,
                            "repeat": repeat, "kind": task["kind"], "cohort": task["cohort"]}
                with patch("sys.argv", argv), redirect_stdout(io.StringIO()), \
                        patch("codegraph_eval.subprocess.check_output", side_effect=["abc", "", "codex test", "arch test"]), \
                        patch("codegraph_eval.prepare_indexes", return_value=0), \
                        patch("codegraph_eval.run_attempt", side_effect=attempt) as run:
                    main()
                metadata = json.loads((root / "output/metadata.json").read_text())
                self.assertEqual(metadata["max_model_attempts"], maximum)
                self.assertEqual(metadata["recover_known_file"], enabled)
                self.assertEqual(metadata["prefetch_known_file"], prefetched)
                self.assertEqual(metadata["verify_known_file_answer"], evidence)
                self.assertEqual(metadata["answer_evidence_policy"], ANSWER_EVIDENCE_POLICY if evidence else None)
                self.assertEqual(metadata["prefetch_byte_budget"], 8192)
                self.assertEqual(run.call_count, 28)
                self.assertTrue(all(call.args[-1] == enabled for call in run.call_args_list))
                self.assertTrue(all(call.kwargs["prefetch_known_file"] == prefetched for call in run.call_args_list))
                self.assertTrue(all(call.kwargs["verify_known_file_answer"] == evidence for call in run.call_args_list))

    def test_prefetch_preserves_whole_source_and_lf_line_numbers(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for raw in (b"// comment\r\nfn answer() {}\r\n", "// \u03bb\u2028text\nfn answer() {}".encode(), b"first\n\n"):
                (root / "small.rs").write_bytes(raw)
                result = prefetch_source(root, "small.rs", 8192)
                self.assertEqual(result["status"], "ready")
                self.assertEqual(result["sha256"], hashlib.sha256(raw).hexdigest())
                self.assertEqual(result["read_calls"], 1)
                data = json.loads(result["payload"])
                reconstructed = "\n".join(line for _, line in data["lines"])
                if data["ends_with_newline"]:
                    reconstructed += "\n"
                self.assertEqual(reconstructed.encode(), raw)
                self.assertEqual(data["lines"][1][0], 2)
                self.assertTrue(data["complete"])
                self.assertLessEqual(result["payload_bytes"], 8192)

    def test_prefetch_never_supplies_partial_or_nontext_content(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for raw, budget, status in ((b"x" * 100, 99, "over_budget"),
                                        (b"fn a() {}", 32, "over_budget"),
                                        (b"\xff", 8192, "non_text"), (b"\0", 8192, "non_text"),
                                        (b" \n", 8192, "empty")):
                (root / "small.rs").write_bytes(raw)
                result = prefetch_source(root, "small.rs", budget)
                self.assertEqual(result["status"], status)
                self.assertEqual(result["payload"], "")
                self.assertEqual(result["payload_bytes"], 0)
            for budget in (0, 65537):
                with self.assertRaises(ValueError):
                    prefetch_source(root, "small.rs", budget)
            self.assertEqual(prefetch_source(root, "absent.rs", 8192)["status"], "unavailable")
            with patch.object(Path, "open", side_effect=PermissionError):
                result = prefetch_source(root, "small.rs", 8192)
            self.assertEqual(result["status"], "unavailable")
            self.assertEqual(result["payload"], "")

    def test_prefetch_rejects_escape_and_graph_index_sources(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "repo").mkdir()
            (root / "outside.rs").write_text("fn secret() {}")
            (root / "repo/link.rs").symlink_to(root / "outside.rs")
            for name in ("link.rs", "../outside.rs"):
                self.assertEqual(prefetch_source(root / "repo", name, 8192)["status"], "outside_workspace")
            for folder in (".archgraph", ".archdevgraph", ".git"):
                (root / "repo" / folder).mkdir()
                (root / "repo" / folder / "nodes").write_text("index data")
                self.assertEqual(prefetch_source(root / "repo", f"{folder}/nodes", 8192)["status"], "excluded_path")
            (root / "repo/alias.rs").symlink_to(root / "repo/.archgraph/nodes")
            self.assertEqual(prefetch_source(root / "repo", "alias.rs", 8192)["status"], "excluded_path")

    def test_prefetched_answer_requires_no_model_tool_but_counts_host_reads(self):
        for condition in ("no_graph", "default"):
            result, commands, _ = self.run_mock_recovery([
                {"answer": "emit_ready_body"}], condition=condition, prefetch=True)
            self.assertTrue(result["success"])
            self.assertTrue(verified_success(result))
            self.assertTrue(result["prefetched_source_verified"])
            self.assertEqual(result["tool_calls"], 0)
            self.assertEqual(result["graph_calls"], 0)
            self.assertEqual(len(commands), 1)
            self.assertIn("already read the complete", commands[0][-1])
            self.assertIn("fn emit_ready_body() {}", commands[0][-1])
            self.assertNotIn("execute one targeted source read", commands[0][-1])
            arm = summarize([result])["all"]["arms"][condition]
            self.assertEqual(arm["host_source_reads"], 2)
            self.assertEqual(arm["prefetched_controls"], 1)
            self.assertEqual(arm["tokens"], 110)
            self.assertEqual(arm["uncached_tokens"], 50)
            self.assertGreater(arm["prefetch_payload_bytes"], 0)

    def test_prefetch_does_not_accept_stale_incorrect_or_usage_free_answers(self):
        for stage in ({"answer": "emit_ready_body", "source_after": "fn changed() {}"},
                      {"answer": "emit_body_ready"},
                      {"answer": "emit_ready_body", "usage": False},
                      {"answer": "emit_ready_body", "exit": 1}):
            result, commands, _ = self.run_mock_recovery([stage], prefetch=True)
            self.assertFalse(result["success"])
            self.assertFalse(verified_success(result))
            self.assertEqual(len(commands), 1)
            if "source_after" in stage:
                self.assertFalse(result["prefetch"]["fresh"])
                self.assertFalse(result["known_file_source_verified"])

    def test_prefetch_budget_miss_falls_back_and_can_use_bounded_recovery(self):
        result, commands, _ = self.run_mock_recovery([
            {"answer": "emit_body_ready"},
            {"command": "cat engine.rs", "answer": "emit_ready_body"}], prefetch=True, byte_budget=1)
        self.assertTrue(result["success"])
        self.assertFalse(result["prefetched_source_verified"])
        self.assertEqual(result["prefetch"]["status"], "over_budget")
        self.assertEqual(result["prefetch"]["payload_bytes"], 0)
        self.assertNotIn("Source evidence", commands[0][-1])
        self.assertEqual(result["recovery_attempts"], 1)
        self.assertEqual(result["input_tokens"], 200)
        result, commands, _ = self.run_mock_recovery([
            {"answer": "emit_ready_body"}], enabled=False, prefetch=True, byte_budget=1)
        self.assertFalse(result["success"])
        self.assertEqual(len(commands), 1)

    def test_prefetch_has_no_answer_key_dependency_or_discovery_side_effects(self):
        task = {"kind": "known_file", "cohort": "compiler", "prompt": "Find the declaration in a.rs.", "answer_any": [["gold"]]}
        original = prompt_for(task, "default", source_payload='{"lines": [[1, "fn actual() {}"]]}')
        task["answer_any"] = [["different secret"]]
        self.assertEqual(original, prompt_for(task, "default", source_payload='{"lines": [[1, "fn actual() {}"]]}'))
        self.assertNotIn("gold", original)
        result, commands, _ = self.run_mock_recovery([
            {"answer": "emit_ready_body"}], kind="discovery", prefetch=True)
        self.assertNotIn("prefetch", result)
        self.assertNotIn("Source evidence", commands[0][-1])
        for condition in ("graph_cli", "graph_mcp"):
            with self.assertRaises(ValueError):
                prompt_for(task, condition, source_payload="source")

    def test_new_known_file_manifest_covers_small_sources_and_large_fallbacks(self):
        tasks = load_tasks(ROOT / "doc/codegraph_eval_known_file_tasks.json")
        self.assertEqual(len(tasks), 6)
        self.assertEqual({t["kind"] for t in tasks}, {"known_file"})
        previous_paths = {t.get("source_path") for t in load_tasks(ROOT / "doc/codegraph_eval_cross_file_tasks.json")}
        self.assertTrue(all(t["source_path"] not in previous_paths for t in tasks))
        for task in tasks:
            receipt = prefetch_source(ROOT, task["source_path"], 8192)
            self.assertEqual(receipt["status"], "over_budget" if "large_source" in task["id"] else "ready")
            if receipt["status"] == "ready":
                self.assertLessEqual(receipt["payload_bytes"], 8192)
            source = (ROOT / task["source_path"]).read_text()
            if task["cohort"] == "user":
                _, number = task["answer_any"][0][0].rsplit(":", 1)
                self.assertRegex(source.splitlines()[int(number) - 1], r"^(ram|synchronizer|module) ")
            else:
                self.assertIn("fn " + task["answer_any"][0][0] + "(", source)

    def test_prefetch_budget_includes_json_rendering(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "a.rs").write_text("fn a() {}\n")
            receipt = prefetch_source(root, "a.rs", 8192)
            exact = receipt["payload_bytes"]
            self.assertEqual(prefetch_source(root, "a.rs", exact)["status"], "ready")
            self.assertEqual(prefetch_source(root, "a.rs", exact - 1)["status"], "over_budget")

    def test_paired_source_arm_is_not_graph_savings_and_requires_both_correct(self):
        baseline, _, _ = self.run_mock_recovery([
            {"command": "cat engine.rs", "answer": "emit_ready_body"}], condition="no_graph")
        prefetched, commands, _ = self.run_mock_recovery([
            {"answer": "emit_ready_body"}], condition="source_prefetch")
        self.assertTrue(prefetched["success"])
        self.assertIn("Graph commands, graph MCP tools", commands[0][-1])
        baseline.update(input_tokens=200, cached_input_tokens=120)
        result = summarize([baseline, prefetched])["all"]
        self.assertEqual(result["source_prefetch_vs_search"]["pairs"], 1)
        self.assertEqual(result["source_prefetch_vs_search"]["search_tokens"], 210)
        self.assertEqual(result["source_prefetch_vs_search"]["prefetch_tokens"], 110)
        self.assertEqual(result["graph_used_successful_pairs"], 0)
        self.assertIsNone(result["graph_used_pair_saving_pct"])
        self.assertEqual(result["arms"]["source_prefetch"]["prefetched_controls"], 1)
        baseline["success"] = False
        result = summarize([baseline, prefetched])["all"]["source_prefetch_vs_search"]
        self.assertEqual(result["pairs"], 0)
        self.assertIsNone(result["total_saving_pct"])

    def test_source_arm_rejects_forbidden_graph_and_non_known_tasks(self):
        result, commands, _ = self.run_mock_recovery([
            {"command": "arch graph dev-query ready", "answer": "emit_ready_body"}], condition="source_prefetch")
        self.assertFalse(result["success"])
        self.assertFalse(result["valid_method"])
        self.assertEqual(len(commands), 1)
        with self.assertRaisesRegex(ValueError, "requires known-file"):
            self.run_mock_recovery([], condition="source_prefetch", kind="discovery")

    def test_prefetch_cli_prevents_invalid_budget_and_prefetched_baseline(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            exe = root / "bin"
            exe.touch()
            base = ["codegraph_eval.py", "run", "--repo", str(ROOT),
                    "--tasks", str(ROOT / "doc/codegraph_eval_known_file_tasks.json"),
                    "--output", str(root / "out"), "--arch-bin", str(exe), "--mcp-python", str(exe)]
            for extra, message in ((["--prefetch-byte-budget", "0"], "between 1 and 65536"),
                                   (["--condition", "source_prefetch", "--prefetch-known-file"], "keep the baseline unprefetched"),
                                   (["--condition", "source_prefetch", "--tasks", str(ROOT / "doc/codegraph_eval_cross_file_tasks.json")], "only known-file")):
                err = io.StringIO()
                with patch("sys.argv", base + extra), redirect_stderr(err), \
                        patch("codegraph_eval.subprocess.Popen") as launch, \
                        self.assertRaises(SystemExit) as raised:
                    main()
                self.assertEqual(raised.exception.code, 2)
                self.assertIn(message, err.getvalue())
                launch.assert_not_called()

    def test_source_prefetch_cli_schedules_a_paired_bounded_experiment(self):
        template, _, _ = self.run_mock_recovery([
            {"command": "cat engine.rs", "answer": "emit_ready_body"}])
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            exe = root / "bin"
            exe.touch()
            argv = ["codegraph_eval.py", "run", "--repo", str(ROOT),
                    "--tasks", str(ROOT / "doc/codegraph_eval_known_file_tasks.json"),
                    "--output", str(root / "out"), "--arch-bin", str(exe), "--mcp-python", str(exe),
                    "--condition", "no_graph", "--condition", "source_prefetch", "--recover-known-file"]
            def attempt(task, condition, repeat, *args, **kwargs):
                return {**template, "task_id": task["id"], "kind": task["kind"],
                        "cohort": task["cohort"], "condition": condition, "repeat": repeat}
            with patch("sys.argv", argv), redirect_stdout(io.StringIO()), \
                    patch("codegraph_eval.subprocess.check_output", side_effect=["abc", "", "codex test", "arch test"]), \
                    patch("codegraph_eval.prepare_indexes", return_value=0), \
                    patch("codegraph_eval.run_attempt", side_effect=attempt) as run:
                main()
            meta = json.loads((root / "out/metadata.json").read_text())
            self.assertEqual(meta["conditions"], ["no_graph", "source_prefetch"])
            self.assertEqual(meta["max_model_attempts"], 24)
            self.assertFalse(meta["prefetch_known_file"])
            self.assertEqual(run.call_count, 12)
            self.assertEqual(sum(call.args[1] == "source_prefetch" for call in run.call_args_list), 6)

    def test_excerpts_are_numbered_literal_bounded_and_query_only(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            lines = ["// unrelated padding " * 4] * 100
            lines[50] = "pub fn adapt(input: WireSpan) -> ReportRange {\r"
            raw = ("\n".join(lines) + "\n").encode()
            (root / "sample.rs").write_bytes(raw)
            query = "In sample.rs, find the function from WireSpan to ReportRange."
            result = prefetch_source(root, "sample.rs", 2048, query)
            self.assertEqual(result["status"], "ready")
            self.assertFalse(result["complete"])
            payload = json.loads(result["payload"])
            self.assertFalse(payload["complete"])
            self.assertEqual(payload["total_lines"], 100)
            self.assertEqual(payload["sha256"], hashlib.sha256(raw).hexdigest())
            self.assertEqual(dict(payload["lines"])[51], lines[50])
            self.assertTrue(all(lines[n - 1] == text for n, text in payload["lines"]))
            self.assertLessEqual(len(payload["lines"]), 20)
            self.assertLessEqual(result["payload_bytes"], 2048)
            self.assertEqual(result["read_bytes"], len(raw))
            self.assertEqual(result, prefetch_source(root, "sample.rs", 2048, query))
            self.assertEqual(prefetch_source(root, "sample.rs", 2048)["status"], "over_budget")
            self.assertEqual(prefetch_source(root, "sample.rs", 2048, "In sample.rs find absent_marker")["status"], "no_excerpt")
            self.assertEqual(prefetch_source(root, "sample.rs", 1, query)["status"], "no_excerpt")

    def test_excerpts_do_not_starve_exact_type_pairs_with_common_variables(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            lines = ["let wire = ReportRange::new(0); // repeated"] * 100
            lines[70] = "fn adapter(wire: Wire) -> ReportRange {"
            (root / "a.rs").write_text("\n".join(lines))
            result = prefetch_source(root, "a.rs", 1500, "Convert Wire into ReportRange in a.rs")
            self.assertIn(71, result["selected_lines"])

    def test_excerpt_limits_nontext_and_path_guards(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            query = "In a.rs find target"
            for content, status in ((b"x" * (1024 * 1024 + 2), "source_too_large"),
                                    (b"target\n" * 32769, "excerpt_limit"),
                                    (b"target\0", "non_text"), (b"\xff", "non_text")):
                (root / "a.rs").write_bytes(content)
                receipt = prefetch_source(root, "a.rs", 512, query)
                self.assertEqual(receipt["status"], status)
                self.assertFalse(receipt["payload"])
                self.assertLessEqual(receipt["read_bytes"], 1024 * 1024 + 1)
            (root / "a.rs").write_text("target\n" * 100)
            for oversized_query in ("target " * 2000, " ".join(f"term{i}" for i in range(65))):
                self.assertEqual(prefetch_source(root, "a.rs", 512, oversized_query)["status"], "excerpt_limit")
            (root / "nested").mkdir()
            (root / "nested/link.rs").symlink_to(root / "a.rs")
            self.assertEqual(prefetch_source(root / "nested", "link.rs", 512, query)["status"], "outside_workspace")
            (root / ".git").mkdir()
            (root / ".git/source").write_text("target\n")
            self.assertEqual(prefetch_source(root, ".git/source", 512, query)["status"], "excluded_path")

    def test_excerpt_mode_keeps_small_file_payload_and_baseline_unchanged(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "a.rs").write_text("fn target() {}\n")
            original = prefetch_source(root, "a.rs", 8192)
            current = prefetch_source(root, "a.rs", 8192, "target")
            self.assertEqual(original["payload"], current["payload"])
        stage = {"command": "cat engine.rs", "answer": self.ready_citation()}
        original, original_commands, _ = self.run_mock_recovery([stage], condition="no_graph", verify_answer=True)
        current, current_commands, _ = self.run_mock_recovery([stage], condition="no_graph", verify_answer=True, excerpts=True)
        self.assertEqual([c[-1] for c in original_commands], [c[-1] for c in current_commands])
        self.assertNotIn("prefetch", current)
        self.assertEqual(current["success"], original["success"])

    @staticmethod
    def excerpt_source():
        return "// Engine ready body\nfn emit_ready_body() {}\n" + "// filler\n" * 120 + "fn unseen() {}\n"

    def test_excerpt_citation_can_avoid_tool_call_without_gold_selection(self):
        result, commands, _ = self.run_mock_recovery([
            {"answer": self.ready_citation()}], condition="source_prefetch", verify_answer=True,
            excerpts=True, source=self.excerpt_source(), byte_budget=512)
        self.assertTrue(result["success"])
        self.assertFalse(result["prefetch"]["complete"])
        self.assertTrue(result["stages"][0]["prefetch_answer_covered"])
        self.assertEqual(result["tool_calls"], 0)
        self.assertEqual(result["prefetch"]["read_calls"], 2)
        self.assertIn("NOT the complete file", commands[0][-1])
        self.assertNotIn("read the complete named source", commands[0][-1])
        changed, altered_commands, _ = self.run_mock_recovery([
            {"answer": self.ready_citation()}], condition="source_prefetch", verify_answer=True,
            excerpts=True, source=self.excerpt_source(), byte_budget=512, answer_groups=[["wrong_gold"]])
        self.assertEqual([c[-1] for c in commands], [c[-1] for c in altered_commands])
        self.assertEqual(result["prefetch"], changed["prefetch"])
        self.assertFalse(changed["success"])
        self.assertEqual(changed["model_attempts"], 1)
        arm = summarize([result])["all"]["arms"]["source_prefetch"]
        self.assertEqual(arm["excerpt_prefetched_controls"], 1)
        self.assertEqual(arm["prefetch_read_bytes"], 2 * len(self.excerpt_source().encode()))
        self.assertEqual(arm["answer_evidence_read_bytes"], len(self.excerpt_source().encode()))
        self.assertEqual(arm["host_read_bytes"], 3 * len(self.excerpt_source().encode()))

    def test_unseen_source_bound_answer_needs_real_read_or_fails(self):
        answer = self.ready_citation(symbol="unseen", line=123, quote="fn unseen() {}")
        for recovery, success in (({"answer": answer}, False),
                                  ({"command": "sed -n '123p' engine.rs", "tool_output": "fn unseen() {}", "answer": answer}, True)):
            result, commands, _ = self.run_mock_recovery([
                {"answer": answer}, recovery], condition="source_prefetch", verify_answer=True,
                excerpts=True, source=self.excerpt_source(), byte_budget=512, answer_groups=[["unseen"]])
            self.assertTrue(result["known_file_answer_verified"])
            self.assertFalse(result["stages"][0]["prefetch_answer_covered"])
            self.assertFalse(result["stages"][0]["source_verified"])
            self.assertEqual(result["success"], success)
            self.assertEqual(len(commands), 2)
            self.assertEqual(result["input_tokens"], 200)

    def test_stale_or_removed_excerpt_source_fails_closed(self):
        for change in ({"source_after": self.excerpt_source() + "// altered outside excerpt\n"},
                       {"remove_source": True}):
            stages = [{"answer": self.ready_citation(), **change}, {"answer": self.ready_citation()}]
            result, _, _ = self.run_mock_recovery(stages, condition="source_prefetch", verify_answer=True,
                                                excerpts=True, source=self.excerpt_source(), byte_budget=512)
            self.assertFalse(result["success"])
            self.assertFalse(result["prefetch"]["fresh"])

    def test_excerpts_require_verified_paired_treatment(self):
        for options in ({"verify_answer": False}, {"verify_answer": True, "prefetch": True},
                        {"verify_answer": True, "condition": "default"}):
            with self.assertRaisesRegex(ValueError, "excerpt prefetch"):
                self.run_mock_recovery([], excerpts=True, **options)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            exe = root / "bin"
            exe.touch()
            argv = ["codegraph_eval.py", "run", "--output", str(root / "out"), "--arch-bin", str(exe),
                    "--mcp-python", str(exe), "--prefetch-excerpts"]
            for extra in ([], ["--verify-known-file-answer"], ["--condition", "no_graph", "--condition", "source_prefetch"],
                          ["--verify-known-file-answer", "--condition", "no_graph", "--condition", "source_prefetch",
                           "--condition", "source_prefetch"]):
                with patch("sys.argv", argv + extra), redirect_stderr(io.StringIO()), \
                        patch("codegraph_eval.subprocess.Popen") as launch, self.assertRaises(SystemExit):
                    main()
                launch.assert_not_called()

    def test_excerpt_failures_keep_usage_and_method_gates(self):
        for change in ({"usage": False}, {"exit": 1}, {"command": "arch graph dev-query ready"}):
            result, commands, _ = self.run_mock_recovery([
                {"answer": self.ready_citation(), **change}], condition="source_prefetch", verify_answer=True,
                excerpts=True, source=self.excerpt_source(), byte_budget=512)
            self.assertFalse(result["success"])
            self.assertEqual(len(commands), 1)
        result, commands, _ = self.run_mock_recovery([
            {"answer": "null"}, {"answer": self.ready_citation()}], condition="source_prefetch", verify_answer=True,
            excerpts=True, source=self.excerpt_source(), byte_budget=512)
        self.assertTrue(result["success"])
        self.assertEqual(len(commands), 2)
        self.assertEqual(result["input_tokens"], 200)

    def test_excerpt_cli_records_policy_and_does_not_increase_call_cap(self):
        template, _, _ = self.run_mock_recovery([
            {"command": "cat engine.rs", "answer": self.ready_citation()}], verify_answer=True)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            exe = root / "bin"
            exe.touch()
            argv = ["codegraph_eval.py", "run", "--repo", str(ROOT),
                    "--tasks", str(ROOT / "doc/codegraph_eval_known_file_tasks.json"),
                    "--output", str(root / "out"), "--arch-bin", str(exe), "--mcp-python", str(exe),
                    "--condition", "no_graph", "--condition", "source_prefetch", "--prefetch-excerpts",
                    "--verify-known-file-answer"]
            with patch("sys.argv", argv), redirect_stdout(io.StringIO()), \
                    patch("codegraph_eval.subprocess.check_output", side_effect=["abc", "", "codex test", "arch test"]), \
                    patch("codegraph_eval.prepare_indexes", return_value=0), \
                    patch("codegraph_eval.run_attempt", return_value=template) as run:
                main()
            meta = json.loads((root / "out/metadata.json").read_text())
            self.assertEqual(meta["prefetch_policy"], "query-windows-v1")
            self.assertEqual(meta["prefetch_excerpt_condition"], "source_prefetch")
            self.assertEqual(meta["answer_evidence_policy"], ANSWER_EVIDENCE_POLICY)
            self.assertEqual(meta["max_model_attempts"], 24)
            self.assertEqual(run.call_count, 12)
            self.assertTrue(all(call.kwargs["prefetch_excerpts"] for call in run.call_args_list))

    def test_excerpt_selection_does_not_clip_oversized_lines(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "a.rs").write_text("target " + "x" * 4000 + "\n" + "padding\n" * 8 + "fn target() {}\n")
            receipt = prefetch_source(root, "a.rs", 512, "target")
            payload = json.loads(receipt["payload"])
            self.assertNotIn(1, dict(payload["lines"]))
            self.assertEqual(dict(payload["lines"])[10], "fn target() {}")
            self.assertLessEqual(receipt["payload_bytes"], 512)

    def test_excerpt_quote_must_match_supplied_snapshot_even_after_restoration(self):
        original = self.excerpt_source()
        altered = original.replace("fn emit_ready_body() {}", "fn emit_ready_body() { changed(); }")
        answer = self.ready_citation(quote="fn emit_ready_body() { changed(); }")
        def change_and_restore(answer, workspace, source_path):
            path = workspace / source_path
            path.write_text(altered)
            result = known_file_answer_evidence(answer, workspace, source_path)
            path.write_text(original)
            return result
        with patch("codegraph_eval.known_file_answer_evidence", side_effect=change_and_restore):
            result, commands, _ = self.run_mock_recovery([
                {"answer": answer}, {"answer": answer}], condition="source_prefetch", verify_answer=True,
                excerpts=True, source=original, byte_budget=512)
        self.assertEqual(len(commands), 2)
        self.assertTrue(result["known_file_answer_verified"])
        self.assertFalse(result["success"])
        self.assertFalse(result["prefetch"]["fresh"])
        self.assertTrue(all(not stage["prefetch_answer_covered"] for stage in result["stages"]))

    def test_comment_witness_is_not_a_semantic_declaration_proof(self):
        source = self.excerpt_source().replace("// Engine ready body", "// ready body emit_ready_body")
        answer = self.ready_citation(line=1, quote="// ready body emit_ready_body")
        result, commands, _ = self.run_mock_recovery([
            {"answer": answer}], condition="source_prefetch", verify_answer=True,
            excerpts=True, source=source, byte_budget=512, answer_groups=[["engine.rs:2"]])
        self.assertTrue(result["known_file_answer_verified"])
        self.assertFalse(result["success"])
        self.assertEqual(len(commands), 1)  # No answer-key-guided retry.

    def test_existing_controls_have_bounded_source_coverage_without_changing_small_payloads(self):
        for task in load_tasks(ROOT / "doc/codegraph_eval_known_file_tasks.json"):
            original = prefetch_source(ROOT, task["source_path"], 8192)
            current = prefetch_source(ROOT, task["source_path"], 8192, task["prompt"])
            self.assertEqual(current["status"], "ready", task["id"])
            self.assertLessEqual(current["payload_bytes"], 8192)
            if original["status"] == "ready":
                self.assertEqual(current["payload"], original["payload"])
            else:
                rows = json.loads(current["payload"])["lines"]
                self.assertFalse(current["complete"])
                observed = "\n".join(f"{task['source_path']}:{n} {line}" for n, line in rows)
                self.assertTrue(answer_passes(task, observed))

    def test_held_out_manifest_pins_exact_declarations_and_disjoint_paths(self):
        path = ROOT / "doc/codegraph_eval_held_out_source_tasks.json"
        self.assertEqual(hashlib.sha256(path.read_bytes()).hexdigest(),
                         "55d46fc805d51e48e24ff927da84ffc9caf29c6537d4a8b47e7994f8f11e3859")
        tasks = load_tasks(path)
        self.assertEqual(len(tasks), 6)
        self.assertEqual(sum(t["cohort"] == "user" for t in tasks), 3)
        self.assertEqual(len(validate_source_pins(tasks, ROOT)), 6)
        prior_paths = {t.get("source_path") for manifest in (
            "codegraph_eval_tasks.json", "codegraph_eval_cross_file_tasks.json", "codegraph_eval_known_file_tasks.json")
            for t in load_tasks(ROOT / "doc" / manifest)}
        prefixes = {
            "held_out_rom_declaration": "ram RomLut",
            "held_out_block_decoder_declaration": "module Lz4BlockDecoder",
            "held_out_mesh_declaration": "module Mesh4x4",
            "held_out_clock_gate_emitter": "pub(crate) fn emit_clkgate(",
            "held_out_format_tag_lookup": "pub fn by_tag(",
            "held_out_async_fifo_detection": "pub fn detect_async_fifo(",
        }
        for task in tasks:
            self.assertNotIn(task["source_path"], prior_paths)
            expected = task["expected_citation"]
            symbol, line = expected["symbol"], expected["line"]
            location = f"{expected['path']}:{line}"
            source_line = (ROOT / task["source_path"]).read_text().split("\n")[line - 1].strip()
            self.assertTrue(source_line.startswith(prefixes[task["id"]]))
            witness = json.dumps({"symbol": symbol, "path": task["source_path"], "line": line, "quote": source_line})
            evidence = known_file_answer_evidence(witness, ROOT, task["source_path"])
            self.assertTrue(evidence["verified"])
            self.assertTrue(answer_passes(task, evidence["graded_answer"]))
            self.assertFalse(answer_passes(task, f"{symbol} {task['source_path']}:{line + 1}"))
            self.assertFalse(answer_passes(task, f"wrong_symbol {location}"))
            self.assertFalse(answer_passes(task, f"{symbol.swapcase()} {location}"))
            self.assertFalse(answer_passes(task, f"{symbol} {location} extra"))
            original = prompt_for(task, "no_graph", verify_known_file_answer=True)
            altered = {**task, "expected_citation": {**expected, "symbol": "different_gold"}, "source_sha256": "0" * 64}
            self.assertEqual(original, prompt_for(altered, "no_graph", verify_known_file_answer=True))

    def test_source_pin_schema_and_preflight_fail_closed(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            raw = b"fn ready() {}\n"
            (root / "a.rs").write_bytes(raw)
            task = {"id": "pinned", "kind": "known_file", "cohort": "compiler", "source_path": "a.rs",
                    "source_sha256": hashlib.sha256(raw).hexdigest(), "prompt": "Find ready"}
            pins = validate_source_pins([task], root)
            self.assertEqual(pins["pinned"]["read_bytes"], len(raw))
            for value in (None, 1, "bad", "A" * 64):
                manifest = root / "tasks.json"
                manifest.write_text(json.dumps({"schema_version": 1, "tasks": [{**task, "source_sha256": value}]}))
                with self.assertRaisesRegex(ValueError, "source_sha256"):
                    load_tasks(manifest)
            (root / "a.rs").write_bytes(raw + b"// changed\n")
            with self.assertRaisesRegex(ValueError, "source pin failed"):
                validate_source_pins([task], root)
            (root / "a.rs").unlink()
            with self.assertRaises(ValueError):
                validate_source_pins([task], root)
            (root / "outside.rs").write_bytes(raw)
            (root / "repo").mkdir()
            (root / "repo/link.rs").symlink_to(root / "outside.rs")
            for name in ("link.rs", "../outside.rs", str(root / "outside.rs")):
                with self.assertRaises(ValueError):
                    validate_source_pins([{**task, "source_path": name}], root / "repo")
            (root / ".git").mkdir()
            (root / ".git/a.rs").write_bytes(raw)
            with self.assertRaises(ValueError):
                validate_source_pins([{**task, "source_path": ".git/a.rs"}], root)
            oversized = b"x" * (1024 * 1024 + 1)
            (root / "a.rs").write_bytes(oversized)
            with self.assertRaises(ValueError):
                validate_source_pins([{**task, "source_sha256": hashlib.sha256(oversized).hexdigest()}], root)

    def test_source_pin_is_checked_again_at_answer_without_oracle_retry(self):
        source = "// Engine\nfn emit_ready_body() {}\n"
        digest = hashlib.sha256(source.encode()).hexdigest()
        for pin, success in ((digest, True), ("0" * 64, False)):
            result, commands, _ = self.run_mock_recovery([
                {"command": "cat engine.rs", "answer": self.ready_citation()}], condition="no_graph",
                verify_answer=True, source=source, source_pin=pin)
            self.assertEqual(result["source_pin_verified"], success)
            self.assertEqual(result["success"], success)
            self.assertEqual(len(commands), 1)
            self.assertEqual(verified_success({**result, "success": True}), success)
        with self.assertRaisesRegex(ValueError, "pinned sources"):
            self.run_mock_recovery([], source_pin=digest)

    def test_pin_failure_precedes_output_indexing_and_model_launch(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            exe = root / "bin"
            exe.touch()
            argv = ["codegraph_eval.py", "run", "--repo", str(ROOT),
                    "--tasks", str(ROOT / "doc/codegraph_eval_held_out_source_tasks.json"),
                    "--output", str(root / "out"), "--arch-bin", str(exe), "--mcp-python", str(exe)]
            for flags in ([], ["--verify-known-file-answer"]):
                with patch("sys.argv", argv + flags), redirect_stderr(io.StringIO()), \
                        patch("codegraph_eval.validate_source_pins", side_effect=ValueError("source pin failed")), \
                        patch("codegraph_eval.prepare_indexes") as setup, \
                        patch("codegraph_eval.run_attempt") as run, self.assertRaises(SystemExit):
                    main()
                setup.assert_not_called()
                run.assert_not_called()
                self.assertFalse((root / "out").exists())

    def test_held_out_schedule_has_36_trials_and_72_call_ceiling(self):
        template, _, _ = self.run_mock_recovery([
            {"command": "cat engine.rs", "answer": self.ready_citation()}], verify_answer=True)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            exe = root / "bin"
            exe.touch()
            argv = ["codegraph_eval.py", "run", "--repo", str(ROOT),
                    "--tasks", str(ROOT / "doc/codegraph_eval_held_out_source_tasks.json"),
                    "--output", str(root / "out"), "--arch-bin", str(exe), "--mcp-python", str(exe),
                    "--condition", "no_graph", "--condition", "source_prefetch", "--prefetch-excerpts",
                    "--verify-known-file-answer", "--repeats", "3", "--seed", "83"]
            with patch("sys.argv", argv), redirect_stdout(io.StringIO()), \
                    patch("codegraph_eval.subprocess.check_output", side_effect=["abc", "", "codex test", "arch test"]), \
                    patch("codegraph_eval.prepare_indexes", return_value=0), \
                    patch("codegraph_eval.run_attempt", return_value=template) as run:
                main()
            meta = json.loads((root / "out/metadata.json").read_text())
            self.assertEqual(meta["max_model_attempts"], 72)
            self.assertEqual(meta["seed"], 83)
            self.assertEqual(len(meta["source_pins"]), 6)
            self.assertEqual(set(meta["answer_grading_policies"].values()), {"exact-citation-v1"})
            self.assertEqual(run.call_count, 36)
            self.assertEqual(len({tuple(call.args[:3][1:]) + (call.args[0]["id"],) for call in run.call_args_list}), 36)

    def test_exact_citation_schema_rejects_ambiguous_or_invalid_gold(self):
        task = {"id": "pinned", "kind": "known_file", "cohort": "compiler", "source_path": "engine.rs"}
        valid = {"symbol": "emit_ready_body", "path": "engine.rs", "line": 2}
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "tasks.json"
            for citation in (None, {}, {**valid, "line": True}, {**valid, "line": 0},
                             {**valid, "path": "different.rs"}, {**valid, "symbol": "bad-name"},
                             {**valid, "quote": "extra"}):
                path.write_text(json.dumps({"schema_version": 1, "tasks": [{**task, "expected_citation": citation}]}))
                with self.assertRaisesRegex(ValueError, "expected_citation"):
                    load_tasks(path)
            path.write_text(json.dumps({"schema_version": 1, "tasks": [
                {**task, "expected_citation": valid, "answer_any": [["ambiguous second rubric"]]}]}))
            with self.assertRaises(ValueError):
                load_tasks(path)

    def test_exact_citation_rejects_comment_without_answer_key_retry(self):
        source = "// emit_ready_body emits ready body\nfn emit_ready_body() {}\n"
        expected = {"symbol": "emit_ready_body", "path": "engine.rs", "line": 2}
        for line, quote, success in ((1, "// emit_ready_body emits ready body", False),
                                     (2, "fn emit_ready_body() {}", True)):
            result, commands, _ = self.run_mock_recovery([
                {"answer": self.ready_citation(line=line, quote=quote)}], condition="source_prefetch",
                verify_answer=True, source=source, expected_citation=expected)
            self.assertTrue(result["known_file_answer_verified"])
            self.assertEqual(result["success"], success)
            self.assertEqual(len(commands), 1)


if __name__ == "__main__":
    unittest.main()
