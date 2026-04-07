# Paper Review: arch_paper1.tex (third review)

> Reviewed against compiler (commit 908ff2d+), spec v0.40.0.
> Date: 2026-04-06
> This revision addresses most issues from review 1 and 2.

---

## What's Fixed (good)

- LL(1) grammar now prominently featured in abstract, §2.1, §5.1, and comparison table
- L1D cache case study (§8) with real code stats, verification details, and lessons — far stronger than old RISC-V sketch
- AXI DMA case study (§9) with actual Yosys synthesis results and power analysis
- RDC section added (§3.4) with three violation classes
- FSM example now shows `default` block pattern (Listing 6)
- `Reset<S,P,D?>` type with optional domain in type table
- Email changed to arch.hdl.lang@gmail.com
- VerilogEval density table (Table 8) now uses exact totals in bottom row (3,199 / 4,518)
- RTL++ and SystemC references added to related work
- `arch sim` performance claims now say "same performance class as Verilator" — more honest
- Comparison table adds LL(1) grammar row
- Yosys, OpenSTA, Sky130, PG021 references added

---

## Remaining Inconsistencies

### R1. `generate for` syntax still uses old multi-token form (HIGH)

**Listing 8 (§7):**
```
generate for i in 0..SIZE
  ...
end generate for i
```

**Implementation:** Changed to `generate_for` / `end generate_for` (no trailing variable). This is a key LL(1) change — the paper touts LL(1) but shows pre-LL(1) syntax.

**Fix:**
```
generate_for i in 0..SIZE
  ...
end generate_for
```

Also update the `lstdefinelanguage{Arch}` keyword list (line ~38): add `generate_for`, `generate_if`, `generate_else`; remove bare `generate` from keywords (it's still a token but not used standalone).

---

### R2. §5.2 still says "direct single-pass pipeline" (MEDIUM)

**Paper (§5.2):** "The current compiler is a direct single-pass pipeline: parse → elaborate → type-check → codegen"

**Implementation:** Five phases: Lex → Parse → Elaborate → **Resolve** → Type-Check, then codegen. The Resolve phase (symbol table construction) is a distinct pass.

**Fix:** "The current compiler is a multi-phase pipeline: parse → elaborate → resolve → type-check → codegen, transforming the AST directly to SystemVerilog text without a dedicated intermediate representation."

---

### R3. `assume` claimed as "lexed but skipped" — not actually lexed (LOW)

**Paper (§11, Future Work):** "These keywords are currently lexed but skipped at parse time." — referring to `assert`, `cover`, and `assume`.

**Implementation:** `assert` and `cover` are lexed and skipped. `assume` has no token in the lexer.

**Fix:** "The \texttt{assert} and \texttt{cover} keywords are currently lexed but skipped at parse time; \texttt{assume} is specified but not yet lexed."

---

### R4. L1D case study mentions `generate for` (LOW)

**§8.1:** "generate for: 8-way tag array instantiation"

Should be `generate_for` to match the current syntax.

---

### R5. Compiler pipeline table (Table 3) is missing Resolve phase (LOW)

Table 3 shows: Parse → Elaborate → Type Check → Lower → Verify Emit → SV Emit

The actual pipeline includes **Resolve** (symbol table construction) between Elaborate and Type Check. This phase handles cross-file symbol resolution and is distinct from both elaboration and type checking.

**Fix:** Add a row: "Resolve — Symbol table construction, cross-file name resolution"

---

## New Content Review — Accuracy Check

### L1D Case Study (§8) — Verified Accurate

| Claim | Status |
|-------|--------|
| 1,143 lines across 12 files | ✅ Matches doc/l1d_case_study.md |
| 1,217 lines SV (~6% more concise) | ✅ |
| 8-way, 64 sets × 64B lines = 32 KiB | ✅ |
| 3 FSMs (9-state, 4-state, 4-state) | ✅ |
| 3 RAMs (tag, data, LRU) | ✅ |
| 2 buses (AXI4, CPU) | ✅ |
| 9 testbenches, 1,321 lines | ✅ |
| Tag hit: 10 logic levels (from 14) | ✅ |
| Load hit 3 cycles, miss ~15, dirty eviction ~25 | ✅ |

### AXI DMA Case Study (§9) — Verified Accurate

| Claim | Status |
|-------|--------|
| 1,042 lines across 14 files | ✅ Matches doc/axi_dma_case_study.md |
| 1,176 lines SV (~11% more concise) | ✅ |
| PG021-compatible | ✅ |
| 3 FSMs, 2 FIFOs, 5 buses | ✅ |
| Xilinx: 913 LUTs, 993 FFs | ✅ |
| Sky130: 78,134 µm², 2,017 FFs | ✅ |
| Critical path 4.478 ns, 200 MHz met | ✅ |
| 8 testbenches, 2,075 lines | ✅ |
| Clock gating: 9.73 mW → ~0.02 mW idle | ✅ |

### RDC Section (§3.4) — Matches Spec

Three violation classes match spec §5.4 exactly. Correctly marked as planned (parser accepts, type checker doesn't enforce yet).

---

## Strengthening Suggestions

### S1. The `todo!` example is still just text — show code

§5.1 describes `todo!` but never shows it in a listing. A 4-line example would be compelling:

```
module Cache
  port req: in CacheReq;
  port resp: out CacheResp;
  comb resp = todo!; end comb  // compiles, aborts if simulated
end module Cache
```

This is one of ARCH's most unique features for AI workflows and deserves a code listing.

### S2. No `let` in FSM state bodies shown

The paper's FSM example (Listing 6) uses `comb busy = true; end comb` in states. The newer, cleaner syntax is `let busy = true;` which was just added. This is a nice ergonomic improvement worth showing:

```
state Active
  let busy = true;       // shorthand for comb assignment
  -> Done when count_done;
end state Active
```

### S3. Abstract still leads with LLVM projection

The abstract mentions "future fully native LLVM IR compilation path projected to achieve 15–60× speedup" — this is speculative and dilutes the concrete contributions (LL(1) grammar, type system, case studies with synthesis results). Consider moving to the final sentence or a separate future-work mention.

### S4. `forward` keyword appears in pipeline listing but isn't demonstrated

Listing 4 shows `stall when` and `flush` but not `forward`. The text says "Forwarding muxes, when needed, are expressed as explicit `comb if/else` blocks." This is fine but slightly contradicts having a `forward` keyword — clarify whether `forward` is a directive or just manual comb logic.

### S5. Shift-to-wider-target error is worth mentioning

The compiler now errors on `let wide: UInt<9> = a << 1;` (shifts are non-widening per IEEE §11.6.1). This is a subtle bug that SV allows silently. A one-sentence mention in §3.2 (Bit-Width Safety) would strengthen the "catches bugs SV misses" argument.

### S6. Package-scoped modules — worth a sentence in §11

SV cannot place modules inside packages (flat global namespace only). ARCH plans `inst a: PkgName::Module` with compile-time resolution. This is a concrete improvement over SV worth mentioning in Future Work, even as a single sentence.

---

## Summary

| Category | Count | Severity |
|----------|-------|----------|
| Remaining inconsistencies | 5 (R1-R5) | 1 HIGH, 1 MEDIUM, 3 LOW |
| New content accuracy | All verified | ✅ |
| Strengthening suggestions | 6 (S1-S6) | Nice-to-have |

**The paper is substantially improved.** The two case studies with synthesis data are the strongest addition. The LL(1) grammar discussion is well-integrated. The main remaining issue is **R1** — the `generate for` syntax in Listing 8 directly contradicts the LL(1) claim made elsewhere in the paper. Fix that and R2, and the paper is internally consistent.
