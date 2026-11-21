import hashlib
import json
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch

from compiler_context import MAX_SOURCE_BYTES
from compiler_response import prepare_response, verify_response
from compiler_search import feature_names, retrieval_budget, retrieve_search_context


class CompilerSearchTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        (self.root / "src/codegen/nested").mkdir(parents=True)
        self.sources = {"src/ast.rs": "pub struct CamDecl {}\n",
                        "src/parser.rs": "    fn parse_cam() {}\n",
                        "src/codegen/nested/cam.rs": "    pub(crate) fn emit_cam() {}\n"}
        self.write_sources()
        self.question = ("For ARCH's content-addressable memory construct, identify the AST declaration, "
                         "parser implementation, and SystemVerilog backend emitter. "
                         "Return one source citation per role, with symbol and path:line for each.")

    def write_sources(self):
        for relative, content in self.sources.items():
            (self.root / relative).write_text(content)

    def output(self):
        return "".join(f"{path}:{n}:{line}\n" for path, text in self.sources.items()
                       for n, line in enumerate(text.splitlines(), 1) if "fn " in line or "struct " in line)

    def call(self, **overrides):
        return {"command": ["rg"], "stdout": self.output(), "stderr": "", "exit_code": 0,
                "error": None, "output_clipped": False, **overrides}

    def retrieve(self, **kwargs):
        return retrieve_search_context(self.root, self.question, **kwargs)

    def test_named_phrase_ignores_output_instruction_words(self):
        self.assertEqual(feature_names(self.question),
                         ["content_addressable_memory", "contentaddressablememory", "cam"])
        self.assertEqual(feature_names("Locate the linklist AST, parser and backend owners."), ["linklist"])
        self.assertEqual(feature_names("Locate the credit_channel AST, parser and backend owners."),
                         ["credit_channel", "creditchannel"])
        self.assertEqual(feature_names("For ARCH's io construct, find its owners.")[0], "io")
        self.assertEqual(feature_names("For ARCH's cam and linklist construct, find owners."), [])
        self.assertEqual(feature_names("For ARCH's cam construct and linklist, find owners."), [])
        self.assertEqual(feature_names("For ARCH's cam construct or linklist, find owners."), [])
        self.assertEqual(feature_names("For ARCH's cam construct and ARCH's fifo construct."), [])
        self.assertEqual(feature_names("Return source citation per role with symbol and path:line."), [])

    def test_three_source_roles_reserve_two_recovery_operations(self):
        with patch("compiler_search._run", return_value=self.call()) as run:
            result = self.retrieve()
        self.assertEqual(result["status"], "ready")
        self.assertEqual(result["source_read_calls"], 3)
        self.assertEqual(result["budget"], {"total_operations": 6, "reserve_operations": 2,
                                         "host_operations": 4, "remaining_operations": 2,
                                         "reserve_preserved": True})
        self.assertEqual(result["index_read_calls"], 0)
        self.assertEqual(result["host_graph_calls"], 0)
        self.assertTrue(all("role" not in row for row in result["evidence"]))
        command = run.call_args.args[0]
        self.assertIn("--no-config", command)
        self.assertEqual(command[-1], "src")
        self.assertNotIn("--max-count", command)
        self.assertNotIn("citation", command[-2])
        self.assertNotIn("symbol", command[-2])
        request = prepare_response(self.root, result, self.question)
        self.assertEqual(request["status"], "ready")
        answer = {role: row for role, row in zip(("ast", "parser", "backend"), result["evidence"])}
        self.assertEqual(verify_response(self.root, result, json.dumps(answer))["status"], "verified")

    @unittest.skipUnless(shutil.which("rg"), "ripgrep required")
    def test_real_recursive_search_ignores_generic_and_substring_distractors(self):
        (self.root / "src/distractors.rs").write_text("fn symbol_line() {}\nstruct ConstructFormalModel {}\n")
        with (self.root / "src/ast.rs").open("a") as stream:
            stream.write("pub struct MemoryDecl {}\npub struct CamDeclarationHelper {}\n")
        result = self.retrieve()
        self.assertEqual(result["status"], "ready", result["reason"])
        self.assertEqual([e["symbol"] for e in result["evidence"]], ["CamDecl", "parse_cam", "emit_cam"])
        self.assertNotIn("MemoryDecl", result["payload"])

    def test_no_cross_construct_role_stitching(self):
        output = self.output().replace("CamDecl", "ContentAddressableMemoryDecl")
        with patch("compiler_search._run", return_value=self.call(stdout=output)):
            result = self.retrieve()
        self.assertEqual(result["status"], "needs_search")
        self.assertEqual(result["source_read_calls"], 0)
        self.assertIn("consistent construct", result["reason"])

    def test_bus_scope_requires_scoped_names_without_banning_matching_owners(self):
        query = "Locate bus conditional generate AST and parser owners."
        self.sources = {"src/ast.rs": "pub struct BusGenerateIf {}\n",
                        "src/parser.rs": "fn parse_bus_generate_if() {}\n"}
        self.write_sources()
        with patch("compiler_search._run", return_value=self.call()):
            result = retrieve_search_context(self.root, query, construct="bus_generate_if", roles=("ast", "parser"))
        self.assertEqual(result["status"], "ready", result["reason"])
        self.assertEqual([row["symbol"] for row in result["evidence"]], ["BusGenerateIf", "parse_bus_generate_if"])
        with patch("compiler_search._run", side_effect=AssertionError("scope conflict")):
            rejected = retrieve_search_context(self.root, query, construct="generate", roles=("ast", "parser"))
        self.assertEqual(rejected["status"], "needs_search")
        self.assertEqual(rejected["commands"], [])

    def test_complete_acronym_collision_and_duplicate_role_fail_closed(self):
        full = self.output().replace("CamDecl", "ContentAddressableMemoryDecl").replace("parse_cam", "parse_content_addressable_memory").replace("emit_cam", "emit_content_addressable_memory")
        duplicate = "src/ast.rs:9:pub struct CamDecl {}\n"
        for extra in (full, duplicate):
            with self.subTest(extra=extra), patch("compiler_search._run", return_value=self.call(stdout=self.output() + extra)):
                result = self.retrieve()
                self.assertEqual(result["status"], "needs_search")
                self.assertEqual(result["source_read_calls"], 0)
                self.assertEqual(result["payload"], "")

    def test_failures_keep_counters_and_no_payload(self):
        calls = [self.call(exit_code=2), self.call(error="timeout"), self.call(output_clipped=True),
                 self.call(stdout="", exit_code=1), self.call(stdout="bad row"),
                 self.call(stdout=self.output().replace("fn parse_cam", "struct parse_cam")),
                 self.call(stdout=self.output().replace("src/parser.rs:1", "src/parser.rs:999")),
                 self.call(stdout=self.output().replace("src/ast.rs", "../secret.rs")),
                 self.call(stdout=self.output().replace("emit_cam", "emit_other"))]
        for call in calls:
            with self.subTest(call=call), patch("compiler_search._run", return_value=call) as run:
                result = self.retrieve()
                self.assertEqual(run.call_count, 1)
                self.assertEqual(result["status"], "needs_search")
                self.assertEqual(result["payload"], "")
                self.assertEqual(result["evidence"], [])
                self.assertEqual(result["source_hashes"], {})
                self.assertGreaterEqual(result["budget"]["host_operations"], 1)
                self.assertTrue(result["budget"]["reserve_preserved"])

    def test_failed_source_read_is_counted(self):
        with patch("compiler_search._run", return_value=self.call()), patch("pathlib.Path.open", side_effect=OSError("unreadable")):
            result = self.retrieve()
        self.assertEqual(result["status"], "needs_search")
        self.assertEqual(result["source_read_calls"], 1)
        self.assertEqual(result["budget"]["host_operations"], 2)

    def test_reserve_blocks_source_reads_before_any_file_is_opened(self):
        with patch("compiler_search._run", return_value=self.call()):
            result = self.retrieve(reserve_operations=3)
        self.assertEqual(result["status"], "needs_search")
        self.assertEqual(result["source_read_calls"], 0)
        self.assertEqual(result["budget"]["remaining_operations"], 5)
        with patch("compiler_search._run", side_effect=AssertionError("no allowance")):
            result = self.retrieve(total_operations=2, reserve_operations=1)
        self.assertEqual(result["budget"]["remaining_operations"], 2)

    def test_utf8_payload_and_source_bounds(self):
        for text in ("pub struct CamDecl {} // \u03bb\n", "x" * (MAX_SOURCE_BYTES + 1), "a\0b", "\udcff"):
            (self.root / "src/ast.rs").write_bytes(text.encode("utf-8", "surrogateescape"))
            with self.subTest(text=text[:30]), patch("compiler_search._run", return_value=self.call()):
                result = self.retrieve(byte_budget=1)
                self.assertEqual(result["status"], "needs_search")
                self.assertEqual(result["payload_bytes"], 0)
                self.assertLessEqual(result["source_read_bytes"], 3 * (MAX_SOURCE_BYTES + 1))

    def test_hash_pins_and_stale_after_delivery(self):
        with patch("compiler_search._run", return_value=self.call()):
            result = self.retrieve()
        self.assertEqual(result["source_hashes"]["src/ast.rs"], hashlib.sha256(self.sources["src/ast.rs"].encode()).hexdigest())
        (self.root / "src/ast.rs").write_text("pub struct OtherDecl {}\n")
        self.assertEqual(prepare_response(self.root, result, self.question)["status"], "needs_retrieval")

    def test_symlink_escape_and_source_root_link(self):
        outside = self.root / "outside.rs"
        outside.write_text(self.sources["src/ast.rs"])
        (self.root / "src/ast.rs").unlink()
        (self.root / "src/ast.rs").symlink_to(outside)
        with patch("compiler_search._run", return_value=self.call()):
            self.assertEqual(self.retrieve()["status"], "needs_search")
        (self.root / "src").rename(self.root / "real")
        (self.root / "src").symlink_to(self.root / "real", target_is_directory=True)
        with patch("compiler_search._run", side_effect=AssertionError("unsafe root")):
            self.assertEqual(self.retrieve()["status"], "needs_search")

    def test_lexical_boundary_does_not_claim_rust_syntax_or_semantics(self):
        for prefix, suffix in (("/*\n", "*/\n"), ('const TEXT: &str = r#"\n', '"#;\n')):
            self.sources = {path: prefix + text + suffix for path, text in self.sources.items()}
            self.write_sources()
            with patch("compiler_search._run", return_value=self.call()):
                result = self.retrieve()
            self.assertEqual(result["status"], "ready")
            self.assertEqual(result["evidence_kind"], "lexical_candidates")
            self.assertTrue(all("role" not in row for row in result["evidence"]))

    def test_budget_and_configuration_validation(self):
        for kwargs in ({"total_operations": True}, {"total_operations": -1}, {"total_operations": 33},
                       {"reserve_operations": 7}, {"roles": ()}, {"roles": ("ast", "ast")},
                       {"roles": ("unknown",)}, {"byte_budget": 0}, {"byte_budget": True}, {"byte_budget": 4097}):
            with self.subTest(kwargs=kwargs), self.assertRaises(ValueError):
                self.retrieve(**kwargs)
        for query in ("", "x" * 8193, None):
            with self.assertRaises(ValueError):
                feature_names(query)
        over = retrieval_budget({"commands": [None] * 7, "source_read_calls": 0})
        self.assertEqual(over["remaining_operations"], 0)
        self.assertFalse(over["reserve_preserved"])

    @unittest.skipUnless(shutil.which("rg"), "ripgrep required")
    def test_role_specific_suffixes_with_real_search(self):
        for parser, backend in (("parse_cam_decl", "emit_cam_decl"), ("parse_cam", "emit_cam_sva")):
            with self.subTest(parser=parser, backend=backend):
                self.sources["src/parser.rs"] = f"fn {parser}() {{}}\n"
                self.sources["src/codegen/nested/cam.rs"] = f"fn {backend}() {{}}\n"
                self.write_sources()
                result = self.retrieve()
                self.assertEqual(result["status"], "ready", result["reason"])
                self.assertEqual([r["symbol"] for r in result["evidence"]], ["CamDecl", parser, backend])
                self.assertEqual(result["budget"]["host_operations"], 4)

    @unittest.skipUnless(shutil.which("rg"), "ripgrep required")
    def test_suffix_helpers_and_wrong_roles_do_not_supply_coverage(self):
        for parser, backend in (("parse_cam_decl_helper", "emit_cam_sva"),
                                ("parse_cam_sva", "emit_cam"),
                                ("parse_cam", "emit_cam_sva_extra"),
                                ("emit_cam", "parse_cam_decl"),
                                ("parse_cam_decl_decl", "emit_cam")):
            with self.subTest(parser=parser, backend=backend):
                self.sources["src/parser.rs"] = f"fn {parser}() {{}}\n"
                self.sources["src/codegen/nested/cam.rs"] = f"fn {backend}() {{}}\n"
                self.write_sources()
                result = self.retrieve()
                self.assertEqual(result["status"], "needs_search")
                self.assertEqual(result["source_read_calls"], 0)
                self.assertEqual(result["payload"], "")

    @unittest.skipUnless(shutil.which("rg"), "ripgrep required")
    def test_multiple_suffix_owners_fail_in_both_orders(self):
        for rows in (("parse_cam", "parse_cam_decl"), ("parse_cam_decl", "parse_cam")):
            self.sources["src/parser.rs"] = "".join(f"fn {name}() {{}}\n" for name in rows)
            self.write_sources()
            result = self.retrieve()
            self.assertEqual(result["status"], "needs_search")
            self.assertIn("ambiguous", result["reason"])
            self.assertEqual(result["source_read_calls"], 0)

    @unittest.skipUnless(shutil.which("rg"), "ripgrep required")
    def test_public_variants_select_unique_routines_without_role_mixing(self):
        self.sources["src/parser.rs"] = "fn parse_cam_for() {}\nfn parse_cam_if() {}\n"
        self.sources["src/codegen/nested/cam.rs"] = "fn emit_cam_if() {}\nfn emit_cam_for() {}\n"
        self.write_sources()
        for query, variant in (("Trace conditional cam", "if"), ("Trace iterative cam", "for"),
                               ("Trace cam if", "if"), ("Trace cam for", "for")):
            with self.subTest(query=query):
                result = retrieve_search_context(self.root, query, construct="cam")
                self.assertEqual(result["status"], "ready", result["reason"])
                self.assertEqual([r["symbol"] for r in result["evidence"]],
                                 ["CamDecl", "parse_cam_" + variant, "emit_cam_" + variant])
        self.sources["src/codegen/nested/cam.rs"] = "fn emit_cam_for() {}\nfn emit_cam() {}\n"
        self.write_sources()
        result = retrieve_search_context(self.root, "Trace conditional cam", construct="cam")
        self.assertEqual(result["status"], "needs_search")
        self.assertEqual(result["source_read_calls"], 0)

    @unittest.skipUnless(shutil.which("rg"), "ripgrep required")
    def test_generic_conflicting_and_unrelated_variant_cues_fail_closed(self):
        self.sources["src/parser.rs"] = "fn parse_cam_if() {}\nfn parse_cam_for() {}\n"
        self.write_sources()
        for query in ("Trace cam", "Find owners for cam", "Trace cam if and for", "Trace cam if+for",
                      "Trace conditional and iterative cam", "Trace conditional cam and iterative cam",
                      "Trace cam. Return null if uncertain.", "Trace cam and conditional fifo",
                      "Find cam for each role", "Locate cam if possible", "Trace not conditional cam"):
            with self.subTest(query=query):
                result = retrieve_search_context(self.root, query, construct="cam")
                self.assertEqual(result["status"], "needs_search")
                self.assertEqual(result["source_read_calls"], 0)

    @unittest.skipUnless(shutil.which("rg"), "ripgrep required")
    def test_variant_uniqueness_and_read_budget_remain_strict(self):
        self.sources["src/parser.rs"] = "fn parse_cam_if() {}\nfn parse_cam_for() {}\n"
        self.write_sources()
        result = retrieve_search_context(self.root, "conditional cam", construct="cam", total_operations=3,
                                         reserve_operations=0)
        self.assertEqual(result["status"], "needs_search")
        self.assertEqual(result["source_read_calls"], 0)
        self.assertEqual(result["budget"]["host_operations"], 1)
        (self.root / "src/parser").mkdir()
        (self.root / "src/parser/duplicate.rs").write_text("fn parse_cam_if() {}\n")
        result = retrieve_search_context(self.root, "conditional cam", construct="cam")
        self.assertEqual(result["status"], "needs_search")
        self.assertIn("ambiguous", result["reason"])

    @unittest.skipUnless(shutil.which("rg"), "ripgrep required")
    def test_metadata_block_and_assertion_suffixes_are_exact_and_role_specific(self):
        self.sources = {"src/ast.rs": "pub struct CamMeta {}\n",
                        "src/parser.rs": "fn parse_cam_block() {}\n",
                        "src/codegen/nested/cam.rs": "fn emit_cam_asserts() {}\n"}
        self.write_sources()
        result = self.retrieve()
        self.assertEqual(result["status"], "ready", result["reason"])
        self.assertEqual([r["symbol"] for r in result["evidence"]],
                         ["CamMeta", "parse_cam_block", "emit_cam_asserts"])
        self.assertEqual(result["budget"]["host_operations"], 4)
        valid = self.sources.copy()
        for path, wrong in (("src/ast.rs", "struct CamMetadata {}\n"),
                            ("src/ast.rs", "struct CamMetaExtra {}\n"),
                            ("src/parser.rs", "fn parse_cam_block_extra() {}\n"),
                            ("src/parser.rs", "fn parse_cam_asserts() {}\n"),
                            ("src/codegen/nested/cam.rs", "fn emit_cam_block() {}\n"),
                            ("src/codegen/nested/cam.rs", "fn emit_cam_asserts_extra() {}\n"),
                            ("src/codegen/nested/cam.rs", "fn emit_other_asserts() {}\n")):
            with self.subTest(wrong=wrong):
                self.sources = {**valid, path: wrong}
                self.write_sources()
                failed = self.retrieve()
                self.assertEqual(failed["status"], "needs_search")
                self.assertEqual(failed["source_read_calls"], 0)

    @unittest.skipUnless(shutil.which("rg"), "ripgrep required")
    def test_credit_backend_scopes_require_explicit_unique_public_selection(self):
        self.sources = {"src/ast.rs": "pub struct CreditChannelMeta {}\n",
                        "src/parser.rs": "fn parse_credit_channel_block() {}\n",
                        "src/codegen/nested/cam.rs": "fn emit_credit_channel_state() {}\n"
                            "fn emit_credit_channel_receiver_state() {}\nfn emit_credit_channel_asserts() {}\n"}
        self.write_sources()
        positives = (("Locate credit channel sender-side state owners.", "emit_credit_channel_state"),
                     ("Locate credit channel receiver-side state owners.", "emit_credit_channel_receiver_state"),
                     ("Locate credit channel protocol assertions owners.", "emit_credit_channel_asserts"),
                     ("Locate credit channel sender-side state, rather than implying receiver FIFO synthesis.",
                      "emit_credit_channel_state"),
                     ("Locate credit channel receiver state, not sender state.", "emit_credit_channel_receiver_state"))
        for query, backend in positives:
            with self.subTest(query=query):
                result = retrieve_search_context(self.root, query, construct="credit_channel")
                self.assertEqual(result["status"], "ready", result["reason"])
                self.assertEqual(result["evidence"][-1]["symbol"], backend)
                self.assertEqual(result["budget"]["host_operations"], 4)
                limited = retrieve_search_context(self.root, query, construct="credit_channel",
                                                  total_operations=3, reserve_operations=0)
                self.assertEqual(limited["status"], "needs_search")
                self.assertEqual(limited["source_read_calls"], 0)
        for query in ("Locate credit channel owners.", "Locate credit channel sender and receiver state owners.",
                      "Locate credit channel sender FIFO owners.",
                      "Locate credit channel sender FIFO and protocol assertions owners.",
                      "Locate credit channel sender owners.",
                      "Locate credit channel receiver owners.",
                      "Locate credit channel receiver state and sender owners.",
                      "Locate credit channel sender state and protocol assertions owners.",
                      "Locate credit channel sender state, not receiver state but also protocol assertions.",
                      "Locate credit channel sender state rather than implying receiver FIFO synthesis and protocol assertions."):
            with self.subTest(query=query):
                result = retrieve_search_context(self.root, query, construct="credit_channel")
                self.assertEqual(result["status"], "needs_search")
                self.assertEqual(result["source_read_calls"], 0)
        self.sources["src/codegen/nested/cam.rs"] = "fn emit_credit_channel_state() {}\n"
        self.write_sources()
        for query in ("Locate credit channel receiver state owners.", "Locate credit channel owners."):
            result = retrieve_search_context(self.root, query, construct="credit_channel")
            self.assertEqual(result["status"], "needs_search")
            self.assertEqual(result["payload"], "")
        self.sources["src/codegen/nested/cam.rs"] += "fn emit_credit_channel_sender_state() {}\n"
        self.write_sources()
        result = retrieve_search_context(self.root, positives[0][0], construct="credit_channel")
        self.assertEqual(result["status"], "needs_search")
        self.assertIn("ambiguous", result["reason"])

    def test_public_nested_subject_cannot_be_dropped_or_swapped(self):
        for query, rejected in (("Locate transaction methods declared on a bus.", ("bus", "method", "assert", "handshake")),
                                ("Locate handshake channel owners on a bus.", ("bus", "channel", "credit_channel")),
                                ("Locate credit channel sender state owners.", ("bus", "credit", "sender_state")),
                                ("Locate clock-domain declaration owners.", ("clock", "module", "synchronizer")),
                                ("Locate handshake channels and transaction methods.", ("handshake", "tlm_method"))):
            for construct in rejected:
                with self.subTest(query=query, construct=construct), patch("compiler_search._run", side_effect=AssertionError("scope rejected")):
                    result = retrieve_search_context(self.root, query, construct=construct)
                self.assertEqual(result["status"], "needs_search")
                self.assertEqual(result["commands"], [])
                self.assertEqual(result["source_read_calls"], 0)

    def test_unsupported_backend_exclusions_fail_before_retrieval(self):
        for scope in ("without sender state", "except sender state", "other than receiver state",
                      "no sender state", "non-sender state", "not only sender state",
                      "rather than looking at receiver state", "sender state except assertions"):
            query = f"Locate credit channel {scope} owners."
            for construct in (None, "credit_channel"):
                with self.subTest(scope=scope, construct=construct), patch("compiler_search._run", side_effect=AssertionError("excluded scope")):
                    result = retrieve_search_context(self.root, query, construct=construct)
                self.assertEqual(result["status"], "needs_search")
                self.assertEqual(result["commands"], [])
                self.assertEqual(result["source_read_calls"], 0)
                if construct is not None:
                    self.assertIn("unsupported public backend exclusion", result["reason"])

    def test_additional_recognized_subjects_conflict_across_sentence_boundaries(self):
        subjects = (("handshake channel", "handshake"), ("transaction method", "tlm_method"),
                    ("clock domain", "domain"), ("credit channel sender state", "credit_channel"))
        for first, construct in subjects:
            for second, other in subjects:
                if construct == other:
                    continue
                for separator in (". Also locate ", "; then identify ", "? Locate ", "! Locate "):
                    query = f"Locate {first} owners{separator}{second} owners."
                    with self.subTest(query=query), patch("compiler_search._run", side_effect=AssertionError("multiple subjects")):
                        for chosen in (None, construct, other):
                            result = retrieve_search_context(self.root, query, construct=chosen)
                            self.assertEqual(result["status"], "needs_search")
                            self.assertIn("multiple public feature subjects", result["reason"])
                            self.assertEqual(result["commands"], [])
                            self.assertEqual(result["source_read_calls"], 0)

    def test_opening_subject_rejects_unknown_alternatives_and_output_only_subjects(self):
        for query in ("Locate handshake channels and fifo owners.",
                      "For ARCH's credit channel or ram construct, identify owners.",
                      "Trace clock domain and synchronizer declarations through their owners.",
                      "Locate transaction methods and methodical helpers.",
                      "Return source citation per role. Label its role clock domain."):
            with self.subTest(query=query), patch("compiler_search._run", side_effect=AssertionError("ambiguous subject")):
                result = retrieve_search_context(self.root, query)
            self.assertEqual(result["status"], "needs_search")
            self.assertEqual(result["source_read_calls"], 0)

    @unittest.skipUnless(shutil.which("rg"), "ripgrep required")
    def test_new_suffixes_do_not_mix_stems_or_hide_duplicate_owners(self):
        self.sources = {"src/ast.rs": "pub struct CamMeta {}\n",
                        "src/parser.rs": "fn parse_cam_block() {}\nfn parse_cam_decl() {}\n",
                        "src/codegen/nested/cam.rs": "fn emit_cam_asserts() {}\n"}
        self.write_sources()
        result = self.retrieve()
        self.assertEqual(result["status"], "needs_search")
        self.assertIn("ambiguous", result["reason"])
        self.sources["src/parser.rs"] = "fn parse_content_addressable_memory_block() {}\n"
        self.write_sources()
        result = self.retrieve()
        self.assertEqual(result["status"], "needs_search")
        self.assertIn("consistent construct", result["reason"])
        self.assertEqual(result["source_read_calls"], 0)

    @unittest.skipUnless(shutil.which("rg"), "ripgrep required")
    def test_public_normalization_is_not_a_global_alias_or_role_word_hint(self):
        self.sources = {"src/ast.rs": "pub struct DomainDecl {}\n",
                        "src/parser.rs": "fn parse_domain() {}\n",
                        "src/codegen/nested/cam.rs": "fn emit_domain() {}\n"}
        self.write_sources()
        for query in ("Locate AST, parser and backend owners.",
                      "Locate AST, parser and backend owners. Return clock_domain as a role label."):
            result = retrieve_search_context(self.root, query, construct="clock_domain")
            self.assertEqual(result["status"], "needs_search")
            self.assertEqual(result["source_read_calls"], 0)
        result = retrieve_search_context(self.root, "Locate clock-domain owners.", construct="clock_domain")
        self.assertEqual(result["status"], "ready", result["reason"])
        self.assertEqual(result["feature_names"], ["domain"])


if __name__ == "__main__":
    unittest.main()
