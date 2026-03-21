#!/usr/bin/env python3
"""ARCH HDL MCP Server — gives any AI assistant the ability to read the
ARCH language reference and invoke the compiler (check / build / sim)."""

import os
import subprocess
import pathlib
import glob as globmod
from mcp.server.fastmcp import FastMCP

PROJECT_ROOT = pathlib.Path(__file__).resolve().parent.parent
ARCH_BIN = os.environ.get("ARCH_BIN", str(PROJECT_ROOT / "target" / "release" / "arch"))

mcp = FastMCP("arch-hdl")


# ── Resources ────────────────────────────────────────────────────────────

@mcp.resource("arch://reference-card")
def reference_card() -> str:
    """Full ARCH HDL AI Reference Card — language syntax, constructs, and examples."""
    return (PROJECT_ROOT / "doc" / "Arch_AI_Reference_Card.md").read_text()


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
            f"[ERROR] arch binary not found at {ARCH_BIN}\n"
            "Build it first: cargo build --release"
        )


# ── Tools ────────────────────────────────────────────────────────────────

@mcp.tool()
def arch_check(files: list[str]) -> str:
    """Type-check one or more .arch files. Returns diagnostics."""
    paths = [str(_resolve_safe(f)) for f in files]
    return _run([ARCH_BIN, "check"] + paths)


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
        sv_path = _resolve_safe(output) if output else pathlib.Path(paths[0]).with_suffix(".sv")
        if sv_path.exists():
            result += f"\n\n--- Generated SystemVerilog ---\n{sv_path.read_text()}"

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
def read_arch_file(path: str) -> str:
    """Read the contents of a .arch file in the project."""
    resolved = _resolve_safe(path)
    if not resolved.exists():
        return f"[ERROR] File not found: {path}"
    return resolved.read_text()


@mcp.tool()
def write_arch_file(path: str, content: str) -> str:
    """Write content to a .arch file in the project. Creates parent
    directories if needed."""
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
