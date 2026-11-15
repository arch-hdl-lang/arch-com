"""Graph MCP boundary and refresh tests (run in the MCP virtual environment)."""

import asyncio
import json
import pathlib
import tempfile
import unittest
from unittest.mock import patch

import arch_mcp_server as server


class GraphToolsTests(unittest.TestCase):
    def test_public_tool_schema_matches_source_defaults(self):
        schemas = {tool.name: tool.inputSchema for tool in asyncio.run(server.mcp.list_tools())}
        for name in ("arch_graph_context", "arch_dev_graph_query"):
            properties = schemas[name]["properties"]
            self.assertIs(properties["include_source"]["default"], True)
            self.assertEqual(properties["token_budget"]["default"], 200)

    def test_rejects_indexed_source_outside_workspace(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp).resolve()
            index = root / "index"
            index.mkdir()
            (index / "files.jsonl").write_text(json.dumps({"abs_path": "/etc/passwd"}) + "\n")
            with patch.object(server, "WORKSPACE_ROOTS", [root]):
                with self.assertRaisesRegex(ValueError, "outside allowed roots"):
                    server.arch_graph_query("module", str(index))

    def test_context_rebuilds_stale_index_from_explicit_sources(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp).resolve()
            source = root / "Demo.arch"
            source.write_text("module Demo end module Demo\n")
            with patch.object(server, "WORKSPACE_ROOTS", [root]), patch.object(
                server, "_run", side_effect=["[ERROR] source changed since indexing", "[OK] indexed", "[OK] feature Demo"]
            ) as run:
                result = server.arch_graph_context("Demo", str(root / "index"),
                                                   source_paths=[str(source)])
            self.assertIn("feature Demo", result)
            self.assertEqual(run.call_count, 3)
            self.assertIn("--clean", run.call_args_list[1].args[0])

    def test_developer_query_rebuilds_stale_index(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp).resolve()
            feature_map = root / "doc/codegraph_feature_map.json"
            feature_map.parent.mkdir()
            feature_map.write_text("{}")
            with patch.object(server, "WORKSPACE_ROOTS", [root]), patch.object(
                server, "_run", side_effect=["[ERROR] map changed since indexing", "[OK] indexed", "[OK] feature pipe_reg"]
            ) as run:
                result = server.arch_dev_graph_query("pipe_reg", str(root))
            self.assertIn("feature pipe_reg", result)
            self.assertEqual(run.call_count, 3)
            self.assertIn("dev-index", run.call_args_list[1].args[0])

    def test_source_excerpt_flag_reaches_both_graph_queries(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp).resolve()
            with patch.object(server, "WORKSPACE_ROOTS", [root]), patch.object(
                server, "_run", return_value="[OK] source"
            ) as run:
                server.arch_graph_context("Demo", index=str(root / "user"), include_source=True)
                self.assertIn("--include-source", run.call_args.args[0])
                server.arch_dev_graph_query("Demo", root=str(root), include_source=True)
                self.assertIn("--include-source", run.call_args.args[0])

    def test_default_source_evidence_and_explicit_location_only(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp).resolve()
            with patch.object(server, "WORKSPACE_ROOTS", [root]), patch.object(
                server, "_run", return_value="[OK] source"
            ) as run:
                for query, kwargs in (
                    (server.arch_graph_context, {"index": str(root / "user")}),
                    (server.arch_dev_graph_query, {"root": str(root)}),
                ):
                    query("feature", **kwargs)
                    command = run.call_args.args[0]
                    self.assertIn("--include-source", command)
                    self.assertEqual(command[command.index("--token-budget") + 1], "200")
                    query("feature", include_source=False, token_budget=600, **kwargs)
                    command = run.call_args.args[0]
                    self.assertNotIn("--include-source", command)
                    self.assertEqual(command[command.index("--token-budget") + 1], "600")

    def test_refresh_preserves_source_policy_and_budget(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp).resolve()
            source = root / "Demo.arch"
            source.write_text("module Demo end module Demo\n")
            feature_map = root / "doc/codegraph_feature_map.json"
            feature_map.parent.mkdir()
            feature_map.write_text("{}")
            for query, kwargs in (
                (server.arch_graph_context, {"index": str(root / "user"), "source_paths": [str(source)]}),
                (server.arch_dev_graph_query, {"root": str(root)}),
            ):
                for include_source in (True, False):
                    with patch.object(server, "WORKSPACE_ROOTS", [root]), patch.object(
                        server, "_run", side_effect=["[ERROR] source changed since indexing", "[OK] indexed", "[OK] result"]
                    ) as run:
                        query("feature", include_source=include_source, token_budget=180, **kwargs)
                    before, after = run.call_args_list[0].args[0], run.call_args_list[2].args[0]
                    self.assertEqual(before, after)
                    self.assertEqual("--include-source" in after, include_source)
                    self.assertEqual(after[after.index("--token-budget") + 1], "180")

    def test_source_defaults_preserve_workspace_and_refresh_failures(self):
        with tempfile.TemporaryDirectory() as tmp, tempfile.TemporaryDirectory() as outside:
            root = pathlib.Path(tmp).resolve()
            external = pathlib.Path(outside) / "External.arch"
            external.write_text("module External end module External\n")
            link = root / "link.arch"
            link.symlink_to(external)
            index = root / "index"
            index.mkdir()
            (index / "files.jsonl").write_text(json.dumps({"abs_path": str(link)}) + "\n")
            with patch.object(server, "WORKSPACE_ROOTS", [root]), patch.object(server, "_run") as run:
                with self.assertRaisesRegex(ValueError, "outside allowed roots"):
                    server.arch_graph_context("External", index=str(index))
                run.assert_not_called()
            with patch.object(server, "WORKSPACE_ROOTS", [root]), patch.object(
                server, "_run", return_value="[ERROR] index missing"
            ) as run:
                self.assertEqual(server.arch_graph_context("Missing", index=str(root / "missing")),
                                 "[ERROR] index missing")
                self.assertEqual(run.call_count, 1)
            source = root / "Demo.arch"
            source.write_text("module Demo end module Demo\n")
            with patch.object(server, "WORKSPACE_ROOTS", [root]), patch.object(
                server, "_run", side_effect=["[ERROR] source changed since indexing", "[ERROR] build failed"]
            ) as run:
                result = server.arch_graph_context("Demo", index=str(root / "missing"), source_paths=[str(source)])
                self.assertEqual(result, "[ERROR] build failed")
                self.assertEqual(run.call_count, 2)


if __name__ == "__main__":
    unittest.main()
