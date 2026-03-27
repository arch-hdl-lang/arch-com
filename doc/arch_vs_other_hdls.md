# ARCH vs Other HDLs: Bluespec and TL-Verilog

## Bluespec (BSV / Bsc)

**Model**: Guarded atomic actions (rules). Each rule fires atomically when its guard is true. The compiler schedules rules to avoid conflicts.

| Aspect | Bluespec | ARCH |
|--------|----------|------|
| Abstraction | Rules + guards; compiler finds schedule | Explicit `seq`/`fsm`/`thread`; user chooses abstraction level |
| Concurrency | Implicit — compiler decides which rules fire together | Explicit — `thread` + `fork/join` + `resource/lock` |
| Scheduling | Compiler-generated, can be surprising; `descending_urgency` pragmas to influence | No scheduling problem — user writes what they mean |
| Output quality | Often verbose, hard-to-read Verilog; opaque naming | Deterministic 1:1 readable SV; naming matches source |
| Learning curve | Steep — functional programming + rule semantics + scheduling mental model | Low — reads like structured RTL with optional higher abstractions |
| Composability | Excellent — interfaces + modules + typeclasses | `bus` + `template` + `generate`; less abstract but more predictable |
| Formal | Limited built-in support | `assert`/`cover` planned; SMT-LIB2 backend planned |

**Key difference**: Bluespec makes you think in rules and lets the compiler schedule. ARCH lets you think at whatever level fits — `comb` for combinational, `seq` for registered, `fsm` for state machines, `thread` for sequential protocols. You always know what hardware you're getting.

**Where Bluespec wins**: expressing complex microarchitectures with many interacting rules (e.g., out-of-order processors) can be more concise once you internalize the model. The type system (typeclasses, polymorphism) is richer.

**Where ARCH wins**: predictability. Bluespec's "will my rules fire together?" problem doesn't exist. Output SV is readable and debuggable. Lower barrier to entry for RTL engineers.

## TL-Verilog (Transaction-Level Verilog)

**Model**: Pipeline-centric. Signals carry implicit pipeline stage context via `>>N` notation. Retiming is a first-class concept.

| Aspect | TL-Verilog | ARCH |
|--------|-----------|------|
| Pipeline model | Implicit staging via `>>1`, `>>2` etc.; signals auto-retimed | Explicit `pipeline` with named stages + `pipe_reg` for N-stage delay |
| Retiming | `>>N$signal` — concise inline syntax | `pipe_reg name: signal stages N;` — explicit, named |
| Hazard logic | Manual | Compiler-generated: `stall when`/`flush`/`forward` directives |
| Syntax | Extends SV with `\TLV` blocks; m4 macro preprocessing | Clean standalone syntax; no preprocessor |
| Tooling | Makerchip (web IDE); limited open-source toolchain | Standard CLI; emits plain SV for any tool |
| State machines | Not a focus — manual or behavioral | First-class `fsm` + `thread` with `wait`/`fork`/`lock` |
| Bus protocols | Not a focus | `bus` + TLM methods + `implement` blocks |
| Output | SV through Sandpiper compiler | Plain SV, Verilator-compatible C++, SMT-LIB2 (planned) |
| Scope | Pipelines and transactions | Full SoC: modules, FSMs, FIFOs, RAMs, arbiters, CDC, pipelines, buses |

### Retiming comparison

TL-Verilog's `>>N` and ARCH's `pipe_reg` / `pipeline` cover the same hardware patterns:

```
// TL-Verilog: inline retiming
$result = >>2$operand + >>1$coefficient;

// ARCH: explicit pipe registers
pipe_reg operand_d2: operand stages 2;
pipe_reg coeff_d1:   coefficient stages 1;
let result: UInt<WIDTH> = operand_d2 + coeff_d1;
```

```
// TL-Verilog: pipeline stages (implicit from >> depth)
|pipe
   @1 $pc = ...;
   @2 $instr = imem[>>1$pc];
   @3 $result = alu(>>1$instr);

// ARCH: named pipeline stages with hazard management
pipeline Fetch
  stage S1
    let pc_out = pc;
  end stage S1
  stage S2
    reg instr: UInt<32>;
    // cross-stage ref: S1.pc_out is the retimed value
  end stage S2
  stage S3
    let result = alu(S2.instr);
    stall when hazard;
    forward result to S2.alu_in when bypass_en;
  end stage S3
end pipeline Fetch
```

| Capability | TL-Verilog | ARCH |
|-----------|-----------|------|
| Retime by N cycles | `>>N$signal` | `pipe_reg name: signal stages N;` |
| Named pipeline stages | `@N` (numbered) | `stage Name` (named) |
| Change pipeline depth | Change `>>N` number | Change `stages N` or add/remove stages |
| Stall propagation | Manual | `stall when cond` — compiler generates valid/stall chain |
| Flush | Manual | `flush` directive — compiler generates flush masks |
| Forwarding | Manual | `forward` directive — compiler generates mux |

TL-Verilog is more concise for pure retiming.  ARCH is more explicit about what hardware is generated, and adds hazard logic management that TL-Verilog leaves to the user.

**Where TL-Verilog wins**: pure datapath retiming in fewer characters. The implicit staging model lets you restructure pipeline depth with minimal edits.

**Where ARCH wins**: breadth and hazard management. TL-Verilog doesn't address FSMs, bus protocols, CDC, arbitration, resource locking, or pipeline hazards. ARCH covers the full SoC design space with first-class constructs for each concern.

## Summary Matrix

| Feature | Bluespec | TL-Verilog | ARCH |
|---------|----------|-----------|------|
| Pipeline retiming | Implicit from rules | `>>N` inline | `pipe_reg` + `pipeline` stages |
| Pipeline hazards | Implicit from rule guards | Manual | `stall when`/`flush`/`forward` |
| FSM | Implicit from rules | Manual | `fsm` (explicit) + `thread` (sequential) |
| Bus protocols | Interfaces + typeclasses | — | `bus` + TLM methods + `implement` |
| Concurrency control | Rule scheduling | — | `thread` + `fork/join` + `resource/lock` |
| CDC | Manual | — | `fifo` auto-detects; `synchronizer` construct |
| Arbiter | Manual | — | First-class `arbiter` with policies |
| RAM/ROM | Manual | — | First-class `ram` with kinds + latency |
| Counter | Manual | — | First-class `counter` with modes |
| Output readability | Poor (mangled names) | Moderate | High (1:1 naming) |
| LLM generability | Hard (complex semantics) | Moderate | Designed for it |
| Learning curve | Steep | Moderate | Low |
| Tool dependency | Bsc compiler (open-source) | Sandpiper (proprietary core) | `arch` CLI → standard SV |

## ARCH's positioning

ARCH is not the most abstract (Bluespec) or the most pipeline-focused (TL-Verilog). It occupies a distinct niche:

1. **Right abstraction at each level** — `comb`/`seq` when you want RTL control, `fsm`/`thread` when you want behavioral, `bus`+`implement` when you want protocol-level. You pick per-module, per-block.

2. **Predictable output** — every ARCH construct has a documented, deterministic SV expansion. No scheduling surprises, no opaque naming. The generated SV reads like hand-written code.

3. **LLM-first design** — the `keyword Name ... end keyword Name` grammar, explicit types, no implicit conversions, and named endings make it reliably generatable from natural language. Neither Bluespec nor TL-Verilog was designed for this.

4. **Full SoC coverage** — 15+ first-class constructs covering common microarchitecture building blocks (FIFO, RAM, arbiter, synchronizer, counter, regfile, pipeline, FSM, thread, bus) that Bluespec and TL-Verilog leave to manual coding or libraries.

5. **Synthesizable sequential protocols** — `thread` with `wait until`/`fork`-`join`/`resource`-`lock` provides Bluespec-like behavioral expressiveness for protocol logic, but with explicit, predictable FSM lowering instead of implicit rule scheduling.
