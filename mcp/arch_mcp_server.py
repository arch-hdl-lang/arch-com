#!/usr/bin/env python3
"""ARCH HDL MCP Server — gives any AI assistant the ability to read the
ARCH language reference and invoke the compiler (check / build / sim).

WORKFLOW: Before writing any .arch code, call get_construct_syntax() or read
the arch://reference-card resource. This avoids common syntax mistakes
(inst port connection syntax, reserved keywords, let requiring initializer, etc.)."""

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
- Bus → use 'bus' for reusable port bundles (NOT manual individual port declarations for standard interfaces like AXI, APB, custom)
- Only use 'module' for pure combinational/registered logic that doesn't fit the above

Common mistakes to avoid:
- inst connections use 'port <- signal' for inputs and 'port -> wire' for outputs (NOT '=' or direct assignment, no 'connect' keyword)
- Hierarchical references (inst_name.port_name) are FORBIDDEN — always connect outputs explicitly
- 'let' declarations REQUIRE an initializer (let x: UInt<8> = expr;)
- Do NOT use reserved keywords as signal/register names (counter, interface, domain, etc.)
- 'in', 'out', 'state' are contextual keywords — safe to use as port/signal names
- All output ports of an inst MUST be explicitly connected via 'port -> wire'
- Use 'elsif' for chained conditionals (NOT 'else if'). 'else' starts a body block; 'elsif' chains.
- Bit-slice syntax: expr[hi:lo] extracts bits (NOT .trunc<Hi,Lo>())
- Bit/byte reverse: expr.reverse(1) for bit-reverse, expr.reverse(8) for byte-reverse (width must be divisible by N)
- Prefer concat {a, b} over bit-by-bit for loops; prefer direct boolean (z = (A == B);) over if/else
- Prefer putting next-value logic directly in seq (if/elsif) instead of splitting into separate comb + seq blocks. Use 'let' for pure combinational expressions that feed into seq. Only use 'wire' + 'comb' when the combinational value drives multiple consumers or output ports.
- In fsm states, do NOT write '-> X when true;' — omit the transition to stay in the current state (implicit hold), or restructure so the last branch uses a real condition
- Do NOT declare 'domain ... end domain' in pure combinational modules — domains are only needed when Clock/Reset ports are used
- SysDomain is built-in — do NOT declare 'domain SysDomain end domain SysDomain'; just use Clock<SysDomain> directly
- Bus signal access uses dot notation (itcm.cmd_valid), NOT underscore (itcm_cmd_valid)
- Bus ports use 'initiator BusName' or 'target BusName' to set the perspective — 'initiator' keeps signal directions as declared in the bus; 'target' flips them (in↔out)
- Use 'default seq on clk rising;' to set the default clock for seq blocks in the scope
- One-line seq requires 'default seq' — without it, 'seq' must have 'on clk rising/falling'
- Use 'package PkgName ... end package PkgName' to group shared enums/structs/functions/domains; import with 'use PkgName;' at file scope
- Domains declared in a package are shared across files via 'use PkgName;'
- 'inside' operator: expr inside {val1, val2, lo..hi} — returns Bool, set membership
- 'for i in {a, b, c}' — compile-time unrolled value-list iteration (inside comb/seq blocks)
- 'unique if' and 'unique match' assert mutual exclusivity to synthesis (parallel mux): use 'unique if sel == 0 ... end if' or 'unique match opcode ... end match'; emits SV 'unique if' / 'unique case'
- .trunc<N>() errors if N >= source width (not truncating); .zext<N>()/.sext<N>() error if N <= source width (not extending)
""",
)


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

# ── Construct syntax snippets ────────────────────────────────────────────

CONSTRUCT_SYNTAX = {
    "module": """\
module ModuleName
  param PARAM_NAME: const = 32;
  port clk:   in Clock<SysDomain>;      // SysDomain is built-in
  port rst:   in Reset<Sync>;
  port a:     in UInt<8>;
  port reg q: out UInt<8> reset rst=0;  // port reg: output + register in one

  default seq on clk rising;             // sets default clock for all seq

  let sum: UInt<9> = a + 1;             // let REQUIRES initializer
  wire w: UInt<8>;                       // wire: driven in comb blocks

  comb w = a;                            // one-line comb (single assignment)

  comb                                   // multi-line comb (multiple assignments or if/else)
    w = a;
    q = w + 1;                           // ERROR: q is reg, can only assign in seq
  end comb

  seq q <= a;                            // one-line seq (uses default clock)

  seq                                    // multi-line seq (omits 'on clk' when default is set)
    if rst
      q <= 0;
    else
      q <= a;
    end if
  end seq

  seq on clk falling                     // explicit clock overrides default
    q <= a;
  end seq

  // Value-list for (compile-time unrolled, each value gets its own block):
  comb
    for i in {0, 3, 7, 15}
      mask[i] = true;
    end for
  end comb

  // inside operator (set membership, returns Bool):
  let is_special: Bool = opcode inside {3, 7, 16..31};

  // unique if — assert mutual exclusivity; synthesis emits parallel mux:
  comb
    unique if sel == 0
      y = a;
    else
      y = b;
    end if
  end comb

  // unique match — assert mutual exclusivity; emits SV unique case:
  comb
    unique match opcode
      0 => result = a;
      1 => result = b;
      _ => result = 0;
    end match
  end comb
end module ModuleName
""",

    "inst": """\
// Instance syntax — use 'port <- signal' for inputs,
//                       'port -> wire' for outputs.
// Hierarchical references (inst_name.port) are FORBIDDEN.
// All output ports MUST be explicitly connected.

  inst my_inst: ChildModule
    param WIDTH = 16;
    clk   <- clk;
    rst   <- rst;
    data_in  <- input_signal;
    data_out -> output_wire;
  end inst my_inst

  // Then use output_wire in comb/seq blocks (NOT my_inst.data_out)
""",

    "fsm": """\
fsm FsmName
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port go:  in Bool;
  port done: out Bool;

  reg cnt: UInt<4> reset rst=0;

  state [Idle, Running, Done]
  default state Idle;                    // reset state
  default seq on clk rising;             // default clock for all seq in states
  default                                // default outputs (overridden per-state)
    comb done = false;
  end default

  state Idle
    -> Running when go;
  end state Idle

  state Running
    comb done = false;                   // override default if needed
    seq
      cnt <= cnt + 1;
    end seq
    -> Done when cnt == 10;
  end state Running

  state Done
    comb done = true;                    // override default
  end state Done                         // no transition = stays in Done
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

    "package": """\
// Package: reusable namespace for enums, structs, functions, params, domains.
// File must be named PkgName.arch; consumer imports with 'use PkgName;'

// BusPkg.arch
package BusPkg
  domain FastClk
    freq_mhz: 500
  end domain FastClk

  enum BusOp
    Read, Write, Idle
  end enum BusOp

  struct BusReq
    op: BusOp;
    addr: UInt<32>;
    data: UInt<32>;
  end struct BusReq

  function max(a: UInt<32>, b: UInt<32>) -> UInt<32>
    return a > b ? a : b;
  end function max
end package BusPkg

// Consumer.arch
use BusPkg;

module Consumer
  port req: in BusReq;
  port addr_out: out UInt<32>;
  comb addr_out = req.addr;
end module Consumer

// SV output:
//   package BusPkg; ... endpackage
//   import BusPkg::*;
//   module Consumer (...); ... endmodule
""",

    "bus": """\
// ── Bus declaration: reusable port bundle ──
bus ItcmIcb
  param ADDR_W: const = 14;
  param DATA_W: const = 32;

  cmd_valid: out Bool;          // direction from initiator's perspective
  cmd_addr:  out UInt<ADDR_W>;
  cmd_ready: in  Bool;
  rsp_valid: in  Bool;
  rsp_data:  in  UInt<DATA_W>;
  rsp_ready: out Bool;
end bus ItcmIcb

// ── Using a bus port ──
module Master
  port clk:  in Clock<SysDomain>;
  port rst:  in Reset<Sync>;
  port itcm: initiator ItcmIcb;                    // directions as declared
  // With param overrides:
  // port axi: initiator AxiLite<ADDR_W=32, DATA_W=64>;

  comb
    itcm.cmd_valid = 1;          // dot notation for signal access
    itcm.cmd_addr  = addr_r;
  end comb
end module Master

module Slave
  port clk:  in Clock<SysDomain>;
  port rst:  in Reset<Sync>;
  port itcm: target ItcmIcb;                       // directions FLIPPED (in↔out)

  comb
    itcm.cmd_ready = 1;          // cmd_ready is output for target
    itcm.rsp_valid = 1;
  end comb
end module Slave

// ── Instance connections: use dot notation on port name ──
// inst m: Master
//   itcm.cmd_valid -> cmd_valid_w;
//   itcm.cmd_ready <- cmd_ready_w;
// end inst m

// ── SV output: flattened to individual ports ──
// module Master (
//   output logic        itcm_cmd_valid,    // {port}_{signal}
//   output logic [13:0] itcm_cmd_addr,
//   input  logic        itcm_cmd_ready,
//   ...
// );
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
// .trunc<N>() requires N < source width (compiler error otherwise)
// .zext<N>()/.sext<N>() require N > source width (compiler error otherwise)
// Bit slice: x[7:4] extracts bits 7 down to 4
// Single bit: x[3] extracts bit 3
// Cast: (x as SInt<32>), (x as UInt<32>)
// Concat: {a, b}   Replication: {4{a}}
// Reduction: &x (AND), |x (OR), ^x (XOR)
// Set membership: expr inside {val1, val2, lo..hi} — returns Bool, emits SV inside
// Ternary: cond ? a : b
// Bit/byte reverse: x.reverse(1) for bit-reverse, x.reverse(8) for byte-reverse

// ── Naming conventions (recommended, NOT compiler-enforced) ──
// Modules/structs/enums: PascalCase (recommended)
// Signals/ports/regs:    snake_case (recommended)
// Params/constants:      UPPER_SNAKE (recommended)
// Module names are emitted as-is in SV — use the exact name the testbench expects
""",
}


# ── Tools ────────────────────────────────────────────────────────────────

@mcp.tool()
def get_construct_syntax(construct: str) -> str:
    """Get the ARCH syntax for a specific construct. Call this BEFORE writing
    any .arch code to avoid common mistakes.

    Available constructs: module, inst, fsm, pipeline, synchronizer, fifo,
    ram, arbiter, regfile, bus, package, types

    Also returns reserved keywords to avoid as signal/register names.
    Note: 'in', 'out', 'state' are contextual — safe to use as port/signal names.
    Module names are NOT required to be PascalCase — use whatever name the target expects."""
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
