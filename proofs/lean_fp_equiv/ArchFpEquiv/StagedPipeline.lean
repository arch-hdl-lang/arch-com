/-
# Retiming meta-lemma for the staged FP operators (Route B)

`arch build --staged-ops` turns a combinational FP operator into a registered
pipeline for higher fmax. The SMT harness
(`tests/fp_v1/smt_proof/staged_ops_miter.sh`) discharges two lemmas about a
concrete staged design:

* **Lemma A (arithmetic):** shorting every pipeline register (`Q := D`) yields a
  combinational transfer function equal to the single-cycle operator's model
  (e.g. `arch_fma_f32`).
* **Lemma B (timing):** the design is a *balanced* feed-forward pipeline — every
  path from a data input to the output crosses the same number `L` of registers.

Neither lemma alone is the equivalence a user cares about, which is a statement
about the *clocked* circuit over time:

  > the output sampled at time `t + L` equals the operator applied to the input
  > presented at time `t`, for every input stream and every initial register
  > state.

This file supplies the missing bridge. It models a synchronous, balanced,
feed-forward pipeline — the canonical shape a design satisfying Lemma B can be
levelled into — as a list of per-stage combinational functions with one register
between stages, and proves the time-domain equivalence by induction on the stage
list. The proof is pure Lean core (no Mathlib): functions, lists, `Nat`
induction.

Composed with the harness: Lemma A gives `comb fs = spec`, Lemma B gives the
latency `L = fs.length` and licenses viewing the netlist as `run`, and
`staged_pipeline_correct` below concludes `output (t + L) = spec (input t)` for
all inputs — the obligation `arch formal` currently defers (`src/formal.rs`).

The state type `σ` is a single wire-bundle type; per-stage combinational logic is
an arbitrary `σ → σ`. Balance is *structural in this model*: prepending a stage
adds exactly one register, so every input reaches the output through exactly
`fs.length` registers by construction. The theorem is therefore the retiming
fact in its canonical (levelled) form; the graph-theoretic statement over
arbitrary per-net register cuts reduces to this one by retiming a balanced DAG to
levels, which Lemma B certifies for the specific design.
-/

namespace ArchFpEquiv.StagedPipeline

variable {σ : Type}

/-- The combinational function of the register-shorted pipeline: apply the stages
in order (stage 1 first). This is exactly what Lemma A's miter checks. -/
def comb : List (σ → σ) → σ → σ
  | [],       x => x
  | (f :: fs), x => comb fs (f x)

/-- One register between the input stream and the rest of the pipeline: at time
`0` the register holds arbitrary power-on content `init`; at time `n+1` it holds
the combinational image `f (xs n)` of the previous cycle's input. -/
def delay (f : σ → σ) (init : σ) (xs : Nat → σ) : Nat → σ
  | 0     => init
  | n + 1 => f (xs n)

/-- Time-domain semantics of the pipeline as an output stream.

`run inits fs xs` is the output stream of the staged pipeline whose stages are
`fs`, fed input stream `xs`, with `inits` supplying the arbitrary initial content
of each stage register (the head of `inits` is the front register; extra/short
`inits` are harmless — see `run_flush`). Each stage is a register followed by its
combinational function, so the pipeline latency is `fs.length`. -/
def run (inits : List σ) : List (σ → σ) → (Nat → σ) → (Nat → σ)
  | [],       xs => xs
  | (f :: fs), xs =>
    let init := inits.headD (xs 0)
    run inits.tail fs (delay f init xs)

/-- **Flush / latency theorem.** For any stage list `fs`, any input stream `xs`,
and *any* initial register contents `inits`, the pipeline output at time
`t + fs.length` is the register-shorted combinational function applied to the
input presented `fs.length` cycles earlier. In particular the output is
independent of the arbitrary power-on state — it has flushed after `fs.length`
cycles. -/
theorem run_flush (inits : List σ) (fs : List (σ → σ)) (xs : Nat → σ) (t : Nat) :
    run inits fs xs (t + fs.length) = comb fs (xs t) := by
  induction fs generalizing inits xs t with
  | nil => simp [run, comb]
  | cons f fs ih =>
    -- length (f :: fs) = fs.length + 1, so t + (fs.length + 1) = (t+1) + fs.length
    have hlen : t + (f :: fs).length = (t + 1) + fs.length := by
      simp [List.length_cons]; omega
    rw [hlen]
    -- unfold one stage: run (f::fs) xs = run tail fs (delay f init xs)
    show run inits.tail fs (delay f (inits.headD (xs 0)) xs) ((t + 1) + fs.length) = _
    rw [ih inits.tail (delay f (inits.headD (xs 0)) xs) (t + 1)]
    -- delay f init xs (t+1) = f (xs t); comb (f::fs) (xs t) = comb fs (f (xs t))
    simp [delay, comb]

/-- **Staged-operator correctness (Route B conclusion).** If the register-shorted
pipeline computes `spec` (Lemma A) then, for a balanced feed-forward pipeline of
latency `L = fs.length` (Lemma B), the clocked output at `t + L` equals
`spec (input t)` for every input stream and every initial register state.

This is the machine-checked form of the "balanced feed-forward ⟹ transfer =
`L`-delayed combinational function" retiming fact that Route A's SMT decomposition
relies on. -/
theorem staged_pipeline_correct
    (inits : List σ) (fs : List (σ → σ)) (spec : σ → σ)
    (lemmaA : ∀ x, comb fs x = spec x)          -- discharged by the SMT miter
    (xs : Nat → σ) (t : Nat) :
    run inits fs xs (t + fs.length) = spec (xs t) := by
  rw [run_flush, lemmaA]

/-- Sanity: the arbitrary initial register state genuinely does not matter — two
runs with different power-on contents agree from cycle `fs.length` onward. -/
theorem run_init_irrelevant
    (i₁ i₂ : List σ) (fs : List (σ → σ)) (xs : Nat → σ) (t : Nat) :
    run i₁ fs xs (t + fs.length) = run i₂ fs xs (t + fs.length) := by
  rw [run_flush, run_flush]

end ArchFpEquiv.StagedPipeline
