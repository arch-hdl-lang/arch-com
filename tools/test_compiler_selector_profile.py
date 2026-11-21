import copy
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

from compiler_selector_profile import (COMPONENT_ROLES, _bounded_debug, _file_pin, build_selector_invocation, inspect_selector_invocation,
                                      inventory_prompt_input, prepare_selector_run, validate_selector_invocation)


class SelectorProfileTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.binary = self.root / "codex"
        self.binary.write_text("fixture")
        self.binary.chmod(0o700)
        self.workspace = self.root / "workspace"
        self.home = self.root / "home"
        self.workspace.mkdir()
        self.home.mkdir()
        self.catalog = self.root / "models.json"
        self.catalog.write_text(json.dumps({"models": [{"slug": "gpt-6-luna"}]}))
        self.schema = self.root / "schema.json"
        self.schema.write_text('{"type":"object"}')
        self.prompt = "Select supplied evidence only."

    def build(self):
        return build_selector_invocation(self.binary, self.workspace, self.home, self.catalog,
                                         self.schema, self.prompt)

    def messages(self):
        return [{"type": "message", "role": "developer",
                 "content": [{"type": "input_text", "text": "`sandbox_mode` is `read-only`"}],
                 "internal_chat_message_metadata_passthrough": {"content_item_kinds": ["permissions.instructions"]}},
                {"type": "message", "role": "user",
                 "content": [{"type": "input_text", "text": self.prompt}],
                 "internal_chat_message_metadata_passthrough": {"content_item_kinds": ["user.text"]}}]

    def test_profile_is_read_only_and_tool_restricted_without_auto_approval(self):
        invocation = self.build()
        validate_selector_invocation(invocation)
        for command in (invocation["command"], invocation["offline_command"]):
            self.assertEqual(command[command.index("--sandbox") + 1], "read-only")
            self.assertNotIn("--approve-for-me", command)
            self.assertNotIn("--dangerously-bypass-approvals-and-sandbox", command)
        self.assertIn("--strict-config", invocation["command"])
        self.assertIn("--ignore-user-config", invocation["command"])
        self.assertEqual(invocation["environment"], {"CODEX_HOME": str(self.home)})
        self.assertEqual(invocation["stdin"], "DEVNULL")
        self.assertEqual(invocation["settings"]["project_doc_max_bytes"], 0)
        self.assertFalse(invocation["settings"]["skills.include_instructions"])
        self.assertEqual(len(invocation["file_sha256"]), 4)

    def test_changed_files_reject_instead_of_refreshing_pins(self):
        for path in (self.binary, self.catalog, self.schema):
            with self.subTest(path=path):
                original = path.read_bytes()
                invocation = self.build()
                path.write_bytes(original + b" ")
                with self.assertRaises(ValueError):
                    validate_selector_invocation(invocation)
                path.write_bytes(original)

    def test_altered_commands_or_profile_are_rejected(self):
        for mutate in (
                lambda r: r["command"].insert(3, "--approve-for-me"),
                lambda r: r["offline_command"].__setitem__(-1, "other question"),
                lambda r: r["environment"].update(UNSAFE="override"),
                lambda r: r["file_sha256"].update({"/dev/zero": "unexpected"}),
                lambda r: r.__setitem__("stdin", "inherit"),
                lambda r: r.__setitem__("prompt_sha256", "wrong")):
            invocation = self.build()
            mutate(invocation)
            with self.assertRaises(ValueError):
                validate_selector_invocation(invocation)

    def test_repository_and_inherited_configuration_are_not_selector_workspaces(self):
        for path in (self.workspace / ".git", self.root / ".codex/config.toml", self.home / "config.toml"):
            path.parent.mkdir(exist_ok=True)
            path.write_text("config")
            with self.assertRaises(ValueError):
                self.build()
            path.unlink()

    def test_catalog_model_and_input_bounds(self):
        self.catalog.write_text('{"models":[]}')
        with self.assertRaises(ValueError):
            self.build()
        self.catalog.write_text('{"models":[{"slug":"gpt-6-luna"}]}')
        self.prompt = "x" * 65537
        with self.assertRaises(ValueError):
            self.build()

    def test_inventory_records_hashes_and_bytes_without_private_text(self):
        result = inventory_prompt_input(self.messages(), self.prompt)
        self.assertTrue(result["read_only_verified"])
        self.assertFalse(result["complete_wire_request_captured"])
        self.assertNotIn(self.prompt, json.dumps(result))
        self.assertEqual(result["section_text_utf8_bytes"],
                         len(self.prompt.encode()) + len("`sandbox_mode` is `read-only`"))

    def test_inventory_fail_closed_mutations(self):
        original = self.messages()
        mutations = [
            lambda r: r[0]["content"][0].__setitem__("text", "`sandbox_mode` is `workspace-write`"),
            lambda r: r[0].__setitem__("role", "user"),
            lambda r: r[1].__setitem__("role", "developer"),
            lambda r: r[0].__setitem__("internal_chat_message_metadata_passthrough", None),
            lambda r: r[0].__setitem__("content", [None]),
            lambda r: r[0].__setitem__("tool_call", {"name": "unexpected"}),
            lambda r: r[0]["content"][0].__setitem__("tool_call", {}),
            lambda r: r[0]["internal_chat_message_metadata_passthrough"].__setitem__("extra", {}),
            lambda r: r[0]["internal_chat_message_metadata_passthrough"].__setitem__(
                "content_item_kinds", ["agents_md.instructions"]),
            lambda r: r[0]["internal_chat_message_metadata_passthrough"].__setitem__(
                "content_item_kinds", ["host_skills.instructions"]),
            lambda r: r[0].pop("internal_chat_message_metadata_passthrough"),
            lambda r: r[1]["content"][0].__setitem__("text", "different task"),
            lambda r: r.append(copy.deepcopy(r[0])),
        ]
        for mutate in mutations:
            messages = copy.deepcopy(original)
            mutate(messages)
            with self.assertRaises(ValueError):
                inventory_prompt_input(messages, self.prompt)

    def test_inspection_can_only_run_debug_and_closes_stdin(self):
        invocation = self.build()
        with patch("compiler_selector_profile._bounded_debug", return_value=json.dumps(self.messages()).encode()) as run:
            result = inspect_selector_invocation(invocation)
        self.assertTrue(result["read_only_verified"])
        args, _ = run.call_args
        self.assertIn("debug", args[0])
        self.assertIn("prompt-input", args[0])
        self.assertNotIn("exec", args[0])
        self.assertEqual(args[1]["CODEX_HOME"], str(self.home))

    def test_inspection_rejects_changed_files(self):
        invocation = self.build()
        def mutate(*args):
            self.schema.write_text('{"type":"string"}')
            return json.dumps(self.messages()).encode()
        with patch("compiler_selector_profile._bounded_debug", side_effect=mutate):
            with self.assertRaises(ValueError):
                inspect_selector_invocation(invocation)

    def test_every_component_has_its_expected_role(self):
        for kind, role in COMPONENT_ROLES.items():
            for wrong in ("system", "assistant", "user" if role == "developer" else "developer"):
                messages = self.messages()
                messages.append({"type": "message", "role": wrong,
                                 "content": [{"type": "input_text", "text": "context"}],
                                 "internal_chat_message_metadata_passthrough": {"content_item_kinds": [kind]}})
                with self.subTest(kind=kind, wrong=wrong), self.assertRaises(ValueError):
                    inventory_prompt_input(messages, self.prompt)

    def test_debug_capture_bounds_both_streams_and_deadline(self):
        for code in ("import os; os.write(1,b'x'*8192)", "import os; os.write(2,b'x'*8192)",
                     "import sys; sys.stderr.write('warning')", "raise SystemExit(1)",
                     "import time; time.sleep(2)"):
            with self.subTest(code=code), self.assertRaises(ValueError):
                _bounded_debug([sys.executable, "-c", code], os.environ, timeout=0.1, limit=1024)
        output = _bounded_debug([sys.executable, "-c", "import sys; print(len(sys.stdin.read()))"], os.environ)
        self.assertEqual(output.strip(), b"0")

    def test_pins_reject_devices_fifos_and_oversized_files(self):
        fifo = self.root / "fifo"
        os.mkfifo(fifo)
        for path in (Path("/dev/zero"), fifo, self.binary):
            with self.subTest(path=path), self.assertRaises(ValueError):
                _file_pin(path, limit=1)

    def test_prepare_requires_inventory_and_does_not_authorize_live_execution(self):
        with patch("compiler_selector_profile.inspect_selector_invocation", return_value={"read_only_verified": True}) as inspect:
            prepared = prepare_selector_run(self.binary, self.workspace, self.home, self.catalog,
                                            self.schema, self.prompt)
        self.assertEqual(prepared["status"], "offline_inspected")
        self.assertFalse(prepared["live_execution_authorized"])
        inspect.assert_called_once_with(prepared["invocation"])
        with patch("compiler_selector_profile.inspect_selector_invocation", side_effect=ValueError("unsafe")):
            with self.assertRaises(ValueError):
                prepare_selector_run(self.binary, self.workspace, self.home, self.catalog, self.schema, self.prompt)


if __name__ == "__main__":
    unittest.main()
