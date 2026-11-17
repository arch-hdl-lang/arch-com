import hashlib
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

from compiler_context import MAX_OUTPUT_BYTES, MAX_SOURCE_BYTES, _index_digest, _run, context_is_fresh, prepare_compiler_prompt, retrieve_compiler_context


class CompilerContextTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        (self.root / "src").mkdir()
        self.index = self.root / ".archdevgraph"
        self.index.mkdir()
        self.lines = {
            "src/ast.rs": "pub struct CounterDecl {\n}\n",
            "src/parser.rs": "fn parse_counter() {\n}\n",
            "src/backend.rs": "fn emit_counter() {\n}\n",
        }
        for path, text in self.lines.items():
            (self.root / path).write_text(text)
        pins = {path: _index_digest(text.encode()) for path, text in self.lines.items()}
        (self.index / "manifest.json").write_text(json.dumps({"sources": pins}))
        for name in ("nodes.jsonl", "edges.jsonl"):
            (self.index / name).write_text("")
        self.output = "feature counter\n" + "\n".join([
            "  ast src/ast.rs:1 CounterDecl [curated] | pub struct CounterDecl {",
            "  parser src/parser.rs:1 parse_counter [curated] | fn parse_counter() {",
            "  backend src/backend.rs:1 emit_counter [curated] | fn emit_counter() {",
        ]) + "\n"

    def retrieve(self, **kwargs):
        return retrieve_compiler_context(self.root, self.root / "arch", "Locate counter owners.", **kwargs)

    def command(self, stdout=None, **overrides):
        return {"command": [], "stdout": self.output if stdout is None else stdout,
                "stderr": "", "exit_code": 0, "error": None, "output_clipped": False, **overrides}

    def test_unknown_ownership_calls_graph_once_with_literal_query(self):
        with patch("compiler_context._run", return_value=self.command()) as execute:
            result = self.retrieve()
        self.assertEqual(result["status"], "ready")
        self.assertEqual(result["host_graph_calls"], 1)
        self.assertEqual(result["source_read_calls"], 3)
        self.assertEqual(result["index_read_calls"], 1)
        self.assertEqual(result["missing_roles"], [])
        command = execute.call_args.args[0]
        self.assertEqual(command[1:4], ["graph", "dev-query", "Locate counter owners."])
        self.assertEqual(command.count("--role"), 3)
        self.assertIn("--best-match", command)
        self.assertIn("src/ast.rs:1:pub struct CounterDecl {", result["payload"])
        self.assertTrue(context_is_fresh(self.root, result))

    def test_index_hash_compatibility(self):
        self.assertEqual(_index_digest(b""), "cbf29ce484222325")
        self.assertEqual(_index_digest(b"a"), "af63dc4c8601ec8c")

    def test_host_prompt_contains_only_fresh_source_not_raw_logs(self):
        with patch("compiler_context._run", return_value=self.command(stderr="diagnostic-not-evidence")):
            prompt, result = prepare_compiler_prompt(self.root, Path("arch"), "counter")
        self.assertIn("CounterDecl", prompt)
        self.assertNotIn("diagnostic-not-evidence", prompt)
        self.assertEqual(result["consumption_check"]["read_calls"], 3)
        self.assertEqual(result["source_hashes"]["src/ast.rs"], hashlib.sha256(self.lines["src/ast.rs"].encode()).hexdigest())
        with patch("compiler_context._run", return_value=self.command()), patch("compiler_context.check_context", return_value={"fresh": False}):
            prompt, stale = prepare_compiler_prompt(self.root, Path("arch"), "counter")
        self.assertNotIn("CounterDecl", prompt)
        self.assertIn("targeted source search", prompt)
        self.assertEqual(stale["status"], "needs_search")
        self.assertEqual(stale["payload"], "")
        self.assertEqual(stale["evidence"], [])
        self.assertEqual(stale["source_hashes"], {})
        self.assertEqual(stale["reason"], "source changed before prompt consumption")
        self.assertTrue(stale["retrieved_source_hashes"])

    def test_known_file_skips_graph_even_without_index_or_binary(self):
        with patch("compiler_context._run", side_effect=AssertionError("must not query")):
            result = self.retrieve(source_path="src/ast.rs", line_limit=1, index=self.root / "missing")
        self.assertEqual(result["status"], "ready")
        self.assertEqual(result["route"], "source")
        self.assertEqual(result["commands"], [])
        self.assertEqual(result["index_read_calls"], 0)
        self.assertEqual(result["source_read_calls"], 1)
        self.assertTrue(result["truncated"])

    def test_failures_do_not_supply_unverified_evidence_or_retry(self):
        cases = [self.command(exit_code=1), self.command(error="timeout"),
                 self.command(output_clipped=True), self.command(""),
                 self.command(self.output.replace("CounterDecl [curated] | pub struct", "CounterDecl [curated] | wrong struct")),
                 self.command(self.output.replace("src/ast.rs:1", "src/ast.rs:999")),
                 self.command(self.output.replace("src/ast.rs:1", "../secret.rs:1")),
                 self.command(self.output.replace("  backend", "feature other\n  backend")),
                 self.command("\n".join(self.output.splitlines()[:-1])),
                 self.command(self.output.replace("CounterDecl [curated]", "WrongDecl [curated]"))]
        cases += [self.command(self.output + "(truncated)\n"),
                  self.command(self.output + self.output.splitlines()[1] + "\n")]
        for call in cases:
            with self.subTest(call=call), patch("compiler_context._run", return_value=call) as execute:
                result = self.retrieve()
                self.assertEqual(execute.call_count, 1)
                self.assertEqual(result["status"], "needs_search")
                self.assertEqual(result["payload"], "")
                self.assertEqual(result["evidence"], [])

    def test_changed_source_and_postflight_change_fail_closed(self):
        with patch("compiler_context._run", return_value=self.command()):
            original = self.retrieve()
            (self.root / "src/ast.rs").write_text("pub struct Changed {}\n")
            self.assertFalse(context_is_fresh(self.root, original))
            result = self.retrieve()
        self.assertEqual(result["status"], "needs_search")
        self.assertEqual(result["reason"], "source changed during retrieval")

    def test_explicit_paths_never_fall_through_to_graph(self):
        outside = self.root / "outside.rs"
        outside.write_text("pub struct Secret;\n")
        (self.root / "src/link.rs").symlink_to(outside)
        for path in ("../outside.rs", str(outside), "src/link.rs", "src/missing.rs", "doc/map.rs"):
            with self.subTest(path=path), patch("compiler_context._run", side_effect=AssertionError("must not query")):
                result = self.retrieve(source_path=path)
                self.assertEqual(result["status"], "needs_search")
                self.assertEqual(result["commands"], [])

    def test_missing_and_symlinked_index_fail_before_command(self):
        with patch("compiler_context._run", side_effect=AssertionError("must not query")):
            self.assertEqual(self.retrieve(index=self.root / "missing")["status"], "needs_search")
            (self.index / "nodes.jsonl").unlink()
            (self.index / "nodes.jsonl").symlink_to(self.root / "src/ast.rs")
            self.assertEqual(self.retrieve()["status"], "needs_search")

    def test_invalid_configuration(self):
        for options in ({"roles": ()}, {"roles": ("oracle",)}, {"roles": ("ast", "ast")},
                        {"byte_budget": 0}, {"byte_budget": 8193}, {"start_line": 0}, {"line_limit": 81}):
            with self.subTest(options=options), self.assertRaises(ValueError):
                self.retrieve(**options)

    def test_utf8_budget_oversized_and_binary_sources(self):
        for text in ("pub struct CounterDecl { // \u03bb\n", "x" * (MAX_SOURCE_BYTES + 1), "a\0b"):
            (self.root / "src/ast.rs").write_bytes(text.encode())
            result = self.retrieve(source_path="src/ast.rs", byte_budget=1)
            self.assertEqual(result["status"], "needs_search")
            self.assertLessEqual(result["payload_bytes"], 1)
            self.assertLessEqual(result["source_read_bytes"], MAX_SOURCE_BYTES + 1)

    def test_process_failure_timeout_and_bounded_output(self):
        missing = _run([str(self.root / "no-binary")], self.root)
        self.assertIsNotNone(missing["error"])
        timed = _run([sys.executable, "-c", "import time; time.sleep(5)"], self.root, timeout=0.05)
        self.assertEqual(timed["error"], "timeout")
        large = _run([sys.executable, "-c", "import sys; print('x'*20000); print('y'*20000,file=sys.stderr)"], self.root)
        self.assertTrue(large["output_clipped"])
        self.assertEqual(len(large["stdout"].encode()), MAX_OUTPUT_BYTES)
        self.assertEqual(len(large["stderr"].encode()), MAX_OUTPUT_BYTES)

    @unittest.skipUnless(os.name == "posix", "process-group regression")
    def test_inherited_output_pipe_is_bounded(self):
        script = "import subprocess,sys; subprocess.Popen([sys.executable,'-c','import time; time.sleep(10)'])"
        result = _run([sys.executable, "-c", script], self.root, timeout=0.1)
        self.assertEqual(result["error"], "output pipe timeout")
        self.assertLess(result["seconds"], 3)


if __name__ == "__main__":
    unittest.main()
