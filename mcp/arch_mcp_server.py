#!/usr/bin/env python3
"""ARCH HDL MCP Server — gives any AI assistant the ability to read the
ARCH language reference and invoke the compiler (check / build / sim).

WORKFLOW: Before writing any .arch code, call get_construct_syntax() or read
the arch://reference-card resource. This avoids common syntax mistakes
(inst connect syntax, reserved keywords, let requiring initializer, etc.)."""

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

mcp = FastMCP(
    "arch-hdl",
    instructions="""You are working with the ARCH hardware description language.

IMPORTANT WORKFLOW — follow this order when writing .arch code:
1. FIRST call get_construct_syntax() for each construct you will use (module, inst, fsm, etc.)
2. THEN write the .arch code using write_and_check() (writes + type-checks in one call)
3. THEN call arch_build_and_lint() to generate SV and verify with Verilator

CONSTRUCT SELECTION — use first-class constructs when possible:
- FSM behavior → use 'fsm' (NOT a module with manual state register)
- FIFO → use 'fifo' (NOT a module with manual pointers)
- RAM/ROM → use 'ram' with appropriate kind (NOT a module with reg array)
- Arbiter → use 'arbiter' with policy (NOT manual grant logic in a module)
- Pipeline → use 'pipeline' with stages (NOT manual valid/stall registers)
- Only use 'module' for pure combinational/registered logic that doesn't fit the above

Common mistakes to avoid:
- inst connections use 'connect port <- signal' and 'connect port -> wire' (NOT '=' or direct assignment)
- Hierarchical references (inst_name.port_name) are FORBIDDEN — always connect outputs explicitly
- 'let' declarations REQUIRE an initializer (let x: UInt<8> = expr;)
- Do NOT use reserved keywords as signal/register names (counter, interface, domain, etc.)
- 'in', 'out', 'state' are contextual keywords — safe to use as port/signal names
- All output ports of an inst MUST be explicitly connected via 'connect port -> wire'
- Use 'elsif' for chained conditionals (NOT 'else if'). 'else' starts a body block; 'elsif' chains.
- Bit-slice syntax: expr[hi:lo] extracts bits (NOT .trunc<Hi,Lo>())
""",
)


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
            f"[ERROR] arch binary not found at {args[0]}\n"
            "Build it first: cargo build --release"
        )


# ── Reserved keywords (for syntax hints) ────────────────────────────────

RESERVED_KEYWORDS = {
    "module", "pipeline", "fsm", "fifo", "ram", "arbiter", "synchronizer",
    "counter", "regfile", "interface", "domain", "struct", "enum",
    "generate", "inst", "port", "param", "reg", "let", "comb", "seq",
    "assert", "cover", "if", "else", "elsif", "end", "for", "on", "rising",
    "falling", "init", "reset", "sync", "async", "high", "low", "none",
    "forward", "stall", "flush", "when", "kind", "policy", "connect",
    "true", "false", "todo",
}

# ── Construct syntax snippets ────────────────────────────────────────────

CONSTRUCT_SYNTAX = {
    "module": """\
module ModuleName
  param PARAM_NAME: const = 32;
  port clk:   in Clock<SysDomain>;
  port rst:   in Reset<Sync>;
  port a:     in UInt<8>;
  port b:     out UInt<8>;

  reg my_reg: UInt<8> init 0 reset rst;

  let wire_name: UInt<8> = a + 1;    // let REQUIRES initializer

  seq on clk rising
    my_reg <= a;
  end seq

  comb
    b = my_reg;
  end comb
end module ModuleName
""",

    "inst": """\
// Instance syntax — use 'connect port <- signal' for inputs,
//                       'connect port -> wire' for outputs.
// Hierarchical references (inst_name.port) are FORBIDDEN.
// All output ports MUST be explicitly connected.

  inst my_inst: ChildModule
    param WIDTH = 16;
    connect clk   <- clk;
    connect rst   <- rst;
    connect data_in  <- input_signal;
    connect data_out -> output_wire;
  end inst my_inst

  // Then use output_wire in comb/seq blocks (NOT my_inst.data_out)
""",

    "fsm": """\
fsm FsmName
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port go:  in Bool;
  port done: out Bool;

  state Idle, Running, Done;
  reset_state Idle;

  transition Idle -> Running when go;
  transition Running -> Done when true;
  transition Done -> Idle when true;

  comb
    done = (state == Done);
  end comb
end fsm FsmName
""",

    "pipeline": """\
pipeline PipeName
  param DEPTH: const = 3;
  port clk:  in Clock<SysDomain>;
  port rst:  in Reset<Sync>;
  port data_in:  in UInt<32>;
  port data_out: out UInt<32>;

  stage S0
    let x: UInt<32> = data_in + 1;
  end stage S0

  stage S1
    let y: UInt<32> = S0.x + 2;
  end stage S1

  comb
    data_out = S1.y;
  end comb
end pipeline PipeName
""",

    "synchronizer": """\
// kind: ff | gray | handshake | reset | pulse
synchronizer SyncName
  kind ff;
  param STAGES: const = 2;
  port src_clk:  in Clock<SrcDomain>;
  port dst_clk:  in Clock<DstDomain>;
  port rst:      in Reset<Async>;
  port data_in:  in Bool;
  port data_out: out Bool;
end synchronizer SyncName
""",

    "fifo": """\
fifo FifoName
  param DEPTH: const = 16;
  port wr_clk:  in Clock<WrDomain>;
  port rd_clk:  in Clock<RdDomain>;   // different domain = async FIFO
  port rst:     in Reset<Async>;
  port wr_en:   in Bool;
  port wr_data: in UInt<8>;
  port rd_en:   in Bool;
  port rd_data: out UInt<8>;
  port full:    out Bool;
  port empty:   out Bool;
end fifo FifoName
""",

    "ram": """\
// kind: single | simple_dual | true_dual
// latency: 0 (async read) | 1 (sync read) | 2 (output reg)
ram RamName
  kind simple_dual;
  latency 1;
  param DEPTH: const = 256;
  param WIDTH: const = 32;
  port clk:    in Clock<SysDomain>;
  port wr_en:  in Bool;
  port wr_addr: in UInt<8>;
  port wr_data: in UInt<32>;
  port rd_addr: in UInt<8>;
  port rd_data: out UInt<32>;
end ram RamName
""",

    "arbiter": """\
// policy: round_robin | priority | weighted<W> | lru | custom
arbiter ArbName
  policy round_robin;
  param N: const = 4;
  port clk:   in Clock<SysDomain>;
  port rst:   in Reset<Sync>;
  port req:   in UInt<N>;
  port grant: out UInt<N>;
end arbiter ArbName
""",

    "regfile": """\
regfile RegfileName
  param XLEN: const = 32;
  param REGS: const = 32;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Async, Low>;
  port rs1_idx: in UInt<5>;
  port rs1_data: out UInt<32>;
  port wr_en:  in Bool;
  port wr_idx: in UInt<5>;
  port wr_data: in UInt<32>;

  init [0] = 0;
  forward write_before_read: false;
end regfile RegfileName
""",

    "types": """\
// ── Type System ──
// UInt<N>, SInt<N>, Bool, Bit
// Clock<DomainName>, Reset<Sync|Async, High|Low>
// Vec<T, N>, struct StructName / ... / end struct StructName
// enum EnumName / ... / end enum EnumName

// ── Width rules ──
// UInt<8> + UInt<8> → UInt<9>   (result widens by 1)
// No implicit conversions — use .trunc<N>(), .zext<N>(), .sext<N>()
// Bit slice: x.trunc<Hi, Lo>()  e.g. x.trunc<7,4>() = x[7:4]
// Cast: (x as SInt<32>), (x as UInt<32>)

// ── Naming conventions (compiler-enforced) ──
// Modules/structs/enums: PascalCase
// Signals/ports/regs:    snake_case
// Params/constants:      UPPER_SNAKE
""",
}


# ── Tools ────────────────────────────────────────────────────────────────

@mcp.tool()
def get_construct_syntax(construct: str) -> str:
    """Get the ARCH syntax for a specific construct. Call this BEFORE writing
    any .arch code to avoid common mistakes.

    Available constructs: module, inst, fsm, pipeline, synchronizer, fifo,
    ram, arbiter, regfile, types

    Also returns reserved keywords to avoid as signal/register names."""
    key = construct.lower().strip()
    syntax = CONSTRUCT_SYNTAX.get(key)
    if syntax is None:
        available = ", ".join(sorted(CONSTRUCT_SYNTAX.keys()))
        return f"[ERROR] Unknown construct '{construct}'. Available: {available}"

    result = f"--- ARCH syntax: {construct} ---\n{syntax}\n"
    result += f"--- Reserved keywords (do NOT use as signal names) ---\n"
    result += ", ".join(sorted(RESERVED_KEYWORDS))
    return result


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
def arch_build_and_lint(files: list[str], top_module: str, output: str | None = None) -> str:
    """Compile .arch files to SystemVerilog AND run Verilator lint in one step.
    Returns both compiler output and lint results."""
    paths = [str(_resolve_safe(f)) for f in files]
    cmd = [ARCH_BIN, "build"] + paths

    sv_path = _resolve_safe(output) if output else pathlib.Path(paths[-1]).with_suffix(".sv")
    if output:
        cmd += ["-o", str(sv_path)]

    result = _run(cmd)
    if not result.startswith("[OK]"):
        return result

    # Run Verilator lint
    lint_result = _run(
        [VERILATOR_BIN, "--lint-only", "-Wno-DECLFILENAME", "-Wno-UNUSEDSIGNAL",
         str(sv_path), "--top-module", top_module],
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
