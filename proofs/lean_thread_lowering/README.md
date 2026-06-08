# Lean Proof Backends

This Lean project contains proof models for compiler-emitted ARCH certificate
replay files.

## Thread Lowering

The thread-lowering backend machine-checks ARCH thread-to-FSM lowering
certificates.

The first file, `ArchThreadLoweringProof/Simple.lean`, proves a small but useful
theorem:

> if a lowered FSM table is certified to implement each source thread state,
> then the source thread semantics and lowered FSM semantics produce the same
> observable trace for every input stream and every cycle.

`ArchThreadLoweringProof/CountedWait.lean` extends the same theorem shape to
include explicit runtime configuration state (`pc` plus the wait counter) and
`wait N cycle` semantics. It also models `multi_transitions` as a deterministic
guarded dispatch list with a certified fall-through target.

`ArchThreadLoweringProof/FoldedExit.lean` adds an abstract register store and
proves the timing shape used by `fold_wait_until_exit_assignments`: sequential
updates folded into a `wait until` state's exit arm commit on the same edge as
the source thread's post-wait exit update.

The prototype currently covers:

- straight-line states,
- observable per-state actions,
- optional `wait until` guards,
- counted `wait N cycle` states with counter load/decrement/exit behavior,
- single guarded non-dispatch jumps that hold when the guard is false,
- unconditional non-natural jumps, used for fork/join rejoin edges,
- multi-transition dispatch states,
- folded wait-exit sequential updates,
- repeating fall-through to state zero.

That is enough to establish the architecture: Rust lowering should eventually
emit a compact certificate for the actual generated `ThreadFsmState` table, and
Lean should prove that accepting the certificate implies trace equivalence.

## How To Check

Install Lean via `elan`, then run:

```sh
cd proofs/lean_thread_lowering
lake build
```

If `lake` is not on `PATH`, invoke it through elan directly:

```sh
cd proofs/lean_thread_lowering
$HOME/.elan/bin/lake build
```

To emit and replay a compiler-emitted Lean certificate directly:

```sh
arch build Foo.arch --emit-thread-proof-lean=Foo.thread-proof.lean
cd proofs/lean_thread_lowering
lake env lean /path/to/Foo.thread-proof.lean
```

For the compiler to run the replay immediately after emission:

```sh
arch build Foo.arch \
  --check-thread-proof-lean \
  --thread-proof-lean-project=proofs/lean_thread_lowering
```

The compiler resolves `lake` from `PATH`, then `ELAN_HOME/bin/lake`, then
`~/.elan/bin/lake`, so replay can work even when the current shell has not added
elan to `PATH`.

The same Lean thread-lowering proof path is also available from the formal
entry point:

```sh
arch formal Foo.arch --emit-thread-proof-lean=Foo.thread-proof.lean
arch formal Foo.arch \
  --check-thread-proof-lean \
  --thread-proof-lean-project=proofs/lean_thread_lowering
arch formal Foo.arch \
  --check-thread-proof-lean \
  --thread-proof-lean-project=proofs/lean_thread_lowering \
  --thread-proof-only
```

`--check-thread-proof-lean` implies Lean proof emission. If
`--thread-proof-lean-project` is omitted, the compiler uses
`ARCH_THREAD_PROOF_LEAN_PROJECT` when set, otherwise
`proofs/lean_thread_lowering` relative to the current working directory.
`--thread-proof-only` is available on `arch formal` when the desired formal
backend is only the Lean lowering replay; it skips the SMT-LIB2 design-property
backend after the Lean artifact is emitted and checked.

## FIFO And Arbiter Construct Proofs

`ArchConstructProof/Fifo.lean` and `ArchConstructProof/Arbiter.lean` provide
generic proof models for first-class FIFO/LIFO and built-in arbiter constructs.
The compiler emits one Lean instance per supported construct plus generated
equation records for the implementation shape. FIFO certificates instantiate a
parametric proof over arbitrary depth and data width: generated full/empty,
ready/valid, pointer/index, next-pointer, and memory-update equations replay;
pointer indexes stay below `DEPTH`; modulo pointers stay below `2 * DEPTH`;
next-pointer equations preserve that bound; and the FIFO transition refines an
abstract queue step. LIFO records currently cover stack full/empty,
ready/valid, pointer, and memory-update equations. Arbiter records cover the
generated ready-selection equation and round-robin next-pointer equation.

The compiler-side equation source is `construct_formal_ir`: Lean certificates
and SMT-LIB2 sanity checks are emitted from the same typed construct model.
For FIFO/LIFO this includes the full/empty, ready/valid, pointer-index, and
next-pointer equations used by the generated Lean records and by the SMT-LIB2
sanity queries. For `credit_channel`, the same construct model emits Lean and
SMT accounting checks for sender credits, receiver occupancy, `can_send`, and
`valid`; the `arch formal` BMC path consumes the same synthesized
credit/occupancy/pointer equations.

Current construct-proof scope:

- synchronous `fifo` with `OVERFLOW = 0` or no `OVERFLOW` parameter,
- synchronous `kind lifo`,
- `arbiter policy priority`,
- `arbiter policy round_robin`, including non-power-of-two `NUM_REQ`,
- arbiter `latency >= 1`,
- bus-nested `credit_channel` with concrete `DEPTH` and scalar payload type
  `T`.

Async FIFOs and custom/weighted/LRU arbiters are rejected by the construct proof
emitter until their generated equations are wired into dedicated Lean models.
This Lean project already contains reusable library theorems for async FIFO
Gray-code decode/injectivity and abstract pipeline valid/stall/flush behavior;
those files are not yet compiler-emitted per-instance certificates.

To emit and replay Lean construct proofs:

```sh
arch build Foo.arch --emit-construct-proof-lean=Foo.construct-proof.lean
cd proofs/lean_thread_lowering
lake env lean /path/to/Foo.construct-proof.lean
```

For the compiler to run the replay immediately:

```sh
arch build Foo.arch \
  --check-construct-proof-lean \
  --construct-proof-lean-project=proofs/lean_thread_lowering
```

If `--construct-proof-lean-project` is omitted, the compiler uses
`ARCH_CONSTRUCT_PROOF_LEAN_PROJECT`, then `ARCH_THREAD_PROOF_LEAN_PROJECT`, then
`proofs/lean_thread_lowering` relative to the current working directory.

To emit and check the SMT-LIB2 side of the same construct model:

```sh
arch build Foo.arch --emit-construct-proof-smt=Foo.construct-proof.smt2
arch build Foo.arch \
  --check-construct-proof-smt \
  --construct-proof-smt-solver=z3
```

Every SMT query in the certificate is expected to return `unsat`; a `sat` or
`unknown` result fails the check. This is a solver sanity path for the generated
construct equations, separate from `arch formal` module-property BMC.

The JSON sidecar and Python bridge remain useful for debugging certificate
schema changes:

```sh
arch build Foo.arch --emit-thread-proof=Foo.thread-proof.json
python3 proofs/lean_thread_lowering/scripts/cert_to_lean.py \
  Foo.thread-proof.json \
  -o proofs/lean_thread_lowering/ArchThreadLoweringProof/GeneratedFoo.lean
cd proofs/lean_thread_lowering
lake env lean ArchThreadLoweringProof/GeneratedFoo.lean
```

The bridge regression suite can also execute Lean when `lake` is available
through `PATH` or the elan fallback paths: it checks that a matching generated
certificate replays successfully and that an intentionally mismatched source/FSM
dispatch certificate is rejected.

The generated file proves the compiler-recorded lowered FSM table against the
`CountedWait` source model, including repeating versus `thread once`
terminal-hold behavior. FSM targets are emitted from the certificate and Lean
proves they match `sourceNext`; dispatch branch target lists are also an
explicit Lean certificate obligation. Guard/action/update/variable/value names
are interned through per-generated-file symbol tables, so equal source strings
map to equal abstract Lean `Nat`s and distinct source strings cannot collide.
For transition guards, schema v5 certificates and direct Rust-emitted Lean
artifacts use the structured `condition_guard` term as the control-proof
identity, while older JSON certificates fall back to parsed labels in the
Python bridge for compatibility. v5 requires `condition_guard` on every
non-counted transition, so a current source/FSM certificate with the same
display label but different machine-readable guard structure no longer replays.
For structured or parseable dispatch guards, the Python converter also emits small
`GuardExpr.eval` proofs for branch pairs with an obvious contradiction. Boolean
contradictions cover `x` versus `!x`, such as `aw_ready && w_ready` versus
`w_ready && !aw_ready`. Simple Nat comparison contradictions cover loop-style
conditions such as `_t0_loop_cnt_1 < 3.resize(2)` versus
`_t0_loop_cnt_1 >= 3.resize(2)`, proved with Lean's `omega` after expression
simplification. Newer compiler sidecars carry a structured
`condition_guard` term (`atom`, `true`, `false`, `not`, `and`, `or`, `lt`,
`ge`, `eq`, `ne`) derived from the Rust AST. This is still a small untyped
guard subset, not full typed ARCH expression semantics.
The Python bridge rejects non-dispatch target mismatches before Lean
generation. Schema v3+ certificates carry an explicit
`source_next_index`/`source_next_name` for each state; the bridge
requires that source-next target to resolve to the natural next emitted compact
state before using it as the source model's fall-through target. Schema v4+
also carries `source_transitions` separately from lowered `transitions`; the
generated Lean source model is built from the former and the lowered FSM model
from the latter. In the Rust emitter, `source_transitions` are snapshotted
before folded wait-exit assignment optimization and then compacted across
folded states, while lowered `transitions` are read from the post-fold FSM
table. The bridge requires `source_transition_origin: "pre_fold_snapshot"` for
v4+ states so this provenance is checked, not merely documented. State tables
must be contiguous raw FSM tables starting at state 0, with raw state 0 emitted
as the first compact state.
Transition targets must resolve to emitted states after folded-state
compaction; unknown or non-emitted targets are hard certificate errors.
Emitted states are role-checked: non-dispatch states must have exactly one
transition, dispatch states must have at least two, and transition objects must
carry condition, target index, and target name fields. A non-dispatch transition
whose guard is not `always` may target a non-natural compact state; the bridge
models this as a `Control.guarded` state, whose false branch holds the current
state and whose true branch jumps to the recorded target. A non-dispatch
transition whose condition is literal true (`always`, `true`, `1'b1`, or `1`)
and whose target is non-natural is modeled as `Control.jump target`, preserving
fork/join rejoin edges without turning the true literal into an unconstrained
abstract guard.
Counted-wait durations come from the structured `wait_cycles_count` field, not
human-readable labels. The converter also emits per-edge `FoldedExit`
store-effect proofs for folded wait-exit update lists present in the
certificate. Direct assignments are replayed as structured `setVar target
value` store updates. Folded wait-exit updates are accepted only when every
update has a structured assignment representation; unsupported nested
statements are rejected rather than proved through opaque identities.
When a runtime state's direct `seq_assignments` are present, they must cover the
entire `seq_updates` list; partial structured coverage is rejected so generated
action observations cannot silently drop unsupported nested statements. The
direct Rust Lean emitter also turns those structured assignments into concrete
`setVar` update lists and emits per-state `applyUpdates` examples proving the
final store value for every variable written by the action state. For repeated
writes to the same variable in one state, the generated obligation follows the
lowered update order and proves the final write.
Generated certificate proofs use a linear nested case split over state numbers,
so larger real thread tables such as fork/join products replay without the
exponential blow-up of a chained `<;>` tactic.

## Next Extensions

Good next proof increments:

- extend `GuardExpr` comparison support from label-level Nat variables and
  constants to typed ARCH bit-vector expression semantics,
- replace abstract symbol IDs with full ARCH guard and assignment expression
  semantics.
