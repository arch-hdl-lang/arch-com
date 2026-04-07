# Paper Review: arch_paper1.tex (third review)

> Reviewed against compiler (commit 908ff2d+), spec v0.40.0.
> Date: 2026-04-06
> This revision addresses most issues from review 1 and 2.

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


### RDC Section (§3.4) — Matches Spec

Three violation classes match spec §5.4 exactly. Correctly marked as planned (parser accepts, type checker doesn't enforce yet).

---

## Strengthening Suggestions

### S1. The `todo!` example is still just text — show code

§5.1 describes `todo!` but never shows it in a listing.
This is one of ARCH's most unique features for AI workflows and deserves a code listing.
The todo! escape hatch is important because it solves a fundamental problem in AI-assisted hardware design: partial correctness.

Without todo!, an AI generating hardware code faces an all-or-nothing situation — every signal must have a valid driver, every port must be connected, every expression must have the right width. If the AI gets 90% of a module right but is uncertain about one piece of logic, the entire file fails to compile. The AI gets no useful feedback on the 90% it got right.

With todo!:


module Cache
  port req: in CacheReq;
  port resp: out CacheResp;
  port mem_req: out MemReq;

  // AI is confident about this part:
  comb
    mem_req.addr = req.addr;
    mem_req.valid = req.valid;
  end comb

  // AI is unsure about eviction logic:
  comb resp = todo!; end comb
end module Cache
This compiles and type-checks. The compiler verifies the parts the AI got right (widths, types, port connections) and warns about the todo! sites. The AI can then:

Get confirmation that the structure is correct
Fill in todo! sites one at a time
Each intermediate state still compiles
This is the hardware equivalent of red-green-refactor in TDD — skeleton first, logic second. No other HDL has this. In SV, you'd need to wire dummy values (which might silently mask bugs) or leave compilation errors (which block all other checking).

It's especially powerful combined with the tight arch check loop — the AI can iterate many times per second, progressively replacing todo! with real logic, getting compile-time feedback at each step.

That said, the paper does mention todo! in §5.1 — the suggestion S1 in my review was just to add a code listing showing it in action, since it's currently described only in prose. A few lines code example would make the value immediately obvious to a reader.

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

### S7. 2-state simulation limitations need honest disclosure

The paper should:
1. Add a "Limitations of 2-State Simulation" subsection in §6 enumerating residual X sources (unwritten RAM cells, out-of-bounds Vec index, division-by-zero) and their detection status (static / runtime / unhandled).
2. Soften Table 4 row from "2-valued logic throughout" to "2-valued logic + runtime undefined-behavior detection (--check-uninit)" with footnote pointing to the limitations subsection.
3. Soften Table 4 row from "Single-pass evaluation" to "Bounded settle (1–2 passes, statically determined)" — still a major win over SV's unbounded delta cycles, but honest about the implementation.
4. Add to §15 Future Work that `--check-uninit` should grow to cover RAM cells (per-cell valid bitmap), dynamic Vec index bounds checking, and division-by-zero trapping.

**Spec and COMPILER_STATUS already updated to reflect these limitations.**

---

