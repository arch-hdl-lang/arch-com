"""Consumed development regressions using real providers, never a model.

Set ARCH_COMPILER_GRAPH_BIN to require the built-compiler integration checks.
Expected citations select synthetic answers only inside these offline tests.
They are not retrieval arguments or live model hints.
"""

import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from compiler_context import retrieve_compiler_context
from compiler_recovery import RecoverySession
from compiler_selection import prepare_selection, verify_selection


CASES = (
    ("module", (
        "Trace an ARCH module declaration through its AST, parser, and SystemVerilog generation. Identify the declaration node, parser routine, and top-level emitter, citing one source location for each role.",
        "Locate the ARCH module AST, parser and SystemVerilog backend owners."),
     ("ModuleDecl", "parse_module", "emit_module"), "src/codegen/module.rs"),
    ("param", (
        "Where are ARCH `param` and `local param` declarations represented and parsed, and which SystemVerilog codegen routine renders them as parameter declarations? Cite the AST, parser, and backend owner for each role.",
        "Locate the local parameter AST, parser and SystemVerilog emitter."),
     ("ParamDecl", "parse_param_decl", "emit_param_decl"), "src/codegen/mod.rs"),
    ("generate", (
        "Trace a conditional generate block in an ARCH module from its AST representation and parser routine to the SystemVerilog codegen routine that handles generate declarations. Cite one source location for each role.",
        "Locate the ARCH generate if AST, parser and SystemVerilog backend owners.",
        "Locate generate if AST and parser owners for each role, including the SystemVerilog backend."),
     ("GenerateDecl", "parse_generate_if", "emit_generate"), "src/codegen/mod.rs"),
    ("assert", (
        "Trace module-level assertion, cover, and assume declarations from their AST representation and parser to the SystemVerilog Assertions emission routine. Identify and cite the owner for each role.",
        "Locate the assertion and coverage AST, parser and SystemVerilog backend owners."),
     ("AssertDecl", "parse_assert_decl", "emit_assert_sva"), "src/codegen/mod.rs"),
)
ROLES = ("ast", "parser", "backend")


@unittest.skipUnless(os.environ.get("ARCH_COMPILER_GRAPH_BIN"), "set ARCH_COMPILER_GRAPH_BIN for real-provider integration")
class CompilerOwnershipIntegrationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.root = Path(__file__).resolve().parents[1]
        cls.binary = Path(os.environ["ARCH_COMPILER_GRAPH_BIN"]).resolve(strict=True)
        rg = shutil.which("rg")
        if rg is None:
            raise RuntimeError("ripgrep required for compiler ownership integration")
        cls.rg = Path(rg).resolve(strict=True)
        cls.temporary = tempfile.TemporaryDirectory(prefix="arch-ownership-integration-")
        cls.addClassCleanup(cls.temporary.cleanup)
        cls.index = Path(cls.temporary.name) / "index"
        subprocess.run([str(cls.binary), "graph", "dev-index", "--root", str(cls.root),
                        "--out", str(cls.index)], cwd=cls.root, check=True, capture_output=True, timeout=60)

    def check_selection(self, context, query, symbols, backend):
        self.assertEqual(context["status"], "ready", context.get("reason"))
        request = prepare_selection(self.root, context, query, ROLES)
        self.assertEqual(request["status"], "ready", request.get("reason"))
        catalog = json.loads(request["prompt"].rsplit("\n", 1)[1])["evidence"]
        expected = dict(zip(ROLES, zip(symbols, ("src/ast.rs", "src/parser.rs", backend))))
        answer = {}
        for role, (symbol, path) in expected.items():
            matches = [key for key, row in catalog.items() if row["symbol"] == symbol and row["path"] == path]
            self.assertEqual(len(matches), 1, (role, symbol, catalog))
            answer[role] = matches[0]
        result = verify_selection(self.root, context, json.dumps(answer), ROLES, query=query)
        self.assertEqual(result["status"], "verified", result.get("reason"))

    def test_four_consumed_cases_and_paraphrases_have_complete_graph_evidence(self):
        for name, queries, symbols, backend in CASES:
            for query in queries:
                with self.subTest(construct=name, query=query):
                    context = retrieve_compiler_context(self.root, self.binary, query, index=self.index)
                    self.check_selection(context, query, symbols, backend)
                    self.assertEqual(context["host_graph_calls"], 1)
                    self.assertEqual(context["source_read_calls"], 3)
                    self.assertEqual(len(context["commands"]) + context["source_read_calls"], 4)

    def test_public_typed_recovery_preserves_budget_and_exact_owners(self):
        for construct, queries, symbols, backend in CASES:
            for query in queries:
                with self.subTest(construct=construct, query=query):
                    # Model-free counter fixture: one failed graph call, no source reads.
                    initial = {"commands": [{}], "source_read_calls": 0}
                    session = RecoverySession(self.root, initial, rg_binary=self.rg)
                    self.assertEqual(session.prepare(query)["status"], "ready")
                    recovered = session.recover(json.dumps({"construct": construct}))
                    self.assertEqual(recovered["status"], "ready", recovered.get("reason"))
                    self.assertEqual(recovered["host_operations"], 5)
                    self.assertEqual(recovered["remaining_operations"], 1)
                    self.assertEqual(recovered["context"]["host_graph_calls"], 0)
                    self.assertEqual(recovered["context"]["index_read_calls"], 0)
                    self.check_selection(recovered["context"], query, symbols, backend)

    def test_consumed_tlm_question_and_paraphrases_keep_method_ownership(self):
        queries = (
            "For transaction methods declared on an ARCH bus, where are their signatures and modes represented and parsed, and which SystemVerilog codegen routine emits protocol assertions for those methods? Cite the AST, parser, and backend owners.",
            "Locate the transaction method AST, parser and backend for protocol assertions.",
            "Locate TLM method AST, parser and backend owners for protocol assertions.",
            "Locate TLM methods AST, parser and SystemVerilog assertion emission.",
            "Where is a transaction-level method represented, parsed and emitted as protocol assertions?",
            "Where are transaction-level methods represented, parsed and emitted as protocol assertions?",
            "Locate tlm_method AST, parser and backend owners.",
            "Locate transaction_methods AST, parser and backend owners.",
        )
        for query in queries:
            with self.subTest(query=query):
                context = retrieve_compiler_context(self.root, self.binary, query, index=self.index)
                self.check_selection(context, query,
                                     ("TlmMethodMeta", "parse_tlm_method_decl", "emit_tlm_method_asserts"),
                                     "src/codegen/mod.rs")
                self.assertEqual(context["host_graph_calls"], 1)
                self.assertEqual(len(context["commands"]) + context["source_read_calls"], 4)

    def test_transaction_connection_does_not_borrow_method_or_assertion_owners(self):
        query = "Locate transaction connection AST, parser and backend owners."
        context = retrieve_compiler_context(self.root, self.binary, query, index=self.index)
        self.assertEqual(context["status"], "needs_search")
        self.assertEqual(context["source_read_calls"], 0)
        self.assertEqual(context["payload"], "")
        self.assertTrue(context["commands"][0]["stdout"].startswith("feature TLM connect\n"))

    def test_method_near_misses_do_not_anchor_tlm(self):
        for phrase in ("transaction methodology", "TLM methodical", "transaction methodsize"):
            query = f"Locate {phrase} AST, parser and backend owners."
            with self.subTest(query=query):
                context = retrieve_compiler_context(self.root, self.binary, query, index=self.index)
                self.assertEqual(context["status"], "needs_search")
                self.assertEqual(context["source_read_calls"], 0)
                self.assertNotIn("feature tlm_method", context["commands"][0]["stdout"])

    def test_channel_assertions_do_not_borrow_tlm_or_module_backend(self):
        for phrase, feature in (("handshake channel", "bus handshake metadata"),
                                ("credit channel", "bus credit channel metadata")):
            query = f"Locate {phrase} AST, parser and backend owners for protocol assertions."
            with self.subTest(query=query):
                context = retrieve_compiler_context(self.root, self.binary, query, index=self.index)
                self.assertEqual(context["status"], "needs_search")
                self.assertEqual(context["source_read_calls"], 0)
                self.assertEqual(context["payload"], "")
                self.assertTrue(context["commands"][0]["stdout"].startswith(f"feature {feature}\n"))

    def test_late_missing_backend_does_not_starve_real_recovery(self):
        construct, queries, symbols, backend = CASES[0]
        query = queries[0]
        good = retrieve_compiler_context(self.root, self.binary, query, index=self.index)
        self.assertEqual(good["status"], "ready", good.get("reason"))
        call = dict(good["commands"][0])
        call["stdout"] = "\n".join(line for line in call["stdout"].splitlines() if not line.startswith("  backend "))
        with patch("compiler_context._run", return_value=call):
            incomplete = retrieve_compiler_context(self.root, self.binary, query, index=self.index)
        self.assertEqual(incomplete["status"], "needs_search")
        self.assertEqual(incomplete["source_read_calls"], 0)
        session = RecoverySession(self.root, incomplete, rg_binary=self.rg)
        session.prepare(query)
        recovered = session.recover(json.dumps({"construct": construct}))
        self.assertEqual(recovered["host_operations"], 5)
        self.assertEqual(recovered["status"], "ready", recovered.get("reason"))
        self.check_selection(recovered["context"], query, symbols, backend)

    def test_unspecified_generate_variant_is_not_arbitrarily_selected(self):
        for query in ("Locate generate AST, parser and backend owners.",
                      "Locate generate if and for AST parser backend owners.",
                      "Locate conditional and iterative generate AST parser backend owners.",
                      "Locate generate if or generate for AST parser backend owners.",
                      "Locate conditional generate and generate for AST parser backend owners."):
            with self.subTest(query=query):
                context = retrieve_compiler_context(self.root, self.binary, query, index=self.index)
                self.assertEqual(context["status"], "needs_search")
                self.assertEqual(context["source_read_calls"], 0)
                session = RecoverySession(self.root, context, rg_binary=self.rg)
                session.prepare(query)
                recovered = session.recover('{"construct":"generate"}')
                self.assertEqual(recovered["status"], "needs_retrieval")
                self.assertLessEqual(recovered["host_operations"], 6)

    def test_bus_graph_does_not_borrow_module_backend(self):
        query = "Locate bus conditional generate AST parser backend owners."
        context = retrieve_compiler_context(self.root, self.binary, query, index=self.index)
        self.assertEqual(context["status"], "needs_search")
        self.assertEqual(context["source_read_calls"], 0)
        self.assertEqual(context["payload"], "")
        session = RecoverySession(self.root, context, rg_binary=self.rg)
        session.prepare(query)
        recovered = session.recover('{"construct":"generate"}')
        self.assertEqual(recovered["status"], "needs_retrieval")
        self.assertIn("bus generation scope", recovered["reason"])
        self.assertEqual(recovered["host_operations"], 1)

    def test_explicit_iterative_graph_and_recovery(self):
        query = "Locate generate for AST parser backend owners."
        expected = ("GenerateDecl", "parse_generate_for", "emit_generate")
        context = retrieve_compiler_context(self.root, self.binary, query, index=self.index)
        self.check_selection(context, query, expected, "src/codegen/mod.rs")
        session = RecoverySession(self.root, {"commands": [], "source_read_calls": 0}, rg_binary=self.rg)
        session.prepare(query)
        recovered = session.recover('{"construct":"generate"}')
        self.assertEqual(recovered["status"], "ready", recovered.get("reason"))
        self.check_selection(recovered["context"], query, expected, "src/codegen/mod.rs")


if __name__ == "__main__":
    unittest.main()
