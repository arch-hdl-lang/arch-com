"""Run with the MCP environment: python -m unittest discover -s mcp -p 'test_*.py'."""
import asyncio
import unittest
from unittest.mock import patch

import arch_mcp_server as server


class AdviceToolTests(unittest.TestCase):
    def test_schema_exposes_optional_feature_boolean(self):
        tools = asyncio.run(server.mcp.list_tools())
        schema = next(t.inputSchema for t in tools if t.name == "arch_advise")
        self.assertEqual(schema["properties"]["feature"]["type"], "boolean")
        self.assertIs(schema["properties"]["feature"]["default"], False)
        self.assertNotIn("feature", schema.get("required", []))

    def test_mcp_dispatch_forwards_feature_and_preserves_default(self):
        for feature in [None, False, True]:
            with self.subTest(feature=feature), patch.object(server, "_run", return_value="matched") as run:
                arguments = {"query": "round robin arbiter", "top": 2}
                if feature is not None:
                    arguments["feature"] = feature
                asyncio.run(server.mcp.call_tool("arch_advise", arguments))
                expected = [server.ARCH_BIN, "advise", "-k", "2"]
                if feature:
                    expected.append("--feature")
                run.assert_called_once_with(expected + ["round robin arbiter"])


if __name__ == "__main__":
    unittest.main()
