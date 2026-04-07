# Paper Review: arch_paper_1.tex (second review)

> Reviewed against compiler source (commit 908ff2d+), spec v0.40.0, and benchmark data.
> Date: 2026-04-06

---

## A. Factual Inconsistencies with Current Implementation

### A1. `generate for` / `generate if` syntax is outdated (HIGH)

**Paper (Listing 8, line ~480; §6):** Uses `generate for i in 0..SIZE ... end generate for i`

**Implementation:** The grammar was changed to fused single-token keywords: `generate_for`, `generate_if`, `generate_else`. The closing is `end generate_for` (no trailing variable name). This change was made to achieve LL(1) grammar classification.

**Fix:** Update Listing 8 and all prose references to use `generate_for` / `generate_if` / `generate_else`. Remove trailing variable from `end generate_for`.

---

### A2. FSM example uses one-liner comb syntax (MEDIUM)

**Paper (Listing 6, lines ~327-328):** Each state body uses `comb done = false; end comb` on a single line.

**Implementation:** The comb one-liner was removed. Full multi-line `comb ... end comb` blocks are required. However, `let x = expr;` is now supported inside state bodies as shorthand for comb assignments.

**Fix:** Replace `comb done = false; end comb` with `let done = false;` in all FSM state bodies — this is now the idiomatic way to set a single output in a state.

---

### A3. Compiler pipeline is not "single-pass" (MEDIUM)

**Paper (§5.2):** "The current compiler is a direct single-pass pipeline: parse → elaborate → type-check → codegen"

**Implementation:** The actual pipeline is five stages: **Lex → Parse → Elaborate → Resolve → Type-Check**, followed by codegen. The Resolve phase (symbol table construction) is a distinct pass not mentioned in the paper.

**Fix:** Say "direct multi-phase pipeline: lex → parse → elaborate → resolve → type-check → codegen" or just "a classical multi-pass pipeline with no intermediate representation."

---

### A4. `ports` keyword for RAM port groups not reflected (LOW)

**Paper:** All RAM examples use `port rd ... end port rd` syntax.

**Implementation:** RAM port groups now use `ports` (plural): `ports rd ... end ports rd`. This change was made to eliminate LL(2) ambiguity.

**Fix:** Update any RAM code examples to use `ports`.

---

### A5. `.as_clock<D>()` removed — now `as Clock<D>` (LOW)

**Paper:** Does not appear to reference `.as_clock` directly, but if any examples use it, they need updating.

**Implementation:** The `.as_clock<D>()` method was removed. The standard cast syntax `expr as Clock<Domain>` is now the only way to convert Bool/UInt<1> to Clock.

**Fix:** Ensure any clock casting examples use `as Clock<Domain>`.

---

### A6. LL(1) grammar not mentioned (MEDIUM)

**Paper:** No mention of grammar classification or its AI benefits.

**Implementation:** The grammar is now strictly LL(1) — a deliberate design choice with significant AI code generation benefits. The spec has a new §2.4 documenting this with a token dispatch table, SV contrast, and 5 concrete AI benefits (token efficiency, no syntactic traps, instant error localization, context-free understanding, predictable token budget).

**Fix:** Add a paragraph in §2.2 (Block Structure) or §5 (AI-Generatability) noting the LL(1) property and its implications. This is a significant differentiator from SV's unbounded-lookahead grammar.

---

### A7. `assume` keyword claimed but not implemented (LOW)

**Paper (§8, Future Work):** Lists `assert`, `cover`, and `assume` as verification constructs.

**Implementation:** `assert` and `cover` are lexed and skipped. `assume` has no token in the lexer at all.

**Fix:** Note that `assume` is planned but not yet lexed, or add the token.

---

### A8. Shift-to-wider-target is now a compile error (LOW)

**Paper:** Does not mention shift width rules.

**Implementation:** The compiler now errors when a shift result is assigned to a wider target (e.g., `let wide: UInt<9> = a << 1;`), since shifts are non-widening per IEEE §11.6.1. The spec documents this in §3.2 with a width comparison table.

**Fix:** Consider mentioning in §3.1 (Bit-Width Safety) as an example of ARCH catching subtle IEEE width rules that SV allows silently.

---

## B. Benchmark Numbers — All Verified Accurate

| Claim | Status |
|-------|--------|
| VerilogEval: 156/156 solved | ✅ Matches `doc/VerilogEval_Benchmark.md` |
| VerilogEval: 154/156 Verilator-clean | ✅ 2 dataset bugs documented |
| VerilogEval: 3,199 Arch / 4,518 SV lines (70.8%) | ✅ Exact match |
| CVDP: 231 files, 213 pass check (92%) | ✅ Matches `doc/cvdp_benchmark_log.md` |
| CVDP: 133/191 cocotb pass (70%) | ✅ Matches `tests/cvdp/cocotb_results.log` |
| 18 check failures are multi-file | ✅ Documented in benchmark log |

### B1. Line count table inconsistency (LOW)

**Paper Table 4 (§7.3):** Category totals sum to ~3,500 Arch / ~4,800 SV (ratio ~73%)
**Paper Table 3 (§7.2):** Exact totals are 3,199 / 4,518 (ratio 70.8%)

These are the same benchmark but different numbers. Table 4 uses rounded approximations (~). Not technically wrong, but could confuse a careful reader.

**Fix:** Use consistent numbers, or add a footnote to Table 4 saying figures are rounded.

---

## C. Missing Content — Opportunities to Strengthen

### C1. L1D cache and AXI DMA case studies not mentioned

The paper's only case study (§7) is a RISC-V pipeline *sketch* with no code beyond type declarations. Meanwhile, two fully implemented, tested case studies exist:

- **L1D Cache:** 1,143 ARCH lines, 8-way set-associative, 3 FSMs, 3 RAMs, 2 buses, 9 testbenches, all tests pass. Demonstrates FSM composition, RAM latency modeling, bus abstraction, generate_for.
- **AXI DMA:** 1,042 ARCH lines, PG021-compatible, 3 FSMs, 2 FIFOs, 5 buses, clock gating, synthesized through Yosys (Xilinx + Sky130), 200 MHz timing closure, power analysis with clock gating.

Either would be a far stronger case study than the current sketch. The AXI DMA especially demonstrates synthesis-quality output — a key claim the paper makes but doesn't back with data.

**Fix:** Replace or supplement the RISC-V sketch with the L1D or AXI DMA case study. Include actual ARCH code, generated SV, and test/synthesis results.

---

### C2. No synthesis results

The paper claims "deterministic, lint-clean SystemVerilog" and "synthesis tools receive RTL that passes lint cleanly" but provides zero synthesis data. The AXI DMA was actually synthesized:

- Xilinx 7-series: 913 LUTs, 993 FFs
- Sky130 130nm: 78,134 um², meets timing at 200 MHz
- Power: 9.73 mW idle → 13.38 mW active, clock gating reduces idle to ~0.02 mW

Including even a brief synthesis table would substantially strengthen the "predictable RTL" claim.

---

### C3. No mention of package-scoped modules (SV limitation)

The paper compares ARCH vs SV extensively but misses a notable planned advantage: ARCH will support package-scoped modules (`inst a: PkgName::Module`), while SV forces all modules into a flat global namespace requiring tool-specific library mapping. This is a concrete design improvement worth mentioning in §6.

---

### C4. Iteration latency argument lacks concrete timing data

§5.3 makes a strong argument about iteration latency but gives no actual measurements. Adding concrete numbers would be compelling:

- `arch check` time for a typical module (X ms)
- `arch sim` compile + run time vs Verilator equivalent
- Number of generate-check-correct cycles for a typical VerilogEval problem

Even rough measurements would strengthen what is currently a qualitative argument.

---

### C5. `forward` is in the paper but not used in the pipeline example

**Paper (Listing 4):** Shows `stall when` and `flush` but the pipeline example doesn't demonstrate `forward`. The text says "Forwarding muxes, when needed, are expressed as explicit `comb if/else` blocks" — but this contradicts having a `forward` keyword at all.

**Fix:** Either show a `forward` directive in the pipeline example, or clarify that `forward` is a declarative annotation that the compiler lowers to forwarding muxes (if that's the design), or note it as planned.

---

### C6. Comparison table missing some recent features

The comparison table (Table 5) doesn't reflect:
- **LL(1) grammar** — no other HDL has this; worth a row
- **Package-scoped modules** — planned, but SV's inability to do this is worth noting
- **Shift width checking** — catches bugs SV allows silently
- **`let` in FSM state bodies** — unique ergonomic feature

---

## D. Writing / Presentation Critiques

### D1. Abstract is dense — bury the LLVM projection

The abstract mentions "future fully native LLVM IR compilation path projected to achieve 15–60× speedup" — this is speculative and dilutes the concrete contributions. Move to future work or a footnote.

### D2. §7 case study is the weakest section

The RISC-V pipeline case study shows only struct/enum declarations — no actual pipeline code, no FSM code, no FIFO code, no arbiter code. For a paper whose main contribution is first-class constructs, the case study should *demonstrate* those constructs with real code. The L1D or AXI DMA implementations exist and would serve far better.

### D3. Simulation section over-promises

§5.2 dedicates significant space to "Path to Native Compiled Simulation" with projected speedup tables. This is entirely future work. The actual `arch sim` (compiled C++ models) is a solid contribution on its own — let it stand without the speculative LLVM/SIMD projections that invite skepticism from reviewers.

### D4. Table 5 comparison is generous to ARCH

The comparison table gives ARCH checkmarks for "Compiled simulation" with a footnote. While technically accurate (arch sim generates C++), a reviewer familiar with Verilator might note that Verilator also generates compiled C++ from SV, making this less of a differentiator than it appears. The real differentiator is the *integrated* toolchain (one command, no Makefile), not the compilation model.

### D5. `todo!` is undersold

The `todo!` escape hatch is mentioned briefly but is actually one of the most powerful AI-workflow features. No other HDL has this. It deserves a concrete example showing incremental design:

```
module Cache
  port req: in CacheReq;
  port resp: out CacheResp;
  
  // Step 1: skeleton compiles and type-checks
  comb resp = todo!; end comb
end module Cache
```

This enables the AI workflow where the agent generates structure first, then fills in logic — every intermediate state compiles. Worth expanding.

---

## E. Summary

| Category | Issues |
|----------|--------|
| Factual inconsistencies | 8 (A1-A8) |
| Benchmark numbers | All verified accurate |
| Missing content | 6 opportunities (C1-C6) |
| Writing/presentation | 5 suggestions (D1-D5) |

**Top 3 changes that would most strengthen the paper:**

1. **Replace RISC-V sketch with L1D or AXI DMA case study** (C1) — show real code, real tests, real synthesis results
2. **Add LL(1) grammar discussion** (A6, C6) — unique differentiator, directly supports AI-generatability thesis
3. **Update syntax to match implementation** (A1, A2, A4) — `generate_for`, `let` in states, `ports` for RAM
