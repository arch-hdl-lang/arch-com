#!/usr/bin/env python3
"""ARCH HDL MCP Server — gives any AI assistant the ability to read the
ARCH language reference and invoke the compiler (check / build / sim).

MCP instructions are in instructions.md.
Construct syntax snippets are in construct_syntax.md.
"""

import os
import subprocess
import pathlib
import shutil
from mcp.server.fastmcp import FastMCP

SCRIPT_DIR = pathlib.Path(__file__).resolve().parent
PROJECT_ROOT = SCRIPT_DIR.parent

# Load .env written by install.sh (contains ARCH_BIN path)
_env_file = SCRIPT_DIR / ".env"
if _env_file.exists():
    for line in _env_file.read_text().splitlines():
        if "=" in line and not line.startswith("#"):
            k, v = line.split("=", 1)
            os.environ.setdefault(k.strip(), v.strip())

ARCH_BIN = os.environ.get("ARCH_BIN", str(PROJECT_ROOT / "target" / "release" / "arch"))
VERILATOR_BIN = shutil.which("verilator") or "verilator"

# ── Load instructions and construct syntax from markdown files ────────────

_INSTRUCTIONS = (SCRIPT_DIR / "instructions.md").read_text()


def _load_construct_syntax() -> dict[str, str]:
    """Parse Arch_AI_Reference_Card.md into a dict keyed by construct name.

    Extracts:
    - Each '### name' subsection under '## 4. Construct Cards'
    - '## 2. Types' as 'types'
    - '## 3. Expressions & Operators' as 'expressions'
    """
    text = (PROJECT_ROOT / "doc" / "Arch_AI_Reference_Card.md").read_text()
    constructs: dict[str, str] = {}
    current_name: str | None = None
    current_lines: list[str] = []
    in_section: str | None = None  # 'types' | 'expressions' | 'constructs'

    def flush() -> None:
        if current_name is not None:
            constructs[current_name] = "".join(current_lines)

    for line in text.splitlines(keepends=True):
        stripped = line.strip()

        if stripped.startswith("## "):
            flush()
            current_name = None
            current_lines = []
            if "2." in stripped and "Types" in stripped:
                in_section = "types"
                current_name = "types"
            elif "3." in stripped and "Expressions" in stripped:
                in_section = "expressions"
                current_name = "expressions"
            elif "4." in stripped and "Construct" in stripped:
                in_section = "constructs"
            else:
                in_section = None
        elif stripped.startswith("### ") and in_section in ("types", "expressions", "constructs"):
            flush()
            current_name = stripped[4:].strip().lower()
            current_lines = []
        elif current_name is not None:
            current_lines.append(line)

    flush()
    return constructs


CONSTRUCT_SYNTAX = _load_construct_syntax()

# ── Reserved keywords (for syntax hints) ────────────────────────────────

RESERVED_KEYWORDS = {
    "module", "pipeline", "fsm", "fifo", "ram", "arbiter", "synchronizer",
    "counter", "regfile", "interface", "domain", "struct", "enum", "package",
    "generate", "inst", "port", "ports", "param", "reg", "wire", "let",
    "comb", "seq", "latch", "assert", "cover", "if", "else", "elsif", "end",
    "for", "on", "rising", "falling", "init", "reset", "sync", "async",
    "high", "low", "none", "forward", "stall", "flush", "when", "kind",
    "policy", "true", "false", "todo", "use", "inside", "bus",
    "template", "function", "return", "stage", "store", "default",
    "testbench", "initial", "repeat", "clkgate", "linklist", "hook",
    "implements", "from", "match", "transition", "to", "unique",
}

mcp = FastMCP("arch-hdl", instructions=_INSTRUCTIONS)


# ── Resources ────────────────────────────────────────────────────────────

@mcp.resource("arch://reference-card")
def reference_card() -> str:
    """Full ARCH HDL AI Reference Card — language syntax, constructs, and examples."""
    return (PROJECT_ROOT / "doc" / "Arch_AI_Reference_Card.md").read_text()


@mcp.resource("arch://specification")
def specification() -> str:
    """Full ARCH HDL Language Specification — detailed semantics, type system, and all constructs."""
    return (PROJECT_ROOT / "doc" / "ARCH_HDL_Specification.md").read_text()


@mcp.resource("arch://compiler-status")
def compiler_status() -> str:
    """Current compiler feature status and changelog."""
    return (PROJECT_ROOT / "doc" / "COMPILER_STATUS.md").read_text()


# ── Helpers ──────────────────────────────────────────────────────────────

def _resolve_safe(path: str) -> pathlib.Path:
    """Resolve *path* and ensure it stays under PROJECT_ROOT."""
    resolved = (PROJECT_ROOT / path).resolve()
    if not str(resolved).startswith(str(PROJECT_ROOT)):
        raise ValueError(f"Path escapes project root: {path}")
    return resolved


def _run(args: list[str], timeout: int = 30, cwd: str | None = None) -> str:
    """Run a subprocess and return combined stdout + stderr."""
    try:
        result = subprocess.run(
            args,
            capture_output=True,
            text=True,
            timeout=timeout,
            cwd=cwd or str(PROJECT_ROOT),
        )
        parts = []
        if result.stdout.strip():
            parts.append(result.stdout.strip())
        if result.stderr.strip():
            parts.append(result.stderr.strip())
        prefix = "OK" if result.returncode == 0 else f"ERROR (exit {result.returncode})"
        return f"[{prefix}]\n" + "\n".join(parts)
    except subprocess.TimeoutExpired:
        return f"[ERROR] Command timed out after {timeout}s"
    except FileNotFoundError:
        return (
            f"[ERROR] arch binary not found at {args[0]}\n"
            "Build it first: cargo build --release"
        )


# ── Tools ────────────────────────────────────────────────────────────────

@mcp.tool()
def get_construct_syntax(construct: str) -> str:
    """Get the ARCH syntax for a specific construct. Call this BEFORE writing
    any .arch code to avoid common mistakes.

    Available constructs: module, function, pipeline, fsm, fifo, synchronizer,
    ram, counter, arbiter, regfile, linklist, generate, bus, template, package,
    types, expressions

    Also returns reserved keywords to avoid as signal/register names.
    Note: 'in', 'out', 'state' are contextual — safe to use as port/signal names.
    Module names are NOT required to be PascalCase — use whatever name the target expects."""
    key = construct.lower().strip()
    syntax = CONSTRUCT_SYNTAX.get(key)
    if syntax is None:
        available = ", ".join(sorted(CONSTRUCT_SYNTAX.keys()))
        return f"[ERROR] Unknown construct '{construct}'. Available: {available}"

    result = f"--- ARCH syntax: {construct} ---\n{syntax}\n"
    result += "--- Reserved keywords (do NOT use as signal names) ---\n"
    result += ", ".join(sorted(RESERVED_KEYWORDS))
    return result


@mcp.tool()
def arch_check(files: list[str]) -> str:
    """Type-check one or more .arch files. Returns diagnostics.

    Note: every invocation records error→fix pairs into ~/.arch/learn/ for
    retrieval via `arch_advise`. Disable with env ARCH_NO_LEARN=1.
    """
    paths = [str(_resolve_safe(f)) for f in files]
    return _run([ARCH_BIN, "check"] + paths)


@mcp.tool()
def arch_advise(query: str, top: int = 3) -> str:
    """Retrieve past error→fix pairs from the local learning store that
    match `query`. Useful when an agent hits a compiler error and wants to
    see how the same user fixed a similar error before. Returns the top-K
    matches (default 3) with error code, error message, file, and diff.

    The store is built passively by every `arch check/build/sim/formal`
    invocation and lives at ~/.arch/learn/. Run `arch_learn_index` once
    after new events accumulate to refresh the BM25 retrieval index.

    Example queries:
      - "width mismatch trunc"
      - "duplicate definition"
      - "undeclared identifier"
    """
    return _run([ARCH_BIN, "advise", "-k", str(top), query])


@mcp.tool()
def arch_learn_index() -> str:
    """Rebuild the BM25 retrieval index over the local learning store.
    Call after many new events have been recorded; `arch_advise` works
    without this but may miss recent entries until the index is refreshed.
    """
    return _run([ARCH_BIN, "learn-index"])


@mcp.tool()
def arch_learn_prune(
    code: str | None = None,
    contains: str | None = None,
    older_than_days: int | None = None,
    dry_run: bool = True,
) -> str:
    """Remove events from the local learning store. At least one filter is
    required. An event is removed if ANY filter matches.

    - code: exact error_code (e.g. "parse_error", "other", "width_mismatch")
    - contains: substring match against diff, message, or file path
    - older_than_days: remove entries older than N days

    Defaults to dry_run=True — always preview before deleting. Pass
    dry_run=False to actually prune.
    """
    cmd = [ARCH_BIN, "learn-prune"]
    if code: cmd += ["--code", code]
    if contains: cmd += ["--contains", contains]
    if older_than_days is not None: cmd += ["--older-than-days", str(older_than_days)]
    if dry_run: cmd += ["--dry-run"]
    return _run(cmd)


@mcp.tool()
def arch_learn_stats() -> str:
    """Summarize the local learning store: total events and counts by
    error_code. Useful to see what kinds of mistakes the user has been
    making, or to decide whether the store is worth querying."""
    return _run([ARCH_BIN, "learn-stats"])


@mcp.tool()
def arch_build(files: list[str], output: str | None = None) -> str:
    """Compile .arch files to SystemVerilog. Returns compiler output and
    the generated SV content."""
    paths = [str(_resolve_safe(f)) for f in files]
    cmd = [ARCH_BIN, "build"] + paths
    if output:
        cmd += ["-o", str(_resolve_safe(output))]

    result = _run(cmd)

    # Try to read generated SV and append it
    if result.startswith("[OK]"):
        if output:
            sv_path = _resolve_safe(output)
            if sv_path.exists():
                result += f"\n\n--- Generated SystemVerilog ---\n{sv_path.read_text()}"
        else:
            for p in paths:
                sv_path = pathlib.Path(p).with_suffix(".sv")
                if sv_path.exists():
                    result += f"\n\n--- {sv_path.name} ---\n{sv_path.read_text()}"

    return result


@mcp.tool()
def arch_build_and_lint(files: list[str], top_module: str, output: str | None = None) -> str:
    """Compile .arch files to SystemVerilog AND run Verilator lint in one step.
    Returns both compiler output and lint results."""
    paths = [str(_resolve_safe(f)) for f in files]
    cmd = [ARCH_BIN, "build"] + paths

    if output:
        sv_path = _resolve_safe(output)
        cmd += ["-o", str(sv_path)]
        sv_files = [str(sv_path)]
    else:
        sv_files = [str(pathlib.Path(p).with_suffix(".sv")) for p in paths]

    result = _run(cmd)
    if not result.startswith("[OK]"):
        return result

    # Run Verilator lint on all generated SV files
    # Add -I flags for directories containing SV files so Verilator finds submodules
    inc_dirs = list({f"-I{str(pathlib.Path(f).parent)}" for f in sv_files})
    lint_result = _run(
        [VERILATOR_BIN, "--lint-only", "-Wno-DECLFILENAME", "-Wno-UNUSEDSIGNAL"]
        + inc_dirs + sv_files + ["--top-module", top_module],
        timeout=15,
    )

    if lint_result.startswith("[OK]"):
        result += "\n\n[Verilator lint: PASS]"
    else:
        result += f"\n\n[Verilator lint: FAIL]\n{lint_result}"

    return result


@mcp.tool()
def arch_sim(
    arch_files: list[str],
    tb_files: list[str] | None = None,
    outdir: str | None = None,
    check_uninit: bool = False,
    timeout: int = 60,
) -> str:
    """Compile and simulate .arch files with an optional C++ testbench.
    Returns simulation stdout/stderr. Timeout defaults to 60s."""
    paths = [str(_resolve_safe(f)) for f in arch_files]
    cmd = [ARCH_BIN, "sim"] + paths
    if tb_files:
        for tb in tb_files:
            cmd += ["--tb", str(_resolve_safe(tb))]
    if outdir:
        cmd += ["-o", str(_resolve_safe(outdir))]
    if check_uninit:
        cmd.append("--check-uninit")
    return _run(cmd, timeout=timeout)


@mcp.tool()
def write_and_check(path: str, content: str, extra_files: list[str] | None = None) -> str:
    """Write a .arch file AND immediately type-check it. Returns write
    confirmation + check diagnostics in one call. If the module instantiates
    other modules, pass their .arch files in extra_files."""
    resolved = _resolve_safe(path)
    resolved.parent.mkdir(parents=True, exist_ok=True)
    resolved.write_text(content)

    check_files = []
    if extra_files:
        check_files = [str(_resolve_safe(f)) for f in extra_files]
    check_files.append(str(resolved))

    check_result = _run([ARCH_BIN, "check"] + check_files)
    return f"[OK] Wrote {resolved.relative_to(PROJECT_ROOT)}\n\n{check_result}"


@mcp.tool()
def read_arch_file(path: str) -> str:
    """Read the contents of a .arch file in the project."""
    resolved = _resolve_safe(path)
    if not resolved.exists():
        return f"[ERROR] File not found: {path}"
    return resolved.read_text()


@mcp.tool()
def write_arch_file(path: str, content: str) -> str:
    """Write content to a .arch file in the project. Creates parent
    directories if needed.

    TIP: Call get_construct_syntax() first to get correct syntax.
    TIP: Use write_and_check() instead to write + type-check in one call."""
    resolved = _resolve_safe(path)
    resolved.parent.mkdir(parents=True, exist_ok=True)
    resolved.write_text(content)
    return f"[OK] Wrote {resolved.relative_to(PROJECT_ROOT)}"


@mcp.tool()
def list_arch_files(directory: str = ".") -> str:
    """List all .arch files in a directory (recursive)."""
    resolved = _resolve_safe(directory)
    if not resolved.is_dir():
        return f"[ERROR] Not a directory: {directory}"
    files = sorted(resolved.rglob("*.arch"))
    rel = [str(f.relative_to(PROJECT_ROOT)) for f in files]
    return "\n".join(rel) if rel else "(no .arch files found)"


# ── Entry point ──────────────────────────────────────────────────────────

if __name__ == "__main__":
    mcp.run(transport="stdio")
