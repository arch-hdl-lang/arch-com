"""Opt-in selector invocation and offline inventory; never launch a model.

The caller owns authentication, live-run authorization, deadlines and complete
raw-event accounting. This helper does not change normal coding-agent settings.
"""

from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import selectors
import stat
import subprocess
import time


POLICY = "compiler-selector-profile-v2"
INSTRUCTIONS = Path(__file__).with_name("compiler_selector_instructions.md")
DISABLED = ("shell_tool", "unified_exec", "apps", "multi_agent", "multi_agent_v2",
            "browser_use", "computer_use", "image_generation", "view_image", "sleep_tool",
            "plugins", "hooks", "skill_search", "tool_search", "tool_suggest",
            "workspace_dependencies", "goals", "code_mode")
COMPONENT_ROLES = {"permissions.instructions": "developer", "collaboration_mode.instructions": "developer",
                   "multi_agent.role_instructions": "developer", "multi_agent.mode_instructions": "developer",
                   "environments.environment_context": "user", "user.text": "user"}


def _file(path, limit):
    path = Path(path)
    if not path.is_absolute() or not path.is_file() or path.stat().st_size > limit:
        raise ValueError("expected a bounded absolute file")
    with os.fdopen(os.open(path, os.O_RDONLY | os.O_NONBLOCK), "rb") as stream:
        if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):
            raise ValueError("expected a regular file")
        content = stream.read(limit + 1)
    if len(content) > limit:
        raise ValueError("file exceeds its byte bound")
    return path.resolve(), content


def _pin(data):
    return hashlib.sha256(data).hexdigest()


def _file_pin(path, limit=512 * 1024 * 1024):
    digest = hashlib.sha256()
    with os.fdopen(os.open(path, os.O_RDONLY | os.O_NONBLOCK), "rb") as stream:
        info = os.fstat(stream.fileno())
        if not stat.S_ISREG(info.st_mode) or info.st_size > limit:
            raise ValueError("expected a bounded regular file")
        count = 0
        while chunk := stream.read(min(65536, limit - count + 1)):
            count += len(chunk)
            if count > limit:
                raise ValueError("file exceeds its byte bound")
            digest.update(chunk)
    return digest.hexdigest()


def build_selector_invocation(codex: Path, workspace: Path, home: Path, catalog: Path,
                              schema: Path, prompt: str, *, model="gpt-6-luna") -> dict:
    """Prepare a fixed argv/profile; caller executes only after separate approval.

    Use a dedicated workspace and CODEX_HOME, not a compiler checkout or normal
    coding-agent home. Authentication is neither read nor copied by this helper.
    The caller must close stdin and check component pins before execution.
    """
    if not isinstance(prompt, str) or not 1 <= len(prompt.encode()) <= 65536:
        raise ValueError("prompt must contain 1..65536 UTF-8 bytes")
    codex = Path(codex)
    if (not codex.is_absolute() or not codex.is_file()
            or codex.stat().st_size > 512 * 1024 * 1024 or not os.access(codex, os.X_OK)):
        raise ValueError("Codex must be a bounded absolute executable")
    codex = codex.resolve()
    binary_pin = _file_pin(codex)
    host = codex.with_name("codex-code-mode-host")
    if not host.is_file() or not os.access(host, os.X_OK):
        raise ValueError("Codex requires its bundled executable code-mode host")
    host = host.resolve()
    host_pin = _file_pin(host)
    workspace, home = Path(workspace), Path(home)
    if any(not p.is_absolute() or not p.is_dir() or p.is_symlink() for p in (workspace, home)):
        raise ValueError("workspace and home must be absolute nonsymlink directories")
    workspace, home = workspace.resolve(), home.resolve()
    if workspace == home or any((p / ".git").exists() for p in (workspace, *workspace.parents)):
        raise ValueError("use a dedicated non-repository workspace and separate home")
    for parent in (workspace, *workspace.parents):
        if (parent / ".codex/config.toml").exists():
            raise ValueError("dedicated workspace must not inherit project configuration")
    if (home / "config.toml").exists():
        raise ValueError("dedicated home must not contain user configuration")
    catalog, catalog_bytes = _file(catalog, 16 * 1024 * 1024)
    data = json.loads(catalog_bytes)
    models = data.get("models", []) if isinstance(data, dict) else []
    if not isinstance(model, str) or not any(isinstance(m, dict) and m.get("slug") == model for m in models):
        raise ValueError("selected model is absent from the supplied pinned catalog")
    schema, schema_bytes = _file(schema, 65536)
    if not isinstance(json.loads(schema_bytes), dict):
        raise ValueError("output schema must be an object")
    instructions, instruction_bytes = _file(INSTRUCTIONS.resolve(), 8192)
    settings = {
        "model": model, "model_reasoning_effort": "low", "sandbox_mode": "read-only",
        "approval_policy": "on-request", "web_search": "disabled", "project_doc_max_bytes": 0,
        "model_catalog_json": str(catalog), "model_instructions_file": str(instructions),
        "skills.include_instructions": False, "skills.bundled.enabled": False,
    }
    # CLI 0.159.2 requires the host at startup even with code_mode disabled.
    options = ["--sandbox", "read-only", "-C", str(workspace), "--enable", "code_mode_host"]
    for key, value in settings.items():
        options += ["-c", key + "=" + json.dumps(value)]
    for feature in DISABLED:
        options += ["--disable", feature]
    files = {str(codex): binary_pin, str(host): host_pin, **{str(path): _pin(content) for path, content in (
        (catalog, catalog_bytes), (schema, schema_bytes), (instructions, instruction_bytes))}}
    return {"policy": POLICY, "command": [str(codex), "--no-daemon", "exec", "--json",
            "--ignore-user-config", "--strict-config", *options, "--ephemeral",
            "--skip-git-repo-check", "--output-schema", str(schema), prompt],
            "offline_command": [str(codex), "--no-daemon", *options, "debug", "prompt-input", prompt],
            "environment": {"CODEX_HOME": str(home)}, "stdin": "DEVNULL",
            "file_sha256": files, "prompt_sha256": _pin(prompt.encode()),
            "settings": settings, "disabled_features": list(DISABLED),
            "complete_wire_request_captured": False}


def validate_selector_invocation(invocation):
    """Reject changed pinned files and altered argv instead of silently adapting."""
    if invocation.get("policy") != POLICY:
        raise ValueError("unknown selector profile")
    settings = invocation["settings"]
    # Rebuild only canonical bounded inputs, never paths injected into the pin map.
    rebuilt = build_selector_invocation(
        Path(invocation["command"][0]), Path(invocation["command"][invocation["command"].index("-C") + 1]),
        Path(invocation["environment"]["CODEX_HOME"]), Path(settings["model_catalog_json"]),
        Path(invocation["command"][invocation["command"].index("--output-schema") + 1]),
        invocation["command"][-1], model=settings["model"])
    if rebuilt != invocation:
        raise ValueError("selector invocation differs from the fixed profile")


def inventory_prompt_input(messages, prompt):
    """Validate rendered permissions and retain hashes/sizes, not private text."""
    if not isinstance(messages, list):
        raise ValueError("prompt inventory is not a message list")
    sections, permissions, user_texts = [], [], []
    for message in messages:
        if (not isinstance(message, dict) or message.get("type") != "message"
                or set(message) - {"type", "id", "role", "content", "internal_chat_message_metadata_passthrough"}
                or (message.get("id") is not None and not isinstance(message["id"], str))):
            raise ValueError("unexpected prompt item")
        content = message.get("content", [])
        metadata = message.get("internal_chat_message_metadata_passthrough", {})
        if not isinstance(metadata, dict) or set(metadata) - {"content_item_kinds", "turn_id", "create_time"}:
            raise ValueError("unexpected prompt metadata")
        kinds = metadata.get("content_item_kinds", []) if isinstance(metadata, dict) else []
        if not isinstance(content, list) or not isinstance(kinds, list) or not content or len(content) != len(kinds):
            raise ValueError("missing prompt component labels")
        for part, kind in zip(content, kinds):
            if (not isinstance(kind, str) or kind not in COMPONENT_ROLES
                    or message.get("role") != COMPONENT_ROLES[kind]
                    or not isinstance(part, dict) or set(part) != {"type", "text"} or part.get("type") != "input_text"
                    or not isinstance(part.get("text"), str)):
                raise ValueError("unexpected selector prompt component")
            text = part["text"]
            if kind == "permissions.instructions":
                if message.get("role") != "developer":
                    raise ValueError("permission text has the wrong role")
                permissions.append(text)
            if kind == "user.text":
                user_texts.append(text)
            sections.append({"role": message.get("role"), "kind": kind,
                             "utf8_bytes": len(text.encode()), "sha256": _pin(text.encode())})
    if len(permissions) != 1 or "`sandbox_mode` is `read-only`" not in permissions[0]:
        raise ValueError("resolved selector permissions are not explicitly read-only")
    if any(token in permissions[0] for token in ("`workspace-write`", "`danger-full-access`")):
        raise ValueError("resolved selector permissions include writable modes")
    if user_texts != [prompt]:
        raise ValueError("rendered selector task differs from the supplied prompt")
    return {"sections": sections, "section_text_utf8_bytes": sum(s["utf8_bytes"] for s in sections),
            "read_only_verified": True, "complete_wire_request_captured": False,
            "selector_instructions_wire_verified": False,
            "scope": "Offline debug input list only; excludes separately supplied base instructions and schema"}


def _bounded_debug(command, env, *, timeout=30, limit=256 * 1024):
    """Drain both pipes with a shared byte bound and deadline; reap on failure."""
    deadline = time.monotonic() + timeout
    buffers = {"stdout": bytearray(), "stderr": bytearray()}
    total = 0
    with subprocess.Popen(command, env=env, stdin=subprocess.DEVNULL,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE) as process:
        try:
            with selectors.DefaultSelector() as selector:
                selector.register(process.stdout, selectors.EVENT_READ, "stdout")
                selector.register(process.stderr, selectors.EVENT_READ, "stderr")
                while selector.get_map():
                    remaining = deadline - time.monotonic()
                    if remaining <= 0:
                        raise ValueError("offline selector inspection timed out")
                    for key, _ in selector.select(remaining):
                        chunk = os.read(key.fd, min(65536, limit - total + 1))
                        if not chunk:
                            selector.unregister(key.fileobj)
                            continue
                        total += len(chunk)
                        if total > limit:
                            raise ValueError("offline selector inventory exceeds its bound")
                        buffers[key.data].extend(chunk)
            returncode = process.wait(timeout=max(0, deadline - time.monotonic()))
        except BaseException:
            process.kill()
            process.wait()
            raise
    if returncode or buffers["stderr"].strip():
        raise ValueError("offline selector inspection failed or emitted diagnostics")
    return bytes(buffers["stdout"])


def inspect_selector_invocation(invocation):
    """Run only `debug prompt-input`; never run the prepared live command."""
    validate_selector_invocation(invocation)
    env = {key: value for key, value in os.environ.items()
           if key not in ("OPENAI_API_KEY", "CODEX_API_KEY", "OPENAI_BASE_URL")}
    env.update(invocation["environment"])
    output = _bounded_debug(invocation["offline_command"], env)
    inventory = inventory_prompt_input(json.loads(output), invocation["command"][-1])
    validate_selector_invocation(invocation)
    inventory.update(file_sha256=invocation["file_sha256"], prompt_sha256=invocation["prompt_sha256"],
                     settings=invocation["settings"], disabled_features=invocation["disabled_features"])
    return inventory


def prepare_selector_run(codex: Path, workspace: Path, home: Path, catalog: Path,
                         schema: Path, prompt: str, *, model="gpt-6-luna") -> dict:
    """Prepare an inspected run without executing it or granting live approval."""
    invocation = build_selector_invocation(codex, workspace, home, catalog, schema, prompt, model=model)
    inventory = inspect_selector_invocation(invocation)
    return {"status": "offline_inspected", "invocation": invocation, "offline_inventory": inventory,
            "live_execution_authorized": False}
