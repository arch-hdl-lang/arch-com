import copy
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from compiler_selection import COMPACT_POLICY, POLICY, CompactSelectionSession, prepare_selection, verify_selection


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

    def compact(self, context=None, query=None, roles=("ast", "parser", "backend")):
        context = self.context if context is None else context
        query = self.query if query is None else query
        session = CompactSelectionSession(self.root)
        request = session.prepare(context, query, roles)
        self.assertEqual(request["status"], "ready", request["reason"])
        public = json.loads(request["prompt"].rsplit("\n", 1)[1])
        answer = {"binding": public["binding"], **{
            r["role"]: next(k for k, v in public["evidence"].items() if v["path"] == r["path"])
            for r in self.rows if r["role"] in roles}}
        return session, request, answer

    def test_compact_host_rendering_counts_and_byte_reduction(self):
        session, request, answer = self.compact()
        legacy = self.verify()
        result = session.verify(self.context, json.dumps(answer), query=self.query)
        self.assertEqual(request["policy"], COMPACT_POLICY)
        self.assertEqual(result["policy"], COMPACT_POLICY)
        self.assertEqual(self.request["policy"], POLICY)
        self.assertEqual(POLICY, "compiler-evidence-selection-v1")
        self.assertEqual(result["status"], "verified", result["reason"])
        for field in ("answer", "citations", "validation_read_calls", "validation_read_bytes"):
            self.assertEqual(result[field], legacy[field])
        self.assertEqual(request["validation_read_calls"], 3)
        self.assertEqual(request["validation_read_bytes"], self.request["validation_read_bytes"])
        schema = request["output_schema"]
        self.assertEqual(schema["properties"]["binding"], {"type": "string", "const": answer["binding"]})
        self.assertEqual(schema["properties"]["ast"]["anyOf"][0]["enum"], ["0", "1", "2"])
        self.assertEqual(set(schema["required"]), {"binding", "ast", "parser", "backend"})
        self.assertFalse(schema["additionalProperties"])
        for key in self.catalog:
            self.assertNotIn(key, request["prompt"] + json.dumps(schema))
        # Byte comparisons only: no tokenizer or model-cost assertion.
        for field in ("prompt", "output_schema"):
            compact = request[field] if field == "prompt" else json.dumps(request[field], separators=(",", ":"))
            legacy_value = self.request[field] if field == "prompt" else json.dumps(self.request[field], separators=(",", ":"))
            self.assertLess(len(compact.encode()), len(legacy_value.encode()))
        self.assertLess(len(json.dumps(answer).encode()), len(json.dumps(self.answer).encode()))

    def test_compact_cross_request_binding_and_snapshot_domain(self):
        _, _, answer = self.compact()
        other, _, other_answer = self.compact()
        self.assertNotEqual(answer["binding"], other_answer["binding"])
        self.assertEqual({k: v for k, v in answer.items() if k != "binding"},
                         {k: v for k, v in other_answer.items() if k != "binding"})
        result = other.verify(self.context, json.dumps(answer), query=self.query)
        self.assertEqual(result["reason"], "wrong compact selection binding")
        self.assertEqual(result["validation_read_calls"], 0)
        self.assertEqual(other.verify(self.context, json.dumps(other_answer), query=self.query)["status"], "needs_retrieval")
        changed = copy.deepcopy(self.context)
        changed["evidence"][0]["symbol"] = "OtherDecl"
        # Even a repeated nonce cannot collapse different public snapshots.
        with patch("compiler_selection.secrets.token_bytes", return_value=b"fixed nonce"):
            bindings = [self.compact(context=c, query=q, roles=r)[2]["binding"] for c, q, r in (
                (self.context, self.query, ("ast", "parser", "backend")),
                (self.context, self.query + " Again.", ("ast", "parser", "backend")),
                (self.context, self.query, ("ast",)),
                (changed, self.query, ("ast", "parser", "backend")))]
        self.assertEqual(len(set(bindings)), 4)

    def test_compact_invalid_answers_and_tools_consume_attempt(self):
        mutations = [lambda a: "null", lambda a: "not JSON", lambda a: "x" * 4097,
                     lambda a: json.dumps({k: v for k, v in a.items() if k != "binding"}),
                     lambda a: json.dumps({**a, "binding": None}),
                     lambda a: json.dumps({**a, "binding": "other"}),
                     lambda a: json.dumps({**a, "ast": None}),
                     lambda a: json.dumps({**a, "ast": 0}),
                     lambda a: json.dumps({**a, "ast": True}),
                     lambda a: json.dumps({**a, "ast": "99"}),
                     lambda a: json.dumps({**a, "ast": a["parser"]}),
                     lambda a: json.dumps({**a, "path": "../secret"}),
                     lambda a: json.dumps(a)[:-1] + ', "binding": "duplicate"}',
                     lambda a: json.dumps(a)[:-1] + ', "ast": "0"}',
                     lambda a: None]
        for mutation in mutations:
            with self.subTest(mutation=mutation):
                session, _, answer = self.compact()
                result = session.verify(self.context, mutation(answer), query=self.query)
                self.assertEqual(result["status"], "needs_retrieval")
                self.assertEqual(result["validation_read_calls"], 0)
                self.assertNotIn("answer", result)
                self.assertNotIn("citations", result)
                self.assertEqual(session.verify(self.context, json.dumps(answer), query=self.query)["reason"],
                                 "compact selection already attempted")
        session, _, answer = self.compact()
        result = session.verify(self.context, json.dumps(answer), query=self.query, used_tools=True)
        self.assertEqual(result["status"], "needs_retrieval")
        self.assertEqual(result["validation_read_calls"], 0)
        self.assertEqual(session.verify(self.context, json.dumps(answer), query=self.query)["status"], "needs_retrieval")

    def test_compact_lifecycle_and_concurrent_consumption(self):
        session = CompactSelectionSession(self.root)
        self.assertEqual(session.verify(self.context, "{}", query=self.query)["reason"],
                         "compact selection requires a prepared request")
        self.assertEqual(session.prepare(self.context, self.query)["status"], "needs_retrieval")
        session, request, answer = self.compact()
        self.assertEqual(session.prepare(self.context, self.query)["status"], "needs_retrieval")
        # Caller-mutated request metadata must not mutate the host-held mapping.
        request["output_schema"]["properties"]["binding"]["const"] = "changed"
        request["output_schema"]["properties"]["ast"]["anyOf"][0]["enum"].clear()
        with ThreadPoolExecutor(max_workers=2) as pool:
            results = list(pool.map(lambda _: session.verify(self.context, json.dumps(answer), query=self.query), range(2)))
        self.assertEqual(sorted(r["status"] for r in results), ["needs_retrieval", "verified"])
        self.assertEqual(sum(r["validation_read_calls"] for r in results), 6)

    def test_compact_query_roles_metadata_and_source_mutations(self):
        for kwargs in ({"query": "Other question"}, {"query": self.query, "roles": ("ast",)},
                       {"query": self.query, "roles": ("backend", "parser", "ast")}):
            session, _, answer = self.compact()
            result = session.verify(self.context, json.dumps(answer), **kwargs)
            self.assertEqual(result["status"], "needs_retrieval")
            self.assertEqual(result["validation_read_calls"], 0)
        session, _, answer = self.compact()
        changed = copy.deepcopy(self.context)
        changed["evidence"][0]["symbol"] = "OtherDecl"
        result = session.verify(changed, json.dumps(answer), query=self.query)
        self.assertEqual(result["reason"], "unknown or stale evidence ID")
        self.assertEqual(result["validation_read_calls"], 3)
        sessions = [self.compact() for _ in range(2)]
        path = self.root / "src/ast.rs"
        path.write_bytes(path.read_bytes() + b"// outside the displayed row\n")
        session, _, answer = sessions[0]
        self.assertEqual(session.verify(self.context, json.dumps(answer), query=self.query)["status"], "needs_retrieval")
        self.context["source_hashes"]["src/ast.rs"] = hashlib.sha256(path.read_bytes()).hexdigest()
        session, _, answer = sessions[1]
        result = session.verify(self.context, json.dumps(answer), query=self.query)
        self.assertEqual(result["reason"], "unknown or stale evidence ID")
        self.assertNotIn("answer", result)

    def test_compact_graph_binding_and_search_literal_only_semantics(self):
        session, _, answer = self.compact()
        answer["ast"], answer["parser"] = answer["parser"], answer["ast"]
        result = session.verify(self.context, json.dumps(answer), query=self.query)
        self.assertEqual(result["status"], "needs_retrieval")
        self.assertEqual(result["validation_read_calls"], 6)
        self.assertNotIn("citations", result)
        for evidence in ([], [{k: v for k, v in row.items() if k != "role"} for row in self.rows]):
            context = {**self.context, "evidence": evidence}
            for swap in (False, True):
                session, _, answer = self.compact(context=context)
                if swap:
                    answer["ast"], answer["parser"] = answer["parser"], answer["ast"]
                result = session.verify(context, json.dumps(answer), query=self.query)
                self.assertEqual(result["status"], "verified")
                self.assertEqual(result["validation_read_calls"], 6)

    def test_compact_public_inputs_only_and_failed_preparation(self):
        context = {**self.context, "expected_citations": {"ast": "PRIVATE_EXPECTED_OWNER"}, "grade": "PRIVATE_GRADE"}
        _, request, _ = self.compact(context=context)
        self.assertNotIn("PRIVATE", json.dumps(request))
        for context, query in ((None, self.query), ({**self.context, "status": "needs_search"}, self.query),
                               ({**self.context, "payload": "../secret:1:pub struct Secret {}\n"}, self.query),
                               (self.context, ""), (self.context, None), (self.context, "x" * 8193)):
            session = CompactSelectionSession(self.root)
            result = session.prepare(context, query)
            self.assertEqual(result["status"], "needs_retrieval")
            self.assertNotIn("prompt", result)
            self.assertNotIn("output_schema", result)
            self.assertEqual(session.prepare(self.context, self.query)["status"], "needs_retrieval")


if __name__ == "__main__":
    unittest.main()
