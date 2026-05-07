# Paper Review: arch_paper.tex

> Accuracy review against implemented compiler (v0.40.0, 2026-04-04)

---

## Issues Found

### 1. Bool castability claim is wrong (line 219)

**Paper says:** `Bool — 1 bit — Not castable to UInt`

**Reality:** `Bool` and `UInt<1>` are interchangeable — freely assignable to each other, bitwise ops on 1-bit operands return `Bool`. See COMPILER_STATUS: "Bool and UInt<1> are treated as identical types throughout."

**Fix:** Change to "Alias for UInt<1>" or "Interchangeable with UInt<1>".

---

### 2. `crossing` syntax doesn't exist (lines 244–258)

**Paper says:**
```
crossing sys_to_usb
  from: SysDomain,
  to: UsbDomain,
  sync: two_flop,
  data: sys_data -> usb_data,
end crossing sys_to_usb
```

**Reality:** The `crossing` keyword was never implemented. CDC is handled by the `synchronizer` construct with 5 kinds: `ff`, `gray`, `handshake`, `reset`, `pulse`. The compiler detects CDC violations (cross-domain register reads) and directs the user to use `synchronizer` or async `fifo`.

**Fix:** Replace the example with a `synchronizer` example. Update surrounding prose.

---

### ~~3. Latch prevention claim is overstated (lines 267–269)~~ — VERIFIED CORRECT

**Paper says:** "the compiler verifies that every signal assigned in a comb block is assigned on all control paths. A missing else branch or incomplete match is a compile error."

**Reality:** This IS implemented. A `comb` block with `if` but no `else` produces: `signal 'y' is not assigned on all control paths in comb block (infers a latch); add an 'else' branch or a default assignment`. Paper is accurate.

---

### 4. `cam` and `pqueue` listed as implemented constructs (line 129, §4.5)

**Paper says (abstract):** "Pipeline, FSM, FIFO, arbiter, register file, synchronizer, CAM, and priority queue are language keywords"

**Paper says (§4.5):** Lists `cam` and `pqueue` alongside implemented constructs.

**Reality:** Both are ❌ in COMPILER_STATUS. They're specified but not implemented.

**Fix:** Remove `cam` and `pqueue` from the abstract and §4.5 list, or clearly mark them as "specified, not yet implemented". They already appear correctly in Future Work (§8).

---

### 5. `forward` syntax is wrong (line 299)

**Paper says:** `forward alu_result from Fetch.instr;`

**Reality:** The actual syntax requires a `when` condition: `forward expr from expr when expr;`. Additionally, `forward` has been deprecated in favor of explicit `comb if/else` forwarding muxes (see project_pipeline_critique.md).

**Fix:** Either show the correct 3-argument syntax or replace with the recommended `comb if/else` pattern.

---

### 6. Arbiter policy name is wrong (line 370)

**Paper says:** `weighted_round_robin`

**Reality:** The policy is `weighted<W>` or just `weighted`, not `weighted_round_robin`. Also missing `lru` from the list. Full set: `round_robin`, `priority`, `lru`, `weighted<W>`, or custom via `policy FnName;` + `hook`.

**Fix:** Replace with correct policy names.

---

### 7. RAM multi-variable address mapping claimed as implemented (line 376, §7)

**Paper says (§4.5):** "ram (single-port, simple-dual, true-dual with multi-variable address mapping)"

**Paper says (§7 RISC-V case study):** "The unified register RAM uses multi-variable mapping to assign integer registers and CSRs to non-overlapping address ranges"

**Reality:** COMPILER_STATUS shows `ram (multi-var store) ⚠️ Single store variable only; compiler-managed address layout not implemented`.

**Fix:** Remove "multi-variable address mapping" from §4.5. Mark it as planned in the RISC-V case study or remove that paragraph.

---

### 8. Verify Emit phase claims assert/cover → SVA (line 529)

**Paper says (compiler pipeline table):** "Verify Emit — assert/cover/assume → SVA"

**Reality:** `assert`/`cover` are lexed but silently skipped at parse time. No SVA is emitted.

**Fix:** Remove the "Verify Emit" row or mark it as "Planned".

---

### 9. "All registers require reset values" is overstated (line 542)

**Paper says:** "X-propagation from uninitialized state—all registers require reset values"

**Reality:** `reset none` is supported for registers that intentionally have no reset. The `--check-uninit` simulation flag detects reads of uninitialized regs at runtime. Not all registers require reset values.

**Fix:** Change to "All registers have explicit reset policy (including opt-out via `reset none`); simulation-time detection of uninitialized reads via `--check-uninit`".

---

### 10. AIR intermediate representation doesn't exist (lines 548–550)

**Paper says:** "the compiler lowers Arch source to AIR (Arch Intermediate Representation), a typed, clock-domain-aware dataflow graph. AIR nodes carry domain tags, bit-width annotations, and topological ordering metadata."

**Reality:** There is no AIR. The MVP compiler is a direct single-pass pipeline: **parse → elaborate → typecheck → codegen** (AST directly to SV text). No separate intermediate representation, optimization passes, or multi-target backend infrastructure exists. This is intentional for the MVP — a simple, correct compiler that ships. AIR, CIRCT integration, and multi-target backends are future work.

**Fix:** Remove the AIR paragraph, or clearly frame the entire §5.2–5.3 (IR, toolchain targets) as planned architecture rather than current implementation.

---

### 11. Toolchain targets overstated (lines 554–559)

**Paper says:** Five targets listed — ASIC SV, FPGA SV (with BRAM/DSP insertion), formal verification (SVA + SymbiYosys), arch sim C++ models, and HTML documentation.

**Reality:** Only two are implemented:
- ASIC-style IEEE 1800-2017 SystemVerilog
- `arch sim` C++ models (Verilator-compatible)

FPGA-specific primitives, formal verification scripts, and HTML doc generation are not implemented.

**Fix:** List only the two implemented targets. Move the rest to Future Work.

---

### 12. Type-list iteration in generate doesn't exist (line 680)

**Paper says:** `generate for T in [UInt<8>, UInt<16>]` — "type-list iteration"

**Reality:** `generate for` iterates integer ranges only (`for i in 0..N`). Type-list iteration is not implemented.

**Fix:** Remove the type-list iteration claim, or move to Future Work.

---

### 13. "Native simulation ✓" in comparison table is misleading (line 714)

**Paper says:** Arch has a checkmark for "Native simulation" in the comparison table.

**Reality:** `arch sim` generates C++ models compiled by `g++`, which is Verilator-style compiled simulation. The "native LLVM IR" compilation path is described as future work in §5.2 and §8. The checkmark implies it's already implemented.

**Fix:** Change to "Partial" or add a footnote clarifying it's Verilator-backed C++ model generation, not native LLVM compilation.

---

## Things That Are Accurate

- VerilogEval results: 156/156 solved, 154/156 Verilator-clean, ~29% shorter
- CVDP results: 231 files, 213 check-pass, 133/191 cocotb pass (70%)
- Construct descriptions: pipeline, FSM, FIFO, arbiter, regfile, synchronizer, counter, linklist, bus, template, hook, clkgate
- AI-generatability contract and workflow
- Comparison table (except native sim entry)
- Block structure and universal schema
- Bit-width safety and explicit conversion (trunc/zext/sext)
- Single-driver rule enforcement
- Clock domain tracking and CDC detection across inst boundaries
- Hook and template mechanism descriptions
- Derived param expression fix description
- Parallel simulation determinism argument (structural properties)
- All code examples except the `crossing` and `forward` ones
