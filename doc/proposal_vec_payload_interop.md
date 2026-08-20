# Design note: Vec-field TLM payload interop (ARCH ↔ HARC)

*Status: decision requested — no code changes proposed in this note.
Commissioned by maintainer triage of arch-com#500, Gap 3
(2026-07-13): "design note BEFORE any code (flat-pack vs
std::array-carrier mirror, bit-exact analysis of today's layouts vs
harc spec.md §1.2) — will arrive as an awaiting-decision PR."*

*Author: characterization pass, 2026-08-01. All claims below are
either a direct source citation (file:line) or the output of a
command actually run against a freshly built `target/release/arch`
from this worktree and, where noted, a real SV simulator (iverilog
2012, Verilator 5.048) — not inferred from reading code.*

## 0. tl;dr

The bit-layout "conflict" this note was commissioned to resolve does
**not exist today** — I built the exact struct shape from harc-com's
own worked example, ran it through `arch build`, and fed the
generated SV to both iverilog and Verilator. The bit ranges match
harc-com spec.md's worked example digit-for-digit. This is not a
coincidence: harc-com's convention text is explicitly *derived from*
ARCH's SV emission (spec.md: "matching ARCH SV emission as
`logic [N-1:0]`"). See §2 for the experiment.

What *is* missing, confirmed by experiment:

1. **No sim-side wide/flat carrier.** ARCH's native C++ sim represents
   a `Vec<T,N>` struct field as a plain `T[N]` array; there is no
   ARCH-runtime analogue of harc-com's `harc_wide_write_bits()` /
   `harc_wide_clear_bit()` pack/unpack pair. Nothing in-tree calls for
   one today. (§2.2)
2. **A live, reproducible bug**, discovered during characterization
   and independent of any layout decision: `arch sim --pybind` fails
   to *compile* for any struct containing a `Vec<T,N>` field, because
   the generated pybind11 binding does
   `.def_readwrite("data", &S::data)` on a raw C array member, and
   raw C arrays aren't assignable in C++ — pybind11 rejects it at
   compile time. Reproduced on two independent fixtures (§2.3). This
   also falsifies a claim in the existing spec text
   (`ARCH_HDL_Specification.md:324`: "per-field access through pybind
   is by name, so the memory layout is not observable to
   testbenches" — true for scalar fields, false for `Vec` fields,
   which don't compile at all).
3. **No cross-repo cross-reference or regression test** pinning the
   already-correct SV-boundary layout, so a future codegen refactor
   could silently break the cross-repo contract with nothing catching
   it until a HARC pairing fixture fails.

Recommendation (§6): adopt Option A/C (§3, §5) now — document the
already-verified layout explicitly in both specs, add a golden-SV
regression test, close the doc gap. Defer Option B (§4, sim-side wide
carrier) until a concrete consumer exists — nothing in either repo
needs it today, and building it speculatively adds an unused surface.
File the pybind bug separately; it blocks nothing about this decision
but does block any `--pybind --test` cocotb-style TB that touches a
Vec-field struct, TLM or not.

## 1. Problem statement

Both ARCH and HARC support `struct` types containing fixed-size
`Vec<T,N>` fields, and both use them for TLM (`tlm_method`) burst-style
response payloads — the recommended v1 pattern for "data plus status"
returns per `doc/ARCH_HDL_Specification.md:5587`. HARC's
interop-ABI text (harc-com `spec.md`, "HARC/ARCH Interop ABI"
subsection of "## 2. Relationship to ARCH" — see the numbering note
in §1.1 below) states an explicit bit-packing convention for this
shape: struct fields pack MSB-first in declaration order, and within
a `Vec<T,N>` field, `vec[0]` occupies the field's own LSB slot and
`vec[N-1]` occupies its own MSB slot. ARCH's spec states the
struct-field half of this (`ARCH_HDL_Specification.md:324`) but never
states the Vec-sub-layout half, and neither spec states that the two
have been checked against each other.

Issue arch-com#500 Gap 3 frames the risk: "ARCH-sim handles structs
containing `Vec<T,N>` fields, but there is no documented
carrier-promotion / packing convention mirroring harc-com spec.md
§1.2 ... ARCH has neither the sim runtime carrier nor an SV-boundary
honor of the convention." The second half of that sentence is the
one this note had to actually check rather than assume.

**Where it bites**, concretely:

- **Pairing debug.** A HARC testbench driving an ARCH TLM target
  through a Vec-field response (or vice versa, once HARC-side DUTs
  can be ARCH TLM initiators) needs the two runtimes to agree
  bit-for-bit on which element landed in which slot, or a mismatch
  looks like a logic bug when it's actually a convention mismatch.
- **Mixed ARCH/HARC testbenches**, e.g. a HARC `tseq` driving an
  ARCH-compiled DUT (harc-com spec.md §2, "DUT backend" — "ARCH-compiled
  DUTs are the primary path — same IR, same compiler pass, no
  marshaling"): any place a raw bit-vector literal or a cross-language
  wide-value comparison touches a Vec-field record depends on this
  convention being settled and enforced, not just usually-true.
- **Cocotb-style ARCH-native testbenches** (`arch sim --pybind
  --test`, `doc/arch_sim_cocotb.md`) — this is where the new pybind
  finding in §2.3 actually breaks builds today, independent of the
  cross-repo question.

### 1.1 A numbering correction

The issue and the commissioning brief both cite "harc-com spec.md
§1.2." I fetched the live document
(`git -C harc-com show origin/main:spec.md`, 3023 lines) and there is
no numbered §1.2 — the relevant text is an unnumbered bold-run-in
subsection, **"HARC/ARCH Interop ABI,"** inside **"## 2. Relationship
to ARCH"** (spec.md lines 104–153). The worked example on lines
134–153 is the literal `BurstResp` / `data: Vec<uint<32>,4>` shape
this note reuses in §2. Not a substantive issue — just flagging so a
future reader searching for "§1.2" in harc-com doesn't waste time.

## 2. Today's actual layouts (characterized, not assumed)

### 2.1 SV boundary (`arch build`)

**Struct emission** (`src/codegen/mod.rs:1396–1410`, `emit_struct`):
fields are emitted into a plain SV `typedef struct packed { ... }` in
declaration order and SV's own struct-packed semantics do the layout
— ARCH doesn't compute offsets itself. The function's own comment:

> Canonical ARCH packed-struct bit layout: first-declared field = MSB,
> last-declared field = LSB — matching SV's `struct packed` convention

**`Vec<T,N>` field emission** (`src/codegen/mod.rs:6689–6731`,
`emit_type_and_array_suffix`): emits as an SV packed multi-dimension
array, `logic [N-1:0][W-1:0]` — again, standard SV packed-array
syntax, not custom bit math. Per IEEE 1800, a packed array declared
`[N-1:0]` puts index `N-1` at the array's own MSB and index `0` at
its own LSB.

**TLM `rsp_data` ports use this same path, unmodified.** The TLM
return type is pushed straight through as the port type — no separate
flatten/pack step exists for TLM args or returns:

```
// src/resolve.rs:267
result.push((format!("{name}_rsp_data"), Direction::In, ret_ty.clone()));
```

I confirmed this empirically, not just by reading the code: I built
the same struct (`BoundedVecResp32x4 { data: Vec<UInt<32>,4>; len:
UInt<3>; resp: UInt<2>; }`) once behind a `tlm_method` return
(`tests/axi_dma_tlm/TlmIndexedBurstTarget.arch`, an existing in-tree
fixture — no new struct shape invented) and once behind a plain `out`
port (fixture below), and diffed the generated SV:

```
$ arch build TlmIndexedBurstTarget.arch   # existing fixture, unmodified
typedef struct packed {
  logic [3:0] [31:0] data;
  logic [2:0] len;
  logic [1:0] resp;
} BoundedVecResp32x4;
...
output BoundedVecResp32x4 s_read_burst_rsp_data
```

```arch
// PlainPortVecStruct.arch — characterization-only, not committed
struct BoundedVecResp32x4
  data: Vec<UInt<32>, 4>;
  len: UInt<3>;
  resp: UInt<2>;
end struct BoundedVecResp32x4

module PlainPortVecStruct
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port sel: in UInt<2>;
  port out_val: out BoundedVecResp32x4;

  reg lane: Vec<BoundedVecResp32x4, 4> reset rst => 0;

  comb
    out_val = lane[sel];
  end comb
end module PlainPortVecStruct
```

```
$ arch build PlainPortVecStruct.arch
typedef struct packed {
  logic [3:0] [31:0] data;
  logic [2:0] len;
  logic [1:0] resp;
} BoundedVecResp32x4;
...
output BoundedVecResp32x4 out_val
```

Identical `typedef`, identical port-type usage. TLM lowering does not
introduce a divergent packing path.

**Bit-exact verification against a real SV elaborator** — this is the
one claim in this note I did not want to leave as "should be true per
the SV standard." I fed the ARCH-emitted typedef into a probe module
and ran it through both iverilog and Verilator:

```systemverilog
// layout_probe.sv — characterization-only, not committed
typedef struct packed {
  logic [3:0] [31:0] data;
  logic [2:0] len;
  logic [1:0] resp;
} BoundedVecResp32x4;

module layout_probe;
  BoundedVecResp32x4 x;
  logic [132:0] flat;
  initial begin
    x.data[0] = 32'h1111_1111;
    x.data[1] = 32'h2222_2222;
    x.data[2] = 32'h3333_3333;
    x.data[3] = 32'h4444_4444;
    x.len     = 3'h5;
    x.resp    = 2'h1;
    flat = x;
    $display("FULL_FLAT=%h", flat);
    $finish;
  end
endmodule
```

```
$ iverilog -g2012 -o p.vvp layout_probe.sv && vvp p.vvp
FULL_FLAT=0888888886666666644444444222222235

$ verilator --binary layout_probe.sv --top-module layout_probe && ./obj_dir/Vlayout_probe
FULL_FLAT=0888888886666666644444444222222235
```

Both simulators agree. Decoded: `flat[132:101]=data[3]=0x44444444`,
`flat[100:69]=data[2]=0x33333333`, `flat[68:37]=data[1]=0x22222222`,
`flat[36:5]=data[0]=0x11111111`, `flat[4:2]=len=5`, `flat[1:0]=resp=1`.

**This is digit-for-digit the harc-com spec.md worked example**
(spec.md:147–152, reproduced here for comparison — same struct shape,
`BurstResp` vs `BoundedVecResp32x4`, same field types):

```
bits [132:101] data[3]
bits [100:69]  data[2]
bits [68:37]   data[1]
bits [36:5]    data[0]
bits [4:2]     len
bits [1:0]     resp
```

No divergence, checked with two independent SV toolchains against
ARCH's actual (not hypothetical) codegen output.

### 2.2 Native ARCH sim (`arch sim`, C++ backend)

`src/sim_codegen/mod.rs:11083–11119` emits one plain C++ `struct` per
ARCH `struct` declaration, fields in declaration order, via
`cpp_field_decl` (`src/sim_codegen/mod.rs:2422–2428`): a `Vec<T,N>`
field becomes a raw C array `T name[N]`
(`vec_array_info_with_params`), addressed by the same index the ARCH
source uses (`data[0]` in ARCH is `data[0]` in the emitted C++ —
there's no reversal, no packing, just an array).

Confirmed from an actual `arch sim` build of the same
`TlmIndexedBurstTarget.arch` fixture (`arch sim ... --tb
tb_tlm_indexed_burst_target.cpp` — the existing paired testbench,
unmodified, **PASS**):

```cpp
// simbuild/VStructs.h (generated)
struct BoundedVecResp32x4 {
  uint32_t data[4];
  uint8_t len;
  uint8_t resp;
  ...
};
```

```cpp
// simbuild/VTlmIndexedBurstTarget.h (generated) — TLM rsp_data port
BoundedVecResp32x4 s_read_burst_rsp_data;
```

**No wide/flat carrier exists anywhere in this path.** I grepped the
full generated build output for anything resembling HARC's
`harc_wide_write_bits` / `harc_wide_clear_bit` pack-unpack pair
(`runtime/harc_thread_rt.h` in harc-com, lines 261–283) — nothing.
Every struct-with-Vec value in ARCH sim, TLM or not, lives its whole
life as a field-addressed C++ object; it is never treated as one
scalar bit-vector. `src/sim_codegen/mod.rs:11093–11100` already
carries a maintainer comment flagging exactly this as a latent trap
for any future code path that *would* need to serialize the whole
struct to one integer:

> ⚠ Future maintainers: on a little-endian host ... a `memcpy`/
> `reinterpret_cast` of this C++ struct into a wide integer puts the
> FIRST field at the LSBs — the OPPOSITE of ARCH's canonical bit
> layout. If you add a code path that serializes a whole struct to a
> single integer ... you MUST explicitly concatenate `first_field →
> MSB, last_field → LSB` — do NOT rely on `memcpy` or
> `reinterpret_cast`.

That comment is currently correct as a warning about a hazard with no
live exploiter — nothing in-tree does this serialization today. Gap
3's "ARCH has neither the sim runtime carrier" claim is accurate for
this half.

### 2.3 New finding: `arch sim --pybind` does not compile for Vec-field structs

Not anticipated by the issue text, found while generating the C++ for
§2.2. `src/sim_codegen/mod.rs:690–716` emits one `py::class_` per
struct with `.def_readwrite("<field>", &S::<field>)` per field,
unconditionally — no special case for array-typed (`Vec`) fields.
Reproduced on two independent fixtures, TLM and non-TLM:

```
$ arch sim TlmIndexedBurstTarget.arch --pybind
...
error: array type 'unsigned int[4]' is not assignable
  ... property_cpp_function<...>::write<unsigned int (BoundedVecResp32x4::*)[4], 0> ...
  .def_readwrite("data", &BoundedVecResp32x4::data)
Pybind11 link failed for VTlmIndexedBurstTarget_pybind

$ arch sim PlainPortVecStruct.arch --pybind
... identical error, same root cause, no TLM involved ...
```

Raw C arrays are not assignable in C++, so pybind11's
`def_readwrite` — which generates a `c.*pm = value;` setter — fails
to instantiate at compile time. This is orthogonal to the packing
question (§3–§5): whichever option is chosen, `arch sim --pybind`
still won't build for *any* struct with a `Vec` field, TLM payload or
not, until this is fixed separately. It also directly contradicts
`ARCH_HDL_Specification.md:324`'s claim that "per-field access through
pybind is by name" for struct fields — true for scalar fields, false
(does not compile) for `Vec` fields. `gh issue list --search
"pybind array"` / `"def_readwrite"` turned up nothing — not
currently tracked.

**This note does not fix it** (memo-only per scope), but flags it for
separate filing since it's a live, reproducible break independent of
this decision.

## 3. Option A — flat-pack at the SV boundary (formalize the formula)

Define the packing formula explicitly, rather than leaving it as an
emergent property of "SV struct-packed plus SV packed-array
semantics happen to compose this way":

> For struct `S` with fields `f_1 .. f_k` in declaration order, field
> `f_1` occupies the highest bits and `f_k` the lowest — i.e.
> `offset(f_i) = Σ_{j=i+1}^{k} width(f_j)`. For a `Vec<T,N>` field
> occupying bit range `[hi:lo]` (width `N·W`), element `i` occupies
> `[lo + (i+1)·W − 1 : lo + i·W]` — element `0` at the field's own
> LSB, element `N−1` at the field's own MSB.

**Worked example** (from §2.1, same numbers): `BoundedVecResp32x4 {
data: Vec<UInt<32>,4>; len: UInt<3>; resp: UInt<2>; }`, total width
133 bits. `data` (declared first) occupies `[132:5]`; within that,
`data[3]` (element `N-1`) is `[132:101]`, down to `data[0]` (element
`0`) at `[36:5]`; `len` occupies `[4:2]`; `resp` occupies `[1:0]`.

**On the "potential conflict"**: investigated and **not found**.
"ARCH's declaration-order-MSB-first struct rule" and "HARC's
vec[0]=LSB rule" read like they could clash (a naive reading: "first
declared = MSB" vs "index 0 = least significant" sound like opposite
orientations), but they compose without contradiction because they
apply at different levels: the struct rule orders *fields*
(`f_1` MSB → `f_k` LSB), and the Vec rule orders *elements within one
field's own sub-range* (index `N-1` at that sub-range's MSB → index
`0` at its LSB). Nothing about "first field is MSB" says anything
about how a *single* field's own bits are internally ordered — that's
governed entirely by whatever type that field has, and for `Vec<T,N>`
declared `[N-1:0]`, IEEE 1800 packed-array semantics already put
index `N-1` at that field's local MSB. The two rules were never
actually in tension; the appearance of conflict was reading "MSB
first" as a single global statement instead of a recursive one
(applies at every nesting level, struct-of-Vec-of-struct included —
not exercised in this note's experiment but follows from the same
composition and would be worth a fixture if a real user hits it).

Also worth stating plainly: harc-com's convention text was not
independently designed and later found to match — it names its
source. spec.md:120–121: "matching ARCH SV emission as `logic
[N-1:0]`." harc-com wrote the rule by pointing at what ARCH's
codegen produces.

**Cost**: none beyond documentation + a regression test (§6, §7) —
this is what `arch build` already emits, checked in this note against
two independent SV toolchains, not a codegen change.

## 4. Option B — mirror HARC's `std::array` + wide carrier in `arch sim`

Add the sim-runtime half HARC has and ARCH doesn't (§2.2): a
`arch_wide_write_bits()` / `arch_wide_read_bits()` pack/unpack pair
(mirroring `runtime/harc_thread_rt.h:261–283`'s
`harc_wide_write_bits` / `harc_wide_clear_bit`, likely landing beside
`runtime/arch_thread_rt.h` — the existing checked-in sim runtime
header, wired in via `src/sim_codegen/thread_sim.rs:224` /
`src/main.rs:2098–2101`), plus a generated `pack()`/`unpack()` method
per struct (`src/sim_codegen/mod.rs:11083–11119`) using the identical
formula from §3 — same convention, just also implemented, not only
documented, on the sim side. This directly retires the "Future
maintainers" hazard comment at `src/sim_codegen/mod.rs:11093–11100`
by giving it a correct, tested implementation instead of a warning.

If pybind exposure is also touched here, it's the natural place to
fix §2.3's `def_readwrite` break — replace the raw-array binding with
one routed through `pack()`/`unpack()` (or `py::array_t`), which
`arch_sim_cocotb.md`'s cocotb shim would need regardless of whether
that fix ships as part of this option or separately.

**What it's for**: a whole-struct-as-one-scalar view — needed for
things like a DPI-C export of a Vec-field record, a future "struct as
`UInt<N>`" cast, hashing/coverage sampling over a whole transaction,
or a `pybind11 .value`/`__int__` shim. **None of these exist in
ARCH today.** `arch` has no DPI export path, no struct→UInt cast, and
the pybind exposure is per-field (`.def_readwrite`), not a scalar
view, even for structs *without* `Vec` fields.

**Cost**: real implementation surface (§7) — new runtime helpers, a
generated method per struct, a currently-unexercised code path to
keep correct under future struct/Vec grammar changes. Not large, but
not free, and — per the repo's "prefer existing constructs" /
YAGNI lean — it would be built with no consumer, which is exactly the
situation Gap 4 (semantic-trace JSONL) and the refinement-exemplar
item were PARKED for in the same 2026-07-13 triage, rather than built
speculatively.

## 5. Option C — ratify ARCH's existing layout, HARC adapts

This note's characterization (§2.1) revealed that ARCH already has a
de-facto layout, so this option is really "codify what's already
true" rather than "pick a new convention and push the cost onto
harc-com." What would harc-com need to change? **Nothing** — checked
bit-exactly (§2.1) against harc-com's own spec.md text and its own
worked example, both already match. The interesting asymmetry is
*which spec cross-references which*: today ARCH's spec states the
struct-MSB half in isolation (`ARCH_HDL_Specification.md:324`) with
no mention of HARC or of the Vec-sub-layout half; harc-com's spec
states the full rule and explicitly derives it from ARCH's emission.
Option C, concretely, is: ARCH's spec adds the missing Vec-sub-layout
half and an explicit cross-reference to harc-com spec.md's interop-ABI
section, so the dependency is visible from both sides instead of only
one.

Because Option A and Option C converge on identical wording and zero
behavior change once the characterization is done, the only real
three-way choice left is A/C (document only) vs. B (also build the
sim-side carrier) — see §6.

## 6. Compatibility, migration cost, and recommendation

| | Option A/C (document + test) | Option B (+ sim-side carrier) |
|---|---|---|
| Codegen change | None | New runtime helpers + per-struct `pack()`/`unpack()` (`src/sim_codegen/mod.rs`) |
| `.archi` format | Unaffected — confirmed: `.archi` round-trips the full ARCH struct declaration (`data: Vec<UInt<32>, 4>; ...`), not a flattened layout; each backend re-derives bit position from the same source text every build, so there's nothing baked in to migrate. | Same — carrier methods are sim-internal, not part of the `.archi` surface |
| Existing fixtures | Zero risk — no behavior changes, only new assertions on already-produced output | Additive if implemented carefully; risk is scope creep, not breakage |
| harc-com changes | None (§5) | None (§5) |
| New dead/unused surface | None | Yes, until a real consumer exists (§4) |

**Recommendation: adopt Option A/C now.** The layout question this
note was commissioned to answer resolves to "already correct,
verified bit-exact against two SV toolchains and against harc-com's
own worked example" — there is no conflict to design around, so the
lowest-risk, lowest-cost action is to write that down where both
specs can be checked against it, plus a regression test so it stays
true. Defer Option B until something in either repo actually needs a
whole-struct-as-scalar view; building it now would be speculative
infrastructure for zero current callers, which cuts against this
repo's stated preference for composing existing behavior over adding
new surface. Revisit if: a DPI export path is proposed, a
"struct-as-UInt" cast is proposed, or the pybind fix (separately
filed) turns out to need a carrier-shaped solution anyway.

**Separately from this decision**: file §2.3 (the pybind
`def_readwrite` compile break) as its own bug. It's real, reproducible
on a fresh binary, blocks `arch sim --pybind --test` (cocotb-style)
for any Vec-field struct today, and needs fixing regardless of which
option above is chosen.

## 7. If Option A/C is approved: implementation touch list + test plan

**Docs:**
- `doc/ARCH_HDL_Specification.md:322–324` (the `Vec<T,N>` / `struct S`
  table rows) — add the Vec-sub-layout half (`vec[0]`=LSB,
  `vec[N-1]`=MSB within its own field range) and a cross-reference to
  harc-com spec.md's "HARC/ARCH Interop ABI" subsection.
- `doc/ARCH_HDL_Specification.md:5587` (TLM Vec-payload paragraph,
  §22 area) — state the packing convention explicitly at the point
  where Vec-field TLM payloads are introduced, not just "supported."
- `CLAUDE.md` (arch-com root) "TLM Method Support" section — one-line
  pointer to the convention + this note.
- `doc/proposal_arch_harc_tlm_consistency.md` Gap 3 entry — mark
  decided, link this memo and the closing PR.
- arch-com#500 — update Gap 3 status once this memo's PR lands.

**Tests (new, not part of this memo's PR — memo is doc-only per
scope):**
- Golden-SV regression: extend `tests/integration_test.rs` (or a
  fixture-driven test alongside `tests/axi_dma_tlm/`) to run `arch
  build` on `TlmIndexedBurstTarget.arch` (already in-tree, no new
  struct shape needed) and assert on the literal `typedef struct
  packed { logic [3:0] [31:0] data; ... }` text — pins
  `src/codegen/mod.rs:1396–1410` and `:6689–6731` against silent
  regressions that would break the cross-repo contract.
- Bit-layout pinning test: adapt this note's `layout_probe.sv`
  pattern into a checked-in fixture run through Verilator (the repo
  already has an established "Verified end-to-end" pattern for this
  class of claim — see the bounds-checking and div-by-zero sections
  of `CLAUDE.md`) — register it per the `tests/run_fixtures.sh` row
  convention (fixtures need an explicit row; a file alone never runs).
- Native-sim TLM roundtrip: extend
  `tests/axi_dma_tlm/tb_tlm_indexed_burst_target.cpp` (or add a
  sibling) to assert exact `data[]` element values through
  `rsp_data`, not just tag/valid handshake as the current PASS
  criterion does — makes the C++-side per-element behavior in §2.2 an
  explicit regression, not an incidental side effect of the handshake
  test passing.
- File a standalone repro fixture for §2.3 (the pybind compile break)
  so whoever picks up that bug has a minimal reproducer
  (`PlainPortVecStruct.arch` from this note is already minimal and
  TLM-independent).

**If Option B is later greenlit, additionally:**
- `runtime/arch_thread_rt.h` (or a new sibling runtime header) — add
  `arch_wide_write_bits()` / `arch_wide_read_bits()`, mirroring
  `runtime/harc_thread_rt.h:261–283` in harc-com.
- `src/sim_codegen/mod.rs:11083–11119` — emit `pack()`/`unpack()` per
  struct, using the §3 formula.
- `src/sim_codegen/mod.rs:690–716` (pybind struct bindings) — route
  `Vec`-field access through the new accessors instead of
  `def_readwrite` on the raw array (fixes §2.3 as a side effect).
- New test: pack/unpack round-trip for structs with `Vec` fields at
  representative widths (≤128b and >128b, since `>128b` is where
  ARCH's scalar carrier promotes to `VlWide<N>` —
  `src/sim_codegen/mod.rs:2392–2411` — and this option's carrier needs
  to agree with that promotion boundary), cross-checked against the
  Verilator-compiled SV reference the same way §2.1's probe did.
