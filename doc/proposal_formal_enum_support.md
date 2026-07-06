# Proposal: `enum` type support in `arch formal` (SMT-LIB2 BMC encoder)

*Author: research session, 2026-07-06. Surfaced by a review of open
issues/PRs across arch-com and harc-com looking for a concrete,
non-duplicate enhancement.*

## Problem

`arch formal`'s v1 scope is explicitly scalar-only: "flat module, scalar
types, single clock, no Vec/struct/enum" (`doc/COMPILER_STATUS.md`, arch
formal row). Any port or register typed `struct` or `enum` is rejected
outright by `check_scalar_type` (`src/formal.rs:1379-1380`, reject
message at `src/formal.rs:1773`: `"named type ... (struct / enum /
typedef) is not supported by arch formal v1"`).

This blocks formal verification for exactly the construct where BMC is
most naturally valuable: **`fsm` blocks**. The compiler already:

- auto-selects a minimum-width `UInt` encoding for enum variants
  (`enum_width()` in `src/typecheck.rs:8104`, used pervasively for SV
  codegen), and
- auto-generates SVA properties for FSMs — legal-state, state
  reachability, and transition coverage (`doc/COMPILER_STATUS.md`
  `assert`/`cover` row) — that are checked today only via the
  externally-shelled EBMC/Verilator path, never via `arch formal`'s own
  native BMC, because the FSM's state register is enum-typed and enum
  hits the scalar-only reject.

So the project has already built (a) the enum→width encoding, (b) the
exact FSM properties a BMC engine would want to check, and (c) a working
z3/boolector/bitwuzla BMC backend — but the one connective piece (enum
register/port support in the SMT encoder) is missing, and users must
fall back to an external tool for the single highest-value formal
target in the language.

There's also existing, partially-wired scaffolding for this in
`src/formal.rs` itself:

- `enum_variants: HashMap<String, (u64, u32)>` (`src/formal.rs:1048`)
  is already declared on the encoder context.
- `EnumVariant(en, v)` is already matched in expression lowering
  (`src/formal.rs:2178-2184`) and looks the variant up in that map —
  but the map is never populated. `preprocess()` has a comment
  admitting this directly: *"Collect enum variant values (module-scope
  enums not common; look at top-level ast) ... Populated lazily from
  the symbol table would be ideal; for v1 handle Literal only and let
  the encoder fail on EnumVariant with a clear error."*
  (`src/formal.rs:1326-1328`)

In other words, the hard part (variant → (value, width) lookup, and BV
encoding of enum comparisons) is already designed and half-implemented;
what's missing is (1) populating `enum_variants` from the module's enum
declarations at `preprocess()` time, and (2) removing `Ty::Enum` from
the `check_scalar_type` reject path so enum-typed ports/registers get
declared as BV state of the correct minimum width.

## Why it matters

- FSMs are the paradigm case for bounded model checking (illegal-state
  freedom, reachability, transition-legality) — the properties
  `arch formal` is best suited to prove are the ones it currently can't
  touch natively.
- It removes a real toolchain dependency: today, proving an
  auto-generated FSM property requires installing and invoking EBMC.
  With enum support, `arch formal` can do it directly with the same
  `--solver z3|boolector|bitwuzla` flags used everywhere else.
- It's incremental and low-risk: enums are already represented as flat
  `UInt<enum_width(N)>` at the SV/sim boundary, so no new SMT theory is
  needed — just BV variables and BV constants for variant values,
  reusing machinery the encoder already has for `UInt`.
- It directly extends `doc/COMPILER_STATUS.md`'s own stated "Deferred"
  list for `arch formal` (Vec/struct/enum) rather than inventing new
  scope, and enum is the narrowest, best-motivated slice of that list to
  take first (structs and Vecs need field/element-flattening design;
  enum is "just" a width-known integer with named constants).

## Rough implementation approach

1. **Populate `enum_variants` in `preprocess()`.** Walk the module's
   referenced enum declarations (from the top-level AST / symbol table
   the type checker already resolves against — see how
   `Ty::Enum(ident, enum_width(...))` is constructed in
   `src/typecheck.rs:3615`) and insert `"EnumName::Variant" → (value,
   width)` for every variant, mirroring the same enumeration order/value
   assignment the SV emitter uses so BMC and SV codegen never diverge on
   variant encoding.
2. **Allow `Ty::Enum` through `check_scalar_type`.** Where the function
   currently rejects named types, add an `Ty::Enum(_, width)` arm that
   declares the port/register as a plain unsigned BV of `width` bits
   (same path as `UInt<width>` today) rather than erroring.
3. **Verify `EnumVariant` lowering end-to-end.** The match arm at
   `src/formal.rs:2178` already turns a resolved variant into its `(val,
   w)` pair; confirm it emits a BV constant of the right width and that
   equality/inequality comparisons against enum-typed signals lower the
   same way scalar `UInt` comparisons do.
4. **Test plan**, mirroring the hierarchical-formal precedent in
   `doc/plan_hierarchical_formal.md`:
   - A minimal `fsm` (3–4 states) with a `reset_state`, checked for
     legal-state (`!rst |-> state != <invalid encoding>`) — expect
     PROVED.
   - A deliberately-mutated transition, expect REFUTED with a
     counterexample identifying the illegal transition.
   - A `cover` on a specific state, expect HIT at the correct bound.
   - Solver parity across z3/boolector/bitwuzla, matching the existing
     14-test parity suite for the scalar path.
5. **Docs**: update the `arch formal` row in `doc/COMPILER_STATUS.md` to
   move "enum" out of the "no Vec/struct/enum" deferred list, and note
   struct/Vec remain deferred separately (they need field-flattening /
   element-indexing design that enum doesn't).

## Non-goals for this slice

- `struct` and `Vec` support — different design problem (field/element
  flattening into multiple BV variables or arrays), left for a follow-up
  proposal.
- Enum values used inside Vec/struct — blocked on the above.
- Enums crossing an `inst` hierarchy boundary — should compose naturally
  with the existing hierarchical-formal prefix-mangling once both land,
  but isn't required for the flat-module case this proposal targets.

## Related but distinct issues (checked for overlap; none propose this)

- `arch-com#383` — hierarchical `arch formal` rejects an auto-generated
  thread sub-module due to lock-arbitration wire decls. A hierarchy bug,
  not a type-scope gap; orthogonal to this proposal.
- `arch-com#602` — static FSM unreachable-state detection in
  `arch check` (a compile-time lint, not `arch formal`/BMC).
- `doc/plan_hierarchical_formal.md` — widens formal along the
  *hierarchy* axis (nested insts); this proposal widens it along the
  *type* axis (enum). The two are complementary, not overlapping.
