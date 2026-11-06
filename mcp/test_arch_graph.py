"""Graph MCP boundary and refresh tests (run in the MCP virtual environment)."""

import json
import pathlib
import tempfile
import unittest
from unittest.mock import patch

import arch_mcp_server as server


class GraphToolsTests(unittest.TestCase):
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


if __name__ == "__main__":
    unittest.main()
