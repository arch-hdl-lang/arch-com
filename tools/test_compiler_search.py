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


if __name__ == "__main__":
    unittest.main()
