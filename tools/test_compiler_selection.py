import copy
import hashlib
import json
from pathlib import Path
import tempfile
import unittest

from compiler_selection import prepare_selection, verify_selection


class CompilerSelectionTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        (self.root / "src/codegen").mkdir(parents=True)
        self.rows = [
            {"role": "ast", "path": "src/ast.rs", "line": 1, "symbol": "EnumDecl", "quote": "pub struct EnumDecl {"},
            {"role": "parser", "path": "src/parser.rs", "line": 1, "symbol": "parse_enum", "quote": "fn parse_enum(&mut self) {"},
            {"role": "backend", "path": "src/codegen/mod.rs", "line": 1, "symbol": "Codegen::emit_enum", "quote": "pub(crate) fn emit_enum(&mut self) {"}]
        pins = {}
        for row in self.rows:
            data = (row["quote"] + "\n}\n").encode()
            (self.root / row["path"]).write_bytes(data)
            pins[row["path"]] = hashlib.sha256(data).hexdigest()
        self.context = {"status": "ready", "source_hashes": pins, "evidence": copy.deepcopy(self.rows),
                        "payload": "".join(f"{r['path']}:{r['line']}:{r['quote']}\n" for r in self.rows)}
        self.query = "Locate enum compiler owners."
        self.request = prepare_selection(self.root, self.context, self.query)
        self.catalog = json.loads(self.request["prompt"].rsplit("\n", 1)[1])["evidence"]
        self.answer = {r["role"]: next(k for k, v in self.catalog.items() if v["path"] == r["path"]) for r in self.rows}

    def verify(self, answer=None, **kwargs):
        return verify_selection(self.root, self.context, json.dumps(self.answer) if answer is None else answer,
                                query=kwargs.pop("query", self.query), **kwargs)

    def test_host_renders_exact_quotes_and_counts_all_validation(self):
        self.assertEqual(self.request["status"], "ready")
        self.assertEqual(self.request["validation_read_calls"], 3)
        result = self.verify()
        self.assertEqual(result["status"], "verified", result["reason"])
        self.assertEqual(result["validation_read_calls"], 6)
        self.assertEqual(result["citations"]["parser"]["quote"], "fn parse_enum(&mut self) {")
        self.assertEqual(result["citations"]["backend"]["symbol"], "emit_enum")
        self.assertEqual(json.loads(result["answer"]), result["citations"])
        self.assertEqual(self.request["output_schema"]["properties"]["ast"]["anyOf"][0]["enum"], list(self.catalog))

    def test_malformed_null_duplicate_and_model_overrides_are_rejected(self):
        bad = ["{}", "null", "not JSON", "x" * 4097,
               json.dumps({**self.answer, "ast": None}), json.dumps({**self.answer, "ast": True}),
               json.dumps({**self.answer, "ast": 1}), json.dumps({**self.answer, "ast": "unknown"}),
               json.dumps({**self.answer, "ast": self.answer["parser"]}),
               json.dumps({**self.answer, "quote": "fabricated"}),
               json.dumps({**self.answer, "ast": {"id": self.answer["ast"], "path": "../secret"}}),
               json.dumps(self.answer)[:-1] + ', "ast": "duplicate"}']
        for answer in bad:
            with self.subTest(answer=answer):
                result = self.verify(answer)
                self.assertEqual(result["status"], "needs_retrieval")
                self.assertNotIn("answer", result)
                self.assertNotIn("citations", result)

    def test_graph_role_swap_and_tool_use_are_rejected(self):
        swapped = {**self.answer, "ast": self.answer["parser"], "parser": self.answer["ast"]}
        self.assertEqual(self.verify(json.dumps(swapped))["status"], "needs_retrieval")
        result = self.verify(used_tools=True)
        self.assertEqual(result["status"], "needs_retrieval")
        self.assertEqual(result["validation_read_calls"], 0)

    def test_context_and_query_identity(self):
        self.assertEqual(self.verify(query="Locate other owners.")["status"], "needs_retrieval")
        other = copy.deepcopy(self.context)
        other["evidence"][0]["symbol"] = "OtherDecl"
        self.assertEqual(verify_selection(self.root, other, json.dumps(self.answer), query=self.query)["status"], "needs_retrieval")
        data = (self.root / "src/ast.rs").read_bytes() + b"// unrelated edit\n"
        (self.root / "src/ast.rs").write_bytes(data)
        self.assertEqual(self.verify()["status"], "needs_retrieval")
        self.context["source_hashes"]["src/ast.rs"] = hashlib.sha256(data).hexdigest()
        self.assertEqual(self.verify()["reason"], "unknown or stale evidence ID")

    def test_reordered_payload_is_same_snapshot(self):
        self.context["payload"] = "\n".join(reversed(self.context["payload"].splitlines())) + "\n"
        self.assertEqual(self.verify()["status"], "verified")

    def test_unbound_search_retains_literal_only_semantics(self):
        self.context["evidence"] = []
        prepared = prepare_selection(self.root, self.context, self.query)
        catalog = json.loads(prepared["prompt"].rsplit("\n", 1)[1])["evidence"]
        answer = {r["role"]: next(k for k, v in catalog.items() if v["path"] == r["path"]) for r in self.rows}
        self.assertEqual(self.verify(json.dumps(answer))["status"], "verified")

    def test_invalid_source_bounds_and_failed_freshness_return_no_request(self):
        for update in ({"status": "needs_search"}, {"payload": ""}, {"payload": "x" * 4097},
                       {"payload": "../secret:1:pub struct Secret {}\n"}, {"source_hashes": {"../secret": "x"}}):
            context = {**self.context, **update}
            result = prepare_selection(self.root, context, self.query)
            self.assertEqual(result["status"], "needs_retrieval")
            self.assertNotIn("prompt", result)
        for query in ("", None, "x" * 8193):
            with self.assertRaises(ValueError):
                prepare_selection(self.root, self.context, query)

    def test_private_oracle_fields_are_not_model_inputs(self):
        self.context["expected_citations"] = {"ast": "PRIVATE"}
        result = prepare_selection(self.root, self.context, self.query)
        self.assertEqual(result, self.request)
        self.assertNotIn("PRIVATE", result["prompt"])


if __name__ == "__main__":
    unittest.main()
