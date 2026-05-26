# Proposal: cross-repo TLM consistency (ARCH ↔ HARC)

*Author: scheduled review of 2026-05-25. Status: research note — no code
changes proposed yet. Triggered by the harc-com TLM-pairing work
merged 2026-05-25 (harc-com PRs #288, #292, #295, #298, #299, #302).*

## Context

The last 24 h of harc-com work formalized the HARC-side of the
ARCH ↔ HARC TLM interop story. That work surfaced four places where
the two repos describe (or simulate) TLM semantics differently. None
of them are bugs in either compiler today — every shipping fixture
passes. But each is a latent divergence that will produce surprising
behavior the first time a user crosses the boundary in the
"unsupported" direction. This note collects them in one place so a
maintainer can decide which deserve specs, which deserve compiler
work, and which are intentionally HARC-only forever.

Cross-repo references throughout:

- arch-com `CLAUDE.md` § "TLM Method Support" (the normative one-paragraph
  description of TLM in ARCH)
- arch-com `doc/ARCH_HDL_Specification.md` §22 (TLM pin protocol)
- harc-com `spec.md` §1.2 (interop ABI, added in harc-com PR #302)
- harc-com `runtime/harc_thread_rt.h` (bridging helpers added in
  harc-com PR #298)

## Gap 1 — ARCH RHS-fork cannot mix `blocking` and `out_of_order` calls

**Observed in:** harc-com PR #295. The new pairing fixture
`TlmPairingArchInitiator` issues a tagged-OOO read concurrently with a
blocking call, but cannot place them in the same `fork … and … join`
group on the ARCH side. The HARC fixture works around this by spawning
two ARCH threads that each contain one kind of call.

**ARCH spec status:** silent. The TLM section of `CLAUDE.md` says
"one direct-call `fork … and … join` thread lower to a request
arbiter and response router" without qualifying that all branches of
that fork must use the same method-class.

**Why it matters:** users porting from HARC will eventually try the
mixed form. The current compiler behavior is undocumented (we have
not even confirmed whether it produces a clear error, a wrong
lowering, or a silent miscompile — see "Validation needed" below).

**Proposed action (cheap):** add one sentence to `CLAUDE.md`
TLM section and `doc/ARCH_HDL_Specification.md` §22: "All
direct-call branches of a single `fork … and … join` group must use
the same method-class (all `blocking`, or all `out_of_order`). Mixing
classes within one group is not supported; split into separate
threads when needed."

**Proposed action (correct):** add an elaboration check that rejects
mixed-class RHS-fork groups with a precise diagnostic that points
the user at the split-thread workaround. Mirror the validation style
of arch-com PR #411 (`disallow_nested_control_in_do_until`).

**Validation needed before either action:**
1. Write a minimal ARCH module that puts one `blocking` and one
   `out_of_order tags 2` call in the same fork. Observe what
   `arch check` and `arch build` produce.
2. If the lowering is wrong (not just rejected), this becomes a
   real bug, not a doc gap.

## Gap 2 — TLM tag-width upper bound is unspecified

**Observed in:** harc-com PR #295 and PR #302. The interop ABI in
harc-com spec.md §1.2 defines the `_req_tag` / `_rsp_tag` wire names
but not a min/max width. The fixture uses `tags 2` (2-bit tag → 4
outstanding lanes), but the language permits arbitrary `tags N`.

**ARCH spec status:** §22 defines `out_of_order tags N` syntactically
but does not bound N.

**Why it matters:** today both compilers will accept `tags 64` and
both will produce 6-bit tag wires, but at some N the arbiter and
router lowering become impractical (LUT explosion, sim throughput
collapse). More immediately, the C++ carrier type for the tag wire
needs a documented promotion rule — at what N does the tag stop
fitting in `uint8_t` and require `uint16_t` / `HarcWide`?

**Proposed action:** state a recommended upper bound (suggest
N ≤ 8, i.e. 256 outstanding) in both specs and document the carrier
promotion rule alongside the scalar-width carrier rules already in
harc-com spec.md §1.2.

## Gap 3 — Vec-field TLM payload support is HARC-only

**Observed in:** harc-com PR #298 added `harc_wide_clear_bit()` and
`harc_wide_write_bits()` to `runtime/harc_thread_rt.h`, plus a
`std::array<elem, N>` carrier in records, plus four new burst
pairing fixtures. ARCH has none of this.

**ARCH status:** `CLAUDE.md` TLM section treats request args and
return types as scalar. There is no built-in pattern for emitting a
TLM method whose return type contains a `Vec<T, N>` field.

**Why it matters:** ARCH-authored DMA / burst initiators that want to
return a packed line back to a HARC caller currently have no clean
path. A workaround using N separate `out_of_order` lanes is
expressible but loses the "this is one transaction" framing that
makes TLM useful.

**Proposed action (incremental):** scope a `tlm_method` extension
that allows `-> Record<T>` where `Record` contains fixed `Vec<T, N>`
fields. Mirror the HARC packing/unpacking convention (`vec[0]` = LSBs,
`vec[N-1]` = MSBs, per harc-com spec.md §1.2). Either:
- (a) emit the same `std::array<elem, N>` carrier in the ARCH sim
  runtime, or
- (b) keep ARCH sim flat-packed and only honor the convention at
  the SV-emission boundary (where HARC pairing happens).

Option (b) is smaller. Option (a) is cleaner. Decide which based on
whether ARCH ever needs to interop with itself (i.e. ARCH ↔ ARCH TLM
across module boundaries on a Vec payload).

## Gap 4 — Semantic-trace JSONL events are HARC-only

**Observed in:** harc-com PR #299 added `tlm_call` JSONL events
emitted at request and response phase boundaries. The output is keyed
into `--record-trace` and lives next to the existing waveform.
ARCH `arch sim` has no analogous instrumentation.

**Why it matters today:** none directly — both backends can be
debugged independently with their own waveforms. The gap shows up
the moment someone tries to *correlate* an ARCH-side waveform with
a HARC-side trace to debug a pairing mismatch: HARC tells you
"`m.read(0x40) entered at cycle 100`", ARCH says nothing.

**Proposed action:** defer. This is genuinely v1.x territory. File
as a tracked enhancement, not as something blocking the current
TLM-pairing milestone. When picked up, mirror the JSONL schema from
harc-com PR #299 verbatim so a single downstream tool can ingest
both.

## Synthesis

| Gap | Severity | First action |
|-----|----------|--------------|
| 1: mixed-class RHS-fork | latent miscompile risk | reproduce, then either spec or reject |
| 2: tag-width bound | doc clarity | one paragraph in each spec |
| 3: Vec-field payloads | feature absence | design note, not yet code |
| 4: semantic trace parity | nice-to-have | file as v1.x enhancement |

Gaps 1 and 2 are cheap and should land before the next HARC fixture
batch. Gaps 3 and 4 are real but discretionary — the HARC team
already has working ARCH ↔ HARC fixtures without them, so the
forcing function is "first user complaint", not the current
roadmap.

No language-grammar changes are proposed in this note. If gap 3
goes ahead, that's the only one likely to need new syntax (a
record-typed TLM return), and a separate proposal should cover it.
