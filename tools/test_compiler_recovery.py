import json
import os
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch

from compiler_recovery import RecoverySession
from compiler_search import retrieve_search_context
from compiler_selection import prepare_selection, verify_selection


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
        query = kwargs.pop("query", "For a function in ARCH, identify compiler owners.")
        session = RecoverySession(self.root, {"commands": [{}] * used, "source_read_calls": 0},
                                  rg_binary=kwargs.pop("rg_binary", self.binary), **kwargs)
        session.prepare(query)
        return session

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
                session.prepare(f"Locate ARCH {construct} compiler owners.")
                result = session.recover(json.dumps({"construct": construct}))
                self.assertEqual(result["status"], "ready", result["reason"])
                self.assertEqual(result["host_operations"], 5)
                self.assertEqual({row["symbol"] for row in result["context"]["evidence"]},
                                 {construct.title() + "Decl", "parse_" + construct, "emit_" + construct})

    @unittest.skipUnless(shutil.which("rg"), "ripgrep required")
    def test_recovered_context_flows_through_evidence_selection(self):
        # End-to-end wiring for the documented recover -> prepare_selection ->
        # verify_selection flow (doc/compiler_context_preflight.md "Evidence
        # Selection And Typed Recovery"). The selection unit tests fabricate the
        # context dict by hand, so nothing exercises a context actually produced
        # by the real search/recovery provider against the selection helper.
        root = Path(__file__).resolve().parents[1]
        binary = Path(shutil.which("rg")).resolve()
        roles = ("ast", "parser", "backend")
        query = "Identify the compiler owners of the function construct."
        session = RecoverySession(root, {"commands": [{}], "source_read_calls": 0}, rg_binary=binary)
        session.prepare(query)
        context = session.recover(json.dumps({"construct": "function"}))["context"]
        self.assertEqual(context["status"], "ready", context.get("reason"))
        self.assertTrue(all("role" not in row for row in context["evidence"]))

        prepared = prepare_selection(root, context, query, roles)
        self.assertEqual(prepared["status"], "ready", prepared.get("reason"))
        catalog = json.loads(prepared["prompt"].rsplit("\n", 1)[1])["evidence"]

        def role_of(candidate):
            if candidate["path"].startswith("src/ast"):
                return "ast"
            if candidate["symbol"].startswith("parse_"):
                return "parser"
            if candidate["symbol"].startswith("emit_"):
                return "backend"
            return None

        by_role = {role_of(value): key for key, value in catalog.items()}
        self.assertEqual(set(by_role), set(roles))
        correct = {role: by_role[role] for role in roles}

        verified = verify_selection(root, context, json.dumps(correct), roles, query=query)
        self.assertEqual(verified["status"], "verified", verified.get("reason"))
        self.assertEqual({c["symbol"] for c in verified["citations"].values()},
                         {"FunctionDecl", "parse_function", "emit_function"})
        for role, citation in verified["citations"].items():
            self.assertEqual(role_of(citation), role)

        # Search/recovery evidence carries no role metadata, so verify_response
        # validates only literal declarations: a role swap renders the swapped
        # citations instead of being rejected. Graph contexts, which do carry
        # role bindings, reject swaps -- see test_compiler_selection.py
        # ::test_graph_role_swap_and_tool_use_are_rejected.
        swapped = {**correct, "ast": correct["backend"], "backend": correct["ast"]}
        swapped_result = verify_selection(root, context, json.dumps(swapped), roles, query=query)
        self.assertEqual(swapped_result["status"], "verified", swapped_result.get("reason"))
        self.assertEqual(role_of(swapped_result["citations"]["ast"]), "backend")

    def test_typed_search_validation_and_public_prompt_only(self):
        for construct in ("../x", True, "a b", "x" * 65):
            with self.assertRaises(ValueError):
                retrieve_search_context(self.root, "query", construct=construct)
        with self.assertRaises(ValueError):
            retrieve_search_context(self.root, "query", rg_binary=Path("rg"))
        session = self.session(query="PUBLIC QUESTION")
        request = session.prepare("PUBLIC QUESTION")
        self.assertNotIn(str(self.root), request["prompt"])
        self.assertNotIn(session.binary_hash, request["prompt"])
        for query in (None, "", "x" * 8193):
            with self.assertRaises(ValueError):
                session.prepare(query)

    def test_prepared_public_query_is_bound_and_forwarded_without_private_context(self):
        initial = {"commands": [], "source_read_calls": 0, "private_answer": "DO NOT FORWARD"}
        session = RecoverySession(self.root, initial, rg_binary=self.binary)
        query = "Trace a conditional function through its owners."
        session.prepare(query)
        session.prepare(query)
        with self.assertRaisesRegex(ValueError, "already bound"):
            session.prepare("Trace iterative function")
        with patch("compiler_recovery.retrieve_search_context", return_value={
                "commands": [], "source_read_calls": 0, "status": "ready"}) as retrieve:
            result = session.recover('{"construct":"function"}')
        self.assertEqual(result["status"], "ready")
        self.assertEqual(retrieve.call_args.args, (self.root, query))
        self.assertNotIn("private_answer", repr(retrieve.call_args))
        self.assertNotIn("DO NOT FORWARD", repr(retrieve.call_args))
        self.assertEqual(retrieve.call_args.kwargs["reserve_operations"], 0)
        self.assertEqual(retrieve.call_args.kwargs["total_operations"], 6)
        self.assertEqual(session.recover('{"construct":"function"}')["reason"], "recovery already attempted")

    def test_unprepared_recovery_never_searches_and_consumes_attempt(self):
        session = RecoverySession(self.root, {"commands": [{}], "source_read_calls": 0}, rg_binary=self.binary)
        with patch("compiler_recovery.retrieve_search_context", side_effect=AssertionError("unprepared search")):
            result = session.recover('{"construct":"function"}')
        self.assertEqual(result["reason"], "recovery requires a prepared public query")
        self.assertEqual(result["host_operations"], 1)
        self.assertEqual(result["executable_read_calls"], 1)
        self.assertIsNone(result["context"])
        self.assertEqual(session.prepare("function")["status"], "needs_retrieval")

    def test_typed_keyword_cannot_erase_explicit_bus_generation_scope(self):
        for query in ("Locate bus conditional generate AST parser backend owners.",
                      "Locate bus generate if owners.", "Locate generate_if bus owners.",
                      "Trace a conditional generate block in an ARCH bus."):
            for construct in ("generate", "generate_if", "module", "bus"):
                with self.subTest(query=query, construct=construct):
                    session = self.session(1, query=query)
                    with patch("compiler_search._run", side_effect=AssertionError("scope conflict")):
                        result = session.recover(json.dumps({"construct": construct}))
                    self.assertEqual(result["status"], "needs_retrieval")
                    self.assertIn("bus generation scope", result["reason"])
                    self.assertEqual(result["host_operations"], 1)
                    self.assertEqual(result["context"]["source_read_calls"], 0)

    @unittest.skipUnless(shutil.which("rg"), "ripgrep required")
    def test_consumed_ownership_failures_recover_from_empty_context(self):
        root = Path(__file__).resolve().parents[1]
        binary = Path(shutil.which("rg")).resolve()
        cases = (
            ("module", "Trace an ARCH module declaration through its AST, parser, and SystemVerilog generation.",
             ("ModuleDecl", "parse_module", "emit_module")),
            ("param", "Where are ARCH param and local param declarations represented and parsed?",
             ("ParamDecl", "parse_param_decl", "emit_param_decl")),
            ("generate", "Trace a conditional generate block in an ARCH module from its AST representation and parser routine to SystemVerilog.",
             ("GenerateDecl", "parse_generate_if", "emit_generate")),
            ("assert", "Trace module-level assertion, cover, and assume declarations to SystemVerilog Assertions emission.",
             ("AssertDecl", "parse_assert_decl", "emit_assert_sva")))
        for construct, query, owners in cases:
            with self.subTest(construct=construct):
                session = RecoverySession(root, {"commands": [], "source_read_calls": 0}, rg_binary=binary)
                session.prepare(query)
                result = session.recover(json.dumps({"construct": construct}))
                self.assertEqual(result["status"], "ready", result["reason"])
                self.assertEqual(tuple(r["symbol"] for r in result["context"]["evidence"]), owners)
                self.assertEqual(result["host_operations"], 4)
                self.assertEqual(result["remaining_operations"], 2)
                self.assertEqual(result["context"]["commands"][0]["command"][0], str(binary))
                self.assertEqual(result["context"]["host_graph_calls"], 0)
                self.assertEqual(result["context"]["index_read_calls"], 0)

    @unittest.skipUnless(shutil.which("rg"), "ripgrep required")
    def test_real_generate_variants_require_unambiguous_public_query(self):
        root = Path(__file__).resolve().parents[1]
        binary = Path(shutil.which("rg")).resolve()
        for query, expected in (("Trace iterative generate", "parse_generate_for"),
                                ("Trace generate", None), ("Trace generate if and for", None)):
            with self.subTest(query=query):
                session = RecoverySession(root, {"commands": [], "source_read_calls": 0}, rg_binary=binary)
                session.prepare(query)
                result = session.recover('{"construct":"generate"}')
                self.assertEqual(result["status"], "ready" if expected else "needs_retrieval")
                if expected:
                    self.assertEqual(result["context"]["evidence"][1]["symbol"], expected)
                self.assertLessEqual(result["host_operations"], 6)

    def test_prompt_requests_specific_public_compound_subject_without_private_hints(self):
        query = "Locate transaction methods declared on an ARCH bus."
        session = self.session(query=query)
        request = session.prepare(query)
        self.assertTrue(request["prompt"].startswith(query + "\n"))
        self.assertIn("most specific feature subject", request["prompt"])
        self.assertIn("compound and nested feature names", request["prompt"])
        self.assertNotIn("single ARCH language construct keyword", request["prompt"])
        for private in ("TlmMethodMeta", "parse_tlm_method_decl", "emit_tlm_method_asserts", str(self.root)):
            self.assertNotIn(private, request["prompt"])
        self.assertEqual(set(request["output_schema"]["properties"]), {"construct"})

    def test_parent_keywords_and_wrong_nested_subject_consume_attempt_without_search(self):
        for query, construct in (("Locate transaction methods declared on an ARCH bus.", "bus"),
                                 ("Locate a handshake channel on an ARCH bus.", "bus"),
                                 ("Locate credit channel sender-side state owners.", "bus"),
                                 ("Locate clock-domain declaration owners.", "clock"),
                                 ("Locate handshake channel owners.", "tlm_method"),
                                 ("Locate credit channel sender and receiver state owners.", "credit_channel"),
                                 ("Locate credit channel without sender state owners.", "credit_channel"),
                                 ("Locate credit channel except sender state owners.", "credit_channel"),
                                 ("Locate handshake channel owners. Also locate transaction method owners.", "handshake")):
            with self.subTest(query=query, construct=construct):
                session = self.session(1, query=query)
                with patch("compiler_search._run", side_effect=AssertionError("public subject conflict")):
                    result = session.recover(json.dumps({"construct": construct}))
                self.assertEqual(result["status"], "needs_retrieval")
                self.assertEqual(result["host_operations"], 1)
                self.assertEqual(result["remaining_operations"], 5)
                self.assertEqual(session.recover(json.dumps({"construct": construct}))["reason"],
                                 "recovery already attempted")

    @unittest.skipUnless(shutil.which("rg"), "ripgrep required")
    def test_consumed_nested_subjects_recover_with_exact_owners_and_original_budget(self):
        root = Path(__file__).resolve().parents[1]
        binary = Path(shutil.which("rg")).resolve()
        cases = (
            (("domain", "clock_domain"), (
                "Trace an ARCH clock-domain declaration through its AST representation and parser, then identify the SystemVerilog codegen routine that carries its domain metadata into output. Cite one source location for each role.",
                "Locate clock domain AST, parser and backend owners."),
             ("DomainDecl", "parse_domain", "emit_domain")),
            (("tlm_method", "transaction_methods"), (
                "For transaction methods declared on an ARCH bus, where are their signatures and modes represented and parsed, and which SystemVerilog codegen routine emits protocol assertions for those methods? Cite the AST, parser, and backend owners.",
                "Locate the TLM method AST, parser and backend owners for protocol assertions."),
             ("TlmMethodMeta", "parse_tlm_method_decl", "emit_tlm_method_asserts")),
            (("handshake", "handshake_channel"), (
                "For a handshake channel declared on an ARCH bus, where is its parsed metadata represented, which parser routine reads it, and which SystemVerilog routine emits the channel protocol assertions? Cite the AST, parser, and backend owners.",
                "Locate handshake AST, parser and protocol assertion backend owners."),
             ("HandshakeMeta", "parse_handshake_block", "emit_handshake_asserts")),
            (("credit_channel",), (
                "For an ARCH credit channel, trace its metadata representation and parser, then identify the SystemVerilog routine that emits sender-side credit state. Keep the backend scope to sender-side state rather than implying receiver FIFO synthesis. Cite one source location for each role.",
                "Locate credit channel sender state AST, parser and backend owners, not receiver state."),
             ("CreditChannelMeta", "parse_credit_channel_block", "emit_credit_channel_state")))
        roles = ("ast", "parser", "backend")
        for constructs, queries, expected in cases:
            for query in queries:
                direct = retrieve_search_context(root, query, rg_binary=binary)
                self.assertEqual(direct["status"], "ready", direct["reason"])
                self.assertEqual(tuple(row["symbol"] for row in direct["evidence"]), expected)
                self.assertEqual(direct["budget"]["host_operations"], 4)
                for construct in constructs:
                    with self.subTest(query=query, construct=construct):
                        session = RecoverySession(root, {"commands": [{}], "source_read_calls": 0}, rg_binary=binary)
                        session.prepare(query)
                        result = session.recover(json.dumps({"construct": construct}))
                        self.assertEqual(result["status"], "ready", result["reason"])
                        self.assertEqual(result["host_operations"], 5)
                        self.assertEqual(result["remaining_operations"], 1)
                        context = result["context"]
                        self.assertEqual(tuple(row["symbol"] for row in context["evidence"]), expected)
                        self.assertEqual(context["host_graph_calls"], 0)
                        self.assertEqual(context["index_read_calls"], 0)
                        self.assertTrue(all("role" not in row for row in context["evidence"]))
                        self.assertEqual(context["commands"][0]["command"][0], str(binary))
                        self.assertEqual(context["source_read_calls"], 3)
                        # Expected owners affect only this offline answer, never the provider arguments.
                        prepared = prepare_selection(root, context, query, roles)
                        catalog = json.loads(prepared["prompt"].rsplit("\n", 1)[1])["evidence"]
                        answer = {role: next(k for k, v in catalog.items() if v["symbol"] == symbol)
                                  for role, symbol in zip(roles, expected)}
                        self.assertEqual(verify_selection(root, context, json.dumps(answer), roles,
                                                          query=query)["status"], "verified")

    @unittest.skipUnless(shutil.which("rg"), "ripgrep required")
    def test_plural_nested_identifiers_require_the_corresponding_public_subject(self):
        root = Path(__file__).resolve().parents[1]
        binary = Path(shutil.which("rg")).resolve()
        for query, construct, backend in (
                ("Locate handshake channels and their protocol assertions owners.", "handshake_channels", "emit_handshake_asserts"),
                ("Locate credit channels sender state owners.", "credit_channels", "emit_credit_channel_state"),
                ("Locate clock domains owners.", "clock_domains", "emit_domain")):
            session = RecoverySession(root, {"commands": [], "source_read_calls": 0}, rg_binary=binary)
            session.prepare(query)
            result = session.recover(json.dumps({"construct": construct}))
            self.assertEqual(result["status"], "ready", result["reason"])
            self.assertEqual(result["context"]["evidence"][-1]["symbol"], backend)

    @unittest.skipUnless(shutil.which("rg"), "ripgrep required")
    def test_real_credit_channel_facets_never_default_to_sender(self):
        root = Path(__file__).resolve().parents[1]
        binary = Path(shutil.which("rg")).resolve()
        for scope, expected in (("receiver state", "emit_credit_channel_receiver_state"),
                                ("protocol assertions", "emit_credit_channel_asserts"),
                                ("", None), ("sender and receiver state", None),
                                ("sender state and protocol assertions", None)):
            with self.subTest(scope=scope):
                session = RecoverySession(root, {"commands": [{}], "source_read_calls": 0}, rg_binary=binary)
                session.prepare(f"Locate credit channel {scope} AST, parser and backend owners.")
                result = session.recover('{"construct":"credit_channel"}')
                self.assertEqual(result["status"], "ready" if expected else "needs_retrieval", result["reason"])
                if expected:
                    self.assertEqual(result["context"]["evidence"][-1]["symbol"], expected)
                else:
                    self.assertEqual(result["context"]["source_read_calls"], 0)


if __name__ == "__main__":
    unittest.main()
