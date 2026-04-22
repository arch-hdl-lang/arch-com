# Plan: Standard bus package

*Author: session of 2026-04-22. Status: design draft; v1 scope is small.*

## Motivation

AXI4-Stream, AXI4-Lite, AXI4-Full, APB, and Avalon-ST are the bus
shapes that dominate SoC glue code. Every user who tries to write them
by hand runs into the same problems: direction-flip mistakes, forgotten
sideband signals, and LLMs fabricating made-up field names. The
existing `bus` + `handshake` + `generate_if` vocabulary can express
every one of these cleanly — but users shouldn't have to rewrite them
from scratch in every project.

The natural answer is a **standard library**: a set of curated `.arch`
files shipped with the compiler, each defining one canonical bus, that
users import with `use BusX;`. This keeps the language surface unchanged
and pushes the domain knowledge into a library that evolves
independently of the compiler.

This is the "library not language" instinct — the same choice Chisel,
SpinalHDL, and FIRRTL communities eventually landed on for bundled
protocols.

## Principles

1. **No new compiler construct.** Standard buses are `.arch` files that
   use `bus`, `handshake`, and `generate_if` exactly the way user code
   would. If a user reads a stdlib bus file, they learn the canonical
   pattern and can compose their own.
2. **Parameterized, not hardcoded.** Each bus carries the knobs that
   every real design configures: data width, address width, ID width,
   user-signal width, optional feature toggles (e.g., "does this
   AXI-Stream carry `last` + `keep`?"). Users set params at the port
   site, compiler generates the right port set.
3. **Versioned by file, not by release.** Fixing an AXI-Stream bug or
   adding a missing signal is a single PR against one file. No
   compiler work required.
4. **Extensible.** A third party can ship `BusMyCompanyProto.arch`
   the same way; the ARCH compiler treats stdlib and user buses
   identically.

## Package discovery

Today `use X;` resolves `X.arch` by searching:
1. The directory of the current source file
2. `ARCH_LIB_PATH` environment variable (colon-separated paths)

**Extension for stdlib**: the compiler binary itself knows its install
location and adds `<install>/stdlib/` to the search path as a lowest-
priority entry (user code + `ARCH_LIB_PATH` shadow it). A built-in bus
like `BusAxiStream` is then `use BusAxiStream;` with zero setup.

Install-location resolution:
- **Release install**: `<exe>/../stdlib/` where `<exe>` is the directory
  of the `arch` binary — matches Unix convention where `<prefix>/bin/arch`
  and `<prefix>/stdlib/` are siblings. Falls back to `<prefix>/share/arch/stdlib/`
  if the binary is installed to `/usr/local/bin/`.
- **Dev build (`cargo run`)**: walk up from `<exe>` to find a `Cargo.toml`
  and use its `../stdlib/`. Lets `cargo run -- build foo.arch` find the
  stdlib without any env var setup.
- **Override**: `ARCH_STDLIB_PATH` (if set) wins outright.
- **Disable**: `ARCH_NO_STDLIB=1` skips the stdlib search entirely
  (for tests and debugging shadowing).

The resolution order becomes:
1. Same-directory relative path
2. `ARCH_LIB_PATH` entries (user)
3. `<install>/stdlib/` (unless `ARCH_NO_STDLIB=1`)

## Naming convention

Flat names with a `Bus` prefix, e.g., `BusAxiStream`, `BusAxiLite`,
`BusApb`. Matches the existing working convention in the test corpus
(`BusAxiLite` already exists in `tests/axi_dma/`). When/if ARCH gets
module-path namespacing, these can migrate to `Std.AxiStream` etc.
without source churn — just renaming the files.

## v1 contents — ship only these three

Starting narrow on purpose. Adding more is one file each; easy to do
after the first three shake out the structure.

| Bus | Coverage | Key params |
|---|---|---|
| `BusAxiStream`  | AXI4-Stream simple + full | `DATA_W`, `USE_LAST`, `USE_KEEP`, `USE_STRB`, `ID_W`, `DEST_W`, `USER_W` |
| `BusAxiLite`    | AXI4-Lite memory-mapped   | `ADDR_W`, `DATA_W` |
| `BusApb`        | APB3 / APB4               | `ADDR_W`, `DATA_W`, `USE_PPROT`, `USE_PSTRB` |

Explicitly **not** in v1:
- AXI4-Full (ID width, burst, lock, cache, prot, qos, region, response
  channels) — large surface, defer to v2 after the first three prove
  the pattern.
- Avalon-ST, Avalon-MM, Wishbone — demand-driven additions.
- Stateful protocols (credit-based flow control) — belongs in a future
  `credit_channel` construct, not in this package.

## Example: `BusAxiStream` (expected shape)

```
// stdlib/BusAxiStream.arch
bus BusAxiStream
  param DATA_W:  const = 32;
  param ID_W:    const = 0;     // 0 → omit tid
  param DEST_W:  const = 0;     // 0 → omit tdest
  param USER_W:  const = 0;     // 0 → omit tuser
  param USE_LAST: const = 1;    // 0 → omit tlast
  param USE_KEEP: const = 1;    // 0 → omit tkeep
  param USE_STRB: const = 0;    // 0 → omit tstrb

  handshake t: send kind: valid_ready
    data: UInt<DATA_W>;
    generate_if USE_LAST
      last: Bool;
    end generate_if
    generate_if USE_KEEP
      keep: UInt<DATA_W/8>;
    end generate_if
    generate_if USE_STRB
      strb: UInt<DATA_W/8>;
    end generate_if
    generate_if ID_W > 0
      id: UInt<ID_W>;
    end generate_if
    generate_if DEST_W > 0
      dest: UInt<DEST_W>;
    end generate_if
    generate_if USER_W > 0
      user: UInt<USER_W>;
    end generate_if
  end handshake t
end bus BusAxiStream
```

Usage:

```
use BusAxiStream;

module Producer
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port axi: initiator BusAxiStream<DATA_W=32, USE_LAST=1, USE_KEEP=0>;
  ...
end module Producer
```

The port expands to:

```systemverilog
output logic       axi_t_valid,
input  logic       axi_t_ready,
output logic [31:0] axi_t_data,
output logic        axi_t_last
```

Clean, matches industry naming, zero boilerplate at the use site.

## Constraints that surface v2 work

Writing `BusAxiStream` against the current grammar surfaces one real
issue: **`handshake` payload field declarations inside `generate_if`
aren't yet supported**. Today the handshake parser expects a flat list
of `name: type;` fields with no conditional branches. Before v1 ships,
we need to extend the handshake parser to accept `generate_if` inside
a payload list (reusing the existing generate_if machinery).

That's the only compiler-side work required for v1 stdlib. Everything
else is pure data.

## Testing

Each stdlib bus ships with a matching test:

- `tests/stdlib/bus_axi_stream_test.arch` — a Producer + Consumer pair
  using `BusAxiStream`, connected via inst. `arch build` should produce
  Verilator-lint-clean SV; a small TB drives a handful of beats and
  asserts protocol correctness.
- Same pattern for `BusAxiLite` and `BusApb`.

This gives us regression coverage without forcing every test in the
repo to migrate to stdlib names.

## Implementation roadmap

### PR #1 — compiler: stdlib path discovery + handshake-in-generate_if

1. `main.rs resolve_use_imports`: after same-dir + `ARCH_LIB_PATH` lookup,
   consult `<install>/stdlib/`. Respect `ARCH_STDLIB_PATH` override and
   `ARCH_NO_STDLIB=1`.
2. Parser: allow `generate_if COND ... end generate_if` inside
   `handshake` payload blocks, mirroring the existing bus-level
   generate_if. Elaboration already handles the condition evaluation.
3. One integration test confirming stdlib discovery works (a stub
   `.arch` file in `stdlib/` that a module can `use`).

### PR #2 — write the three bus files + tests

1. `stdlib/BusAxiStream.arch`
2. `stdlib/BusAxiLite.arch`
3. `stdlib/BusApb.arch`
4. `tests/stdlib/` Producer/Consumer tests for each.

### PR #3 — docs

Spec mention of stdlib with the contents table and one example;
reference card entry showing `use BusAxiStream;` as the canonical way
to introduce an AXI-Stream port.

## Non-goals

- Not a full AXI4-Full implementation in v1. Defer until the basic
  three shake out the structure.
- Not introducing a namespace / module-path system. Flat `BusX` names
  for now; namespace refactor is a separate language change.
- Not building a formal verification library alongside. Each stdlib
  bus should work with `arch formal` via the normal path (including
  the Tier 2 auto-emitted handshake assertions), but no dedicated
  formal harness ships with the stdlib in v1.

## Open question

Do we want a **versioned** stdlib (`use BusAxiStream@v2;` pinning) or
always-latest? v1 answer: always-latest. If a breaking change to a
stdlib bus ever becomes necessary, we introduce a new name (e.g.,
`BusAxiStreamV2`) rather than invent a versioning syntax.
