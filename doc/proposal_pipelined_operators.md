# Proposal: pipelined arithmetic operators via a verified implementation registry

> **Status: APPROVED 2026-07-12** — design adopted as proposed (maintainer
> sign-off), with the conservative v1 answers to both open questions (explicit
> alignment; `.archpipe` schedule-only). Implementation proceeds per the phase
> plan below; spec sections land with the implementation PRs, not this doc.
>
> **Phases 1–3 DONE (2026-07-12).** `fma<pipelined, 6>` type-checks *and*
> builds/sims end-to-end on both backends — see phase 3 in "Implementation
> phases" below for the clarified comb+retime implementation form and where
> the verification obligation and characterization honesty live. Phases 4–5
> (`.archpipe` loader, generalizing beyond `fma`) are not started.

Status: design proposal / discussion. No implementation in this note. User-facing
syntax + type-system change — requires sign-off before any code, and a spec update
is part of the eventual fix.

## Motivation

ARCH's FP operators (FP32/BF16, #609–#618) are exposed today as *combinational*
expressions: `fma(a,b,c)`, `a * b`, etc. lower to a single combinational cone via
the shared bit-vector IR. That is correct and verified, but it caps the achievable
clock. Physical characterization of the FP32 FMA on Nangate45 (typ., Yosys +
OpenSTA) shows the problem and the opportunity:

| implementation | fmax (Nangate45, typ.) | notes |
|---|---|---|
| exact-wide reference (`arch_fma_f32_ref`) | 45 MHz | 470-bit adder, **does not pipeline** (retiming/hand-split stayed ~39 MHz) |
| sticky-fold, combinational | 102 MHz | bounded GRS datapath |
| sticky-fold, **6-stage pipelined** | 165 MHz raw / **260 MHz buffered** | see depth sweep below |

Buffered depth sweep (Yosys `abc` with `buffer -N 8; upsize; dnsize`, which the
default `abc -liberty` flow omits):

| depth | buffered fmax | DFFs |
|---|---|---|
| 6 | **259.8 MHz** | 1032 |
| 7 | 256.4 MHz | 1221 |
| 10 | 241.5 MHz | 1852 |

So a pipelined FMA reaches ~260 MHz vs. 45 MHz exact-wide — a ~5.8× win — and
**6 stages is the knee** (more stages regress: the residual path is a fine-grained
logic-depth cone the registers can't usefully bisect). But there is no way for a
*user* to ask for the pipelined operator. This proposal adds that surface.

### The trap this proposal must avoid

ARCH already has `pipe_reg<T, N>`, but it is a **delay line** — "a cascade of N
flops" (spec §18a); `q@K` lowers to the K-th shift-register tap. Writing

```arch
pipe_reg acc: fma(a, b, c) stages 6;   // WRONG mental model
```

produces *correct values at 6-cycle latency* but keeps the FMA as one
combinational cone — i.e. 45/102 MHz, **not** 260 MHz. The 6 flops delay the
*result*; they do not split the *operator*. The timing win lives entirely *inside*
the operator, in how its logic is distributed across the register stages. The
surface below is therefore deliberately distinct from a delayed comb result.

## Design overview

Three pieces:

1. **A pipelined-implementation registry** the compiler owns and enforces.
2. **A latency-typed pipelined-operator surface** (`fma<pipelined, N>`), depth declared
   in the call, result tracked by the existing `LatencyAt` machinery.
3. **Extensibility**: users add implementations via an IR-schedule file, gated by
   an equivalence-verification status so custom pipelines cannot quietly break the
   correctness story.

---

## 1. The implementation registry (enforcement)

The set of available pipelined implementations is a first-class table the compiler
owns — not prose in the spec. Key: `(operator, type-profile, stages)`. Value: the
staged implementation + metadata.

Initial contents:

```
operator  profile  stages  status     fmax(ng45,typ)  impl
fma       FP32     6       verified   ~260 MHz         builtin:fma_f32_s6
```

Fields:

- `status` ∈ {`verified`, `unverified`} — see §3.
- `fmax` — characterized, advisory (printed by `arch ops`, used for the
  suboptimal-depth warning).
- `impl` — builtin id or a path to a loaded `.archpipe` file.

### Enforcement points

1. **Type-check resolution.** A pipelined call resolves `(operator, profile, N)`
   against the registry. Miss → hard error that *enumerates what exists*:

   ```
   error: no pipelined implementation of fma<FP32> with 5 stages
     available depths: {6}      (run `arch ops` to list all)
   ```

   This is the enforcement mechanism — a registry lookup, not an honor-system
   spec sentence.

2. **`arch ops` subcommand** (passive discoverability). Lists the registry — and
   nothing more; it does not editorialize about which depth to pick:

   ```
   $ arch ops --pipelined
   operator  profile  depths   status            fmax(ng45,typ)
   fma       FP32     6        verified          ~260 MHz
   ```

3. **The builtin list is documented outside the normative spec.** The spec /
   reference card describe the *mechanism* (the `<pipelined, N>` surface, the
   registry, the enforcement and verification rules) — they do **not** enumerate
   the specific available depths, which churn as implementations are added. The
   concrete contents live in a registry-backed listing (the `arch ops` output, and
   a generated `doc/generated/pipelined_ops.md`), so "what's builtin" can never
   drift from what the compiler accepts, without pinning volatile numbers into the
   normative spec.

### Recommendations route through `arch advise`, not `arch ops`

`arch ops` *lists*; **`arch advise` recommends.** Active guidance — "which depth
should I use," "6 doesn't exist for this profile, what does" — is delivered by the
advisor, both proactively (on query) and reactively (on a mistake), keyed off the
registry:

- **On a no-match error** (`fma<pipelined, 5>` with only `6` registered), the
  diagnostic both enumerates available depths *and* seeds the standard error→fix
  pair so `arch advise` returns the canonical answer ("use `<pipelined, 6>` —
  verified, ~260 MHz knee") the way it does for other compile errors.
- **On a suboptimal-but-valid depth** (a future `stages=3` chosen when `6` is the
  characterized knee), the compiler emits a **warning**, and `arch advise` carries
  the rationale (the depth-vs-fmax data) when asked.
- **On query** ("what pipelined fma depths are available / which is best"),
  `arch advise` answers from the registry + characterization metadata.

This keeps the *tool that lists* (`arch ops`) neutral and the *tool that advises*
(`arch advise`) the single place opinions live — and the opinions stay correct
because both read the same registry.

### "User picks N, compiler warns" interaction

- N **must** be in the table for the profile, else error (point 1 above), with the
  `arch advise` fix-pair seeded.
- If N is in the table but a characterized-better depth also exists, the compiler
  **warns** (it does not error) and `arch advise` supplies the reasoning.

---

## 2. Surface syntax and latency typing

The pipelined variant is selected with an explicit angle-bracket argument list on
the operator — reusing ARCH's existing generic syntax (`UInt<8>`, `Vec<T,N>`,
`pipe_reg<T,N>`). The first argument is the variant token `pipelined` (an existing
reserved keyword, alongside `pipeline` / `pipe_reg`); the second is the **declared
depth**. This is syntactically distinct from a comb `fma`, so the
delay-line trap (§Motivation) is impossible to write by accident:

```arch
port acc: out pipe_reg<FP32, 6> reset rst => 0;
seq on clk rising
  acc@6 <= fma<pipelined, 6>(a, b, c);  // (1) depth 6 is DECLARED in the call;
                                   //     compiler looks up (fma, FP32, 6) in the registry
end seq

let s: FP32 = acc@6;             // (2) consumer reads at latency 6 (LatencyAt)
```

Rules:

- **Depth is declared, not inferred.** The `6` in `fma<pipelined, 6>` is the single
  source of truth for the operator's latency. The compiler uses that literal to
  look up `(fma, FP32, 6)` in the registry; **if there is no match it errors**
  (enumerating available depths — §1). The depth is *not* taken from the
  `pipe_reg`; it is taken from the call.
- **`pipe_reg` latency is checked, not a source.** The result of `fma<pipelined, 6>` is
  a latency-6 value; binding it with `acc@6 <= ...` requires the tap latency (`@6`)
  to **equal** the declared depth. A mismatch is an error:

  ```arch
  acc@6 <= fma<pipelined, 4>(a, b, c);   // error: latency-4 result bound at @6
  ```

  So the `pipe_reg<_,N>` and the `<pipelined, N>` must agree, but the *call* is
  authoritative and the binding is a consistency check.
- **Comb stays the default.** Bare `fma(a, b, c)` is the unchanged combinational
  operator (latency 0). `<pipelined, N>` is the only way to request the retimed variant;
  there is no implicit promotion.
- **Compiler-tracked consumption.** Consumers must read `acc@6`. The checker
  carries a latency on expressions and **rejects latency-mismatched combinations**:

  ```arch
  let bad: FP32 = fadd(acc@6, x);   // error: operands at cycle 6 and cycle 0
  ```

  This catches the "used the pipelined result too early" bug class. Alignment uses
  the existing `LatencyAt` / latency infrastructure (`src/resolve.rs::latency`,
  `ExprKind::LatencyAt`), extended from delay-line taps to operator latency.
- **No silent retiming of arbitrary exprs.** Only registry operators invoked with
  `<pipelined, N>` get the retimed treatment. A plain `acc@6 <= fma(a,b,c)` remains a
  delay line on a comb result and (if we choose) warns: *"comb `fma` delayed 6
  cycles; did you mean `fma<pipelined, 6>`?"*.

---

## 3. Extensibility: user implementations + verification gate

A user implementation is **a stage schedule over the canonical operator IR** — a
map from IR temp → stage index, which is exactly what the staging generator
produces. It fits ARCH's single-IR architecture: the comb operator IR already
exists; the user contributes the *schedule*. Loaded from an `ARCH_LIB_PATH`-style
directory, file `fma_f32_s8.archpipe`:

```
pipelined fma<FP32> stages 8
  source builtin                 # retimes the trusted comb fma IR
  schedule
    stage 0: t0 .. t77
    stage 1: t78 .. t126
    ...
  equiv proof("fma_f32_s8_equiv.lean")    #  | smt | unchecked
end pipelined
```

On startup the compiler loads each `.archpipe` into the registry → usable as
`fma<pipelined, 8>`. (Full-custom *datapath* IR — not just a reschedule of the trusted
comb IR — is a later extension; schedule-over-known-IR is the v1 scope and already
covers the 6/7/10-stage experiments.)

### The verification gate (non-negotiable)

ARCH's FP value proposition is *proven equivalence*. A registry entry therefore
carries a verification `status`, and a custom pipeline is only trusted once it is
shown to compute the same function as the verified comb operator:

- `verified` — the staged IR is proven equivalent to the trusted comb operator
  (sequential equivalence: same function, N-cycle latency). The builtin 6-stage
  qualifies once this proof is wired.
- `unverified` — user IR declared `equiv unchecked`: usable **only** with a
  warning, or behind `--allow-unverified-pipelines`. `arch formal` can discharge
  the obligation (gate-vs-comb sequential equiv) to promote it to `verified`.

This keeps "anyone can add an implementation" from quietly undermining
correctness — the headline result (sticky-fold ≡ exact-wide, #639) must not be
silently voided by a buggy third-party schedule.

The equivalence obligation is mechanical given the architecture: the staged IR and
the comb IR are two renderings of the same operator; a sequential miter (the comb
reference vs. the N-stage netlist, latency-aligned) is the check — the same shape
as the prototype's Verilator equivalence harness, lifted to a proof obligation.

---

## Worked example

```arch
module DotProductStep
  port clk: in Clock<Sys>;
  port rst: in Reset<Sync, High>;
  port a:   in FP32;
  port b:   in FP32;
  port acc_in: in FP32;
  port acc_out: out pipe_reg<FP32, 6>;   // result 6 cycles after inputs

  seq on clk rising
    acc_out@6 <= fma<pipelined, 6>(a, b, acc_in);  // declared depth 6 → (fma, FP32, 6)
                                              //   → builtin:fma_f32_s6
  end seq
end module DotProductStep
```

`arch build` emits the 6-stage retimed datapath; `arch check` enforces that 6 is a
registered depth and that any consumer of `acc_out` reads it at latency 6.

---

## Implementation phases (post-sign-off)

1. **Registry + `arch ops` + enforcement** — table, type-check lookup, enumerated
   error, generated spec section. (No new datapath; wire the existing 6-stage.)
   **DONE.**
2. **`fma<pipelined, N>` surface + latency typing** — parser, latency on exprs, the
   alignment check, codegen binding to `pipe_reg`. **DONE.**
3. **Builtin 6-stage as a `verified` entry** — productize the staging schedule;
   land the sequential-equiv proof obligation that sets `status=verified`.
   **DONE, with a clarified implementation form** (maintainer note,
   2026-07-12): the "6-stage" characterization is the proven combinational
   sticky-fold `fma` cone plus N output register stages, **retimed by
   downstream synthesis** — not a hand-split staged datapath. So
   `builtin:fma_f32_s6` lowers (both `arch build` and `arch sim`) to the comb
   `fma` operator feeding the existing `pipe_reg<T, N>` register cascade; no
   bespoke staged-datapath codegen exists or is needed (`src/pipelined_ops.rs`
   module doc comment). Sequential equivalence to the comb operator holds *by
   construction* (a pure N-cycle delay of an already-verified comb IR node),
   which is the verification obligation this phase closes — locked by a
   randomized lock-step regression test (native-sim ⇄ Verilator-on-emitted-SV,
   `tests/pipelined_fma_lockstep_test.rs`) rather than a separate formal
   equivalence proof (there is no second implementation to prove equivalent
   to the first). The registry's `~260 MHz` fmax figure remains an
   **external** Yosys+OpenSTA+Nangate45 characterization; this repo's
   checked-in synthesis flow (`tests/fp_v1/synth/run_synth.sh --stages N
   MODULE`) cannot reproduce it without a Liberty file and OpenSTA (neither
   available in this repo's sandboxes) — see `tests/fp_v1/synth/README.md`
   "Staged/pipelined operators" for what the checked-in flow *does*
   reproduce (a logic-depth proxy) and why.
4. **`.archpipe` loader + verification gate** — file format, `ARCH_LIB_PATH`
   discovery, `unverified` warning path, `arch formal` promotion. Not started.
5. **Generalize beyond fma** — `mul_pipe`, `add_pipe`; additional characterized
   depths. Not started.

## Open questions

- **Depth declared in the call** (decided). Surface is `fma<pipelined, N>(...)` — the
  depth is an explicit argument the compiler looks up in the registry, erroring on
  no match. Not inferred from the `pipe_reg` (which is only consistency-checked).
- **Recommendation channel** (decided). `arch ops` lists passively; the builtin set
  is documented outside the normative spec (generated registry doc); active
  "which depth" guidance comes from `arch advise`, on query and on error (§1).
- **Variant token spelling** (decided). `<pipelined, N>` — `pipelined` is already a
  reserved keyword (`src/lexer.rs:189`, alongside `pipeline` / `pipe_reg`), whereas
  bare `pipe` is not a word keyword (it lexes as `|`). Reusing `pipelined` keeps the
  pipe-family vocabulary consistent and needs no new reserved word.
- **Mixed-latency expressions** (decided, maintainer sign-off 2026-07-12). Require
  explicit alignment in v1 — no auto-inserted delay lines; latency mismatch is a
  compile error. Revisit auto-alignment later if usage shows real friction.
- **Scope of `.archpipe` v1** (decided, maintainer sign-off 2026-07-12).
  Schedule-over-trusted-IR only — the equivalence obligation stays a pure
  retiming check. Full custom datapath IR is out of scope for v1.

## Non-goals (v1)

- Automatic operator pipelining of arbitrary user expressions (this is operators
  in the registry only).
- Variable/elastic latency, stall/back-pressure on the operator (use the
  `pipeline` construct for hazard logic; this is fixed-latency feed-through).
- Replacing `pipe_reg`'s delay-line semantics (unchanged; this adds a distinct
  retimed-operator path).
