import json
import os
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch

from compiler_recovery import RecoverySession
from compiler_search import retrieve_search_context


class CompilerRecoveryTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        (self.root / "src/codegen").mkdir(parents=True)
        self.sources = {"src/ast.rs": "pub struct FunctionDecl {}\n", "src/parser.rs": "fn parse_function() {}\n",
                        "src/codegen/mod.rs": "fn emit_function() {}\n"}
        for path, text in self.sources.items():
            (self.root / path).write_text(text)
        self.binary = self.root / "pinned-rg"
        self.binary.write_text("#!/bin/sh\nexit 1\n")
        self.binary.chmod(0o755)

    def session(self, used=0, **kwargs):
        return RecoverySession(self.root, {"commands": [{}] * used, "source_read_calls": 0},
                               rg_binary=kwargs.pop("rg_binary", self.binary), **kwargs)

    def call(self, **overrides):
        return {"command": [str(self.binary)], "stdout": "".join(f"{p}:1:{t}" for p, t in self.sources.items()),
                "stderr": "", "exit_code": 0, "error": None, "output_clipped": False, **overrides}

    def test_typed_function_bypasses_prose_stopwords_and_uses_pinned_argv(self):
        session = self.session(1)
        request = session.prepare("For a function in ARCH, identify compiler owners.")
        self.assertEqual(request["status"], "ready")
        self.assertEqual(set(request["output_schema"]["properties"]), {"construct"})
        with patch("compiler_search._run", return_value=self.call()) as run:
            result = session.recover('{"construct":"function"}')
        self.assertEqual(result["status"], "ready", result["reason"])
        self.assertEqual(result["host_operations"], 5)
        self.assertEqual(result["remaining_operations"], 1)
        self.assertEqual(result["executable_read_calls"], 2)
        self.assertEqual(result["context"]["source_read_calls"], 3)
        self.assertEqual(run.call_args.args[0][0], str(self.binary))
        self.assertNotIn("shell", run.call_args.kwargs)
        self.assertTrue(all("role" not in r for r in result["context"]["evidence"]))

    def test_remaining_zero_through_six_never_resets_budget(self):
        for used in range(7):
            session = self.session(used)
            with self.subTest(used=used), patch("compiler_search._run", return_value=self.call()) as run:
                result = session.recover('{"construct":"function"}')
                self.assertEqual(result["status"], "ready" if used <= 2 else "needs_retrieval")
                self.assertLessEqual(result["host_operations"], 6)
                self.assertGreaterEqual(result["host_operations"], used)
                self.assertEqual(result["remaining_operations"], 6 - result["host_operations"])
                self.assertEqual(run.call_count, int(used <= 4))
                if used in (3, 4):
                    self.assertEqual(result["context"]["source_read_calls"], 0)

    def test_invalid_requests_consume_one_shot_without_retrieval(self):
        bad = ["{}", "null", "x" * 257, '{"construct":null}', '{"construct":true}',
               '{"construct":"a","construct":"b"}', '{"construct":"function","path":"src/ast.rs"}',
               '{"construct":"../secret"}', '{"construct":"enum;pwd"}', '{"construct":"two words"}',
               '{"construct":"UPPER"}', json.dumps({"construct": "x" * 65})]
        for answer in bad:
            session = self.session()
            with self.subTest(answer=answer), patch("compiler_search._run", side_effect=AssertionError("must not run")):
                result = session.recover(answer)
                self.assertEqual(result["status"], "needs_retrieval")
                self.assertEqual(result["host_operations"], 0)
                self.assertIsNone(result["context"])
                self.assertEqual(session.recover('{"construct":"function"}')["reason"], "recovery already attempted")
                self.assertEqual(session.prepare("function")["status"], "needs_retrieval")

    def test_failed_search_and_used_tools_never_enable_retry(self):
        for call in (self.call(exit_code=2), self.call(error="timeout"), self.call(output_clipped=True), self.call(stdout="", exit_code=1)):
            session = self.session(1)
            with patch("compiler_search._run", return_value=call) as run:
                first = session.recover('{"construct":"function"}')
                second = session.recover('{"construct":"function"}')
            self.assertEqual(first["status"], "needs_retrieval")
            self.assertEqual(first["host_operations"], 2)
            self.assertEqual(second["host_operations"], 2)
            self.assertEqual(run.call_count, 1)
        session = self.session()
        with patch("compiler_search._run", side_effect=AssertionError("tools already used")):
            self.assertEqual(session.recover('{"construct":"function"}', used_tools=True)["status"], "needs_retrieval")
            self.assertEqual(session.recover('{"construct":"function"}')["reason"], "recovery already attempted")

    def test_executable_replacement_and_symlink_substitution_fail_before_search(self):
        session = self.session()
        self.binary.write_text("#!/bin/sh\nexit 2\n")
        with patch("compiler_search._run", side_effect=AssertionError("changed binary")):
            self.assertEqual(session.recover('{"construct":"function"}')["reason"], "rg executable changed before retrieval")
        session = self.session()
        other = self.root / "other-rg"
        self.binary.rename(other)
        self.binary.symlink_to(other)
        with patch("compiler_search._run", side_effect=AssertionError("substituted binary")):
            self.assertEqual(session.recover('{"construct":"function"}')["status"], "needs_retrieval")

    def test_initial_counters_are_snapshotted_and_reject_malformed(self):
        context = {"commands": [{}], "source_read_calls": 2}
        session = RecoverySession(self.root, context, rg_binary=self.binary)
        context["commands"].clear()
        context["source_read_calls"] = 0
        self.assertEqual(session.used, 3)
        for context in (None, {"source_read_calls": True}, {"source_read_calls": -1},
                        {"commands": "bad"}, {"commands": [{}] * 7}):
            with self.subTest(context=context), self.assertRaises(ValueError):
                RecoverySession(self.root, context, rg_binary=self.binary)
        with self.assertRaises(ValueError):
            self.session(rg_binary=Path("rg"))

    @unittest.skipUnless(shutil.which("rg"), "ripgrep required")
    def test_real_host_search_is_unaffected_by_path_shadowing(self):
        binary = Path(shutil.which("rg")).resolve()
        session = self.session(rg_binary=binary)
        with patch.dict(os.environ, {"PATH": str(self.root / "missing")}):
            result = session.recover('{"construct":"function"}')
        self.assertEqual(result["status"], "ready", result["reason"])
        self.assertEqual(result["context"]["commands"][0]["command"][0], str(binary))

    @unittest.skipUnless(shutil.which("rg"), "ripgrep required")
    def test_consumed_compiler_construct_regressions(self):
        root = Path(__file__).resolve().parents[1]
        binary = Path(shutil.which("rg")).resolve()
        for construct in ("function", "struct", "enum", "synchronizer"):
            with self.subTest(construct=construct):
                session = RecoverySession(root, {"commands": [{}], "source_read_calls": 0}, rg_binary=binary)
                result = session.recover(json.dumps({"construct": construct}))
                self.assertEqual(result["status"], "ready", result["reason"])
                self.assertEqual(result["host_operations"], 5)
                self.assertEqual({row["symbol"] for row in result["context"]["evidence"]},
                                 {construct.title() + "Decl", "parse_" + construct, "emit_" + construct})

    def test_typed_search_validation_and_public_prompt_only(self):
        for construct in ("../x", True, "a b", "x" * 65):
            with self.assertRaises(ValueError):
                retrieve_search_context(self.root, "query", construct=construct)
        with self.assertRaises(ValueError):
            retrieve_search_context(self.root, "query", rg_binary=Path("rg"))
        session = self.session()
        request = session.prepare("PUBLIC QUESTION")
        self.assertNotIn(str(self.root), request["prompt"])
        self.assertNotIn(session.binary_hash, request["prompt"])
        for query in (None, "", "x" * 8193):
            with self.assertRaises(ValueError):
                session.prepare(query)


if __name__ == "__main__":
    unittest.main()
