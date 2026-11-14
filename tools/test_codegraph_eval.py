import io
import json
import os
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from unittest.mock import Mock, patch

from codegraph_eval import answer_passes, graph_command_kind, is_graph_command, known_file_source_read, load_tasks, main, parse_events, prompt_for, run_attempt, summarize, trace_result, verified_success, workspace_env
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

    def run_mock_recovery(self, stages, enabled=True, condition="default", kind="known_file"):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "engine.rs").write_text("// Engine\nfn emit_ready_body() {}\n")
            task = {"id": "held_out", "kind": kind, "cohort": "compiler",
                    "prompt": "In engine.rs, identify the function that emits the ready body.",
                    "source_path": "engine.rs", "answer_any": [["emit_ready_body"]]}
            processes = []
            for stage in stages:
                events = []
                if stage.get("command"):
                    events.append({"type": "item.completed", "item": {
                        "type": "command_execution", "command": stage["command"],
                        "exit_code": stage.get("tool_exit", 0),
                        "aggregated_output": "2:fn emit_ready_body() {}"}})
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
                return processes[len(commands) - 1]
            with patch("codegraph_eval.subprocess.Popen", side_effect=popen):
                result = run_attempt(task, condition, 1, root, root, root / "arch", root / "python",
                                     "test-model", 10, True, recover_known_file=enabled)
            files = {p.name: p.read_text() for p in root.glob("*.jsonl")}
            return result, commands, files

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

    def test_recovery_does_not_allow_earlier_forbidden_graph_use(self):
        result, _, _ = self.run_mock_recovery([
            {"command": "arch graph dev-query ready", "answer": "emit_ready_body"},
            {"command": "cat engine.rs", "answer": "emit_ready_body"},
        ], condition="no_graph")
        self.assertTrue(result["known_file_source_verified"])
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
        for enabled, maximum in ((False, 28), (True, 36)):
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
                def attempt(task, condition, repeat, *args):
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
                self.assertEqual(run.call_count, 28)
                self.assertTrue(all(call.args[-1] == enabled for call in run.call_args_list))


if __name__ == "__main__":
    unittest.main()
