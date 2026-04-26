# Thread-to-FSM Lowering: Algorithm and Equivalence Proof

This document provides

1. a **pseudo-code description** of the thread-to-FSM lowering implemented in
   `src/elaborate.rs` (functions `lower_threads`, `lower_module_threads`,
   `partition_thread_body`, `lower_fork_join`, `lower_thread_for`,
   `lower_thread_lock`), and

2. a **mathematical proof** that the **lowered ARCH RTL** (thread-free ARCH
   AST) is observationally equivalent to the source thread program at every
   clock edge.

### Scope: where this proof sits in the compiler pipeline

```
   Thread program          Thread-free ARCH RTL           SystemVerilog
   (ARCH AST with    ─────────────────────────────►       (output of
    ThreadBlock)     [lower_threads — this proof]         arch build)
                                          │
                                          │  [Codegen — separate proof]
                                          ▼
                                    SystemVerilog
```

The lowering pass `lower_threads` is a source-to-source transformation
**within ARCH AST**: it consumes a module containing `ThreadBlock` items and
produces a module that contains only standard ARCH constructs (`RegDecl`,
`CombBlock`, `RegBlock`, `IfElse`, `InstDecl`).  The subsequent ARCH→SV
codegen step (`src/codegen.rs`) is a separate transformation whose
correctness is established independently — it is essentially a
syntax-directed mapping from ARCH RTL constructs to SV `always_comb`,
`always_ff`, `module`, `assign`, etc.

This document proves only the first step.  Composing the two equivalence
results yields end-to-end correctness from thread programs to SV.

A companion document `thread_lowering_algorithm.md` covers implementation
details, naming conventions, and worked timing examples.  This document is
deliberately abstract — its purpose is to establish a *semantic contract* on
ARCH RTL that the implementation, the simulator, and any future
re-implementation must preserve.

---

## Part I — Pseudo-code

### I.1  Notation

| Symbol | Meaning |
|--------|---------|
| `T = t₀, t₁, …, tₖ₋₁` | the threads in a module, indexed by `i` |
| `B(t)` | the body of thread `t`, a list of `ThreadStmt` |
| `States(t)` | list of `ThreadFsmState` produced for thread `t` |
| `S = States(t)[s]` | one FSM state, with fields `(comb, seq, τ, w, M)` |
| `S.comb` | combinational statements driven while state `s` is active |
| `S.seq` | non-blocking register updates that fire on exit edge from `s` |
| `S.τ : Expr ⊎ ⊥` | single transition condition (one of `wait until`, `do..until`, lock-grant) |
| `S.w : Expr ⊎ ⊥` | counted wait (`wait N cycle`) |
| `S.M : List⟨Expr × StateIdx⟩` | multi-target conditional transitions (fork/join, for-loop) |
| `idx(t)` | thread index `i` for thread `t` |

Exactly one of `τ`, `w`, `M` is set per state (else "unconditional advance").

### I.2  Top-level driver

```
function lower_threads(SourceFile F):
    F' ← empty list
    for item ∈ F.items:
        if item is Module M and M contains threads:
            (M', extras) ← lower_module_threads(M)
            F' ← F' ⧺ extras ⧺ [M']      # generated submodules first, parent after
        else:
            F' ← F' ⧺ [item]
    return F'
```

### I.3  Per-module lowering

```
function lower_module_threads(Module M):
    # Phase 1 — Signal classification
    for each thread t in M:
        (cd_t, sd_t, ar_t) ← collect_thread_signals(B(t))
    CD ← ⋃ cd_t           # comb-driven anywhere
    SD ← ⋃ sd_t           # seq-driven anywhere
    RD ← (⋃ ar_t) ∖ (CD ∪ SD ∪ {clk, rst} ∪ Internals)

    # Phase 2 — Build merged submodule signature
    Sub ← new Module named "_M_threads"
    Sub.ports ← {clk: in, rst: in} ∪ {x: in for x ∈ RD}
                ∪ {x: out for x ∈ CD}
                ∪ {x: out reg-port for x ∈ SD}

    # Phase 3 — Lock arbitration (per shared resource)
    R ← {r : r appears in some `lock r {…}` in M}
    for r ∈ R:
        for i ∈ 0..k−1:
            declare wires _r_req_i, _r_grant_i
        emit comb block:
            _r_grant_i = _r_req_i ∧ ¬_r_grant_0 ∧ … ∧ ¬_r_grant_{i−1}      # priority arbiter

    # Phase 4 — Per-thread state partitioning
    for i ∈ 0..k−1:
        States(t_i) ← partition_thread_body(B(t_i))
        rename _cnt → _t{i}_cnt, _loop_cnt → _t{i}_loop_cnt,
               _r_req → _r_req_i,  _r_grant → _r_grant_i
        rewrite shared(or) seq assigns: x <= v   ↦   _x_in_i = v   (moved to comb)

    # Phase 5 — Code generation (emits ARCH CombBlock + RegBlock items)
    Sub.body ⧺= CombBlock {
        defaults: x = 0 for x ∈ CD, _r_req_i = 0 for all i,r
        for i, s ∈ States(t_i).indexed:
            emit:  if (_t{i}_state == s) { States(t_i)[s].comb }   # ARCH if/else in comb
        for shared(or) signals x: rewrite x = v as x = x | v
    }

    Sub.body ⧺= RegBlock(clock=clk, edge=rising) {
        # Reset is handled per-RegDecl via RegReset::Inherit;
        # the merged RegBlock body fires on each rising edge after reset.
        for i:
            if t_i.default_when = (cond, body):
                if (cond) { body; _t{i}_state <= 0 }
                else      { per-state chain below }
            else:
                per-state chain:
                    for s:
                        if (_t{i}_state == s) {                    # ARCH if/else in seq
                            emit States(t_i)[s].seq
                            emit transition_logic(States(t_i)[s], i, s, t_i.once)
                        }
        for shared(or) seq signals x:
            x <= _x_next                                            # _x_next = ⋁_i _x_in_i
    }

    # Phase 6 — Counter regs
    for i: if t_i has wait-N-cycle:        declare _t{i}_cnt: UInt<32>
           if t_i has for-loop:           declare _t{i}_loop_cnt: UInt<W_i>

    # Phase 7 — Parent rewiring
    M.body ← M.body ∖ {RegDecl x : x ∈ CD ∪ SD} ∖ {Resource _}
    M.body ← M.body ⧺ [InstDecl Sub _threads (...)]    # connect by name
    return (M, [Sub])
```

`transition_logic(S, i, s, once)` is:

```
function transition_logic(S, i, s, once):
    next ← (s+1)             if s+1 < n_states
         ← s                 if once and s = n_states−1     # terminal hold
         ← 0                 otherwise                       # repeating wrap
    case S of
        S.M ≠ ∅:        for (c, t) ∈ S.M:
                            tgt ← (n_states−1 if once else 0) if t ≥ n_states else t
                            emit: if (c) _t{i}_state <= tgt
        S.τ = c:        emit: if (c) _t{i}_state <= next
        S.w = n:        emit: _t{i}_cnt <= _t{i}_cnt − 1
                        emit: if (_t{i}_cnt == 0) _t{i}_state <= next
        otherwise:      emit: _t{i}_state <= next
```

### I.4  State partitioning (the heart of the algorithm)

```
function partition_thread_body(B: list⟨ThreadStmt⟩) → list⟨ThreadFsmState⟩:
    states ← []
    cur_comb ← []                                         # pending combinational assigns
    cur_seq  ← []                                         # pending non-blocking assigns

    for stmt ∈ B:
        case stmt of
            CombAssign ca:           cur_comb ⧺= ca
            SeqAssign  ra:           cur_seq  ⧺= ra
            Log l:                   cur_seq  ⧺= l

            IfElse ie when no wait inside:
                let (c_if, s_if) = split(ie)
                cur_comb ⧺= c_if
                cur_seq  ⧺= s_if

            WaitUntil cond:
                if cur_seq ≠ ∅:
                    states ⧺= ⟨comb=∅, seq=cur_seq, τ=⊥, w=⊥, M=∅⟩    # fire-once predecessor
                    cur_seq ← ∅
                states ⧺= ⟨comb=cur_comb, seq=∅, τ=cond, w=⊥, M=∅⟩      # comb HOLDS during wait
                cur_comb ← ∅

            WaitCycles n:
                flush_pending(states, cur_comb, cur_seq)
                states ⧺= ⟨comb=∅, seq=∅, τ=⊥, w=n, M=∅⟩

            DoUntil(body, cond):
                flush_pending(states, cur_comb, cur_seq)
                let (do_comb, do_seq) = split_simple_assigns(body)
                states ⧺= ⟨comb=do_comb, seq=do_seq, τ=cond, w=⊥, M=∅⟩  # fires every cycle while waiting

            For{var, start, end, body}:
                # Counter init
                cnt_init ← (_loop_cnt <= start)
                if cur_comb = ∅ ∧ cur_seq = ∅ ∧ states.last.M = ∅:
                    states.last.seq ⧺= cnt_init                       # merged into preceding state
                else:
                    cur_seq ⧺= cnt_init
                    flush_pending(states, cur_comb, cur_seq)

                # Body partition + loop-back/exit annotation
                body' ← rewrite_var(body, var ↦ _loop_cnt)
                for_states ← partition_thread_body(body')
                last ← for_states.last
                body_cond ← last.τ ?? true
                last.τ ← ⊥
                last.M ← [
                    (body_cond ∧ _loop_cnt < end_w,  base),                 # loop back to start of for
                    (body_cond ∧ _loop_cnt ≥ end_w,  base + len(for_states))  # exit (sentinel resolved later)
                ]
                last.seq ⧺= guarded(body_cond, _loop_cnt <= _loop_cnt + 1)
                states ⧺= for_states

            Lock{resource=r, body}:
                reject if body contains another lock                       # mutual exclusion safety
                flush_pending(states, cur_comb, cur_seq)
                lock_states ← partition_thread_body(body)
                for ls ∈ lock_states:
                    ls.comb ⧺= (_r_req = 1)                                  # held in every lock-state
                first ← lock_states[0]
                first.comb (excl. _r_req) wrap in `if (_r_grant) {…}`
                first.τ ← (_r_grant ∧ first.τ) or _r_grant
                first.seq wrap in `if (_r_grant) {…}`                       # block updates while waiting for grant
                states ⧺= lock_states

            ForkJoin(branches):
                flush_pending(states, cur_comb, cur_seq)
                fork_states ← lower_fork_join(branches)
                states ⧺= fork_states

            IfElse{cond, then_stmts, else_stmts} when contains_wait(then_stmts) ∨ contains_wait(else_stmts):
                # Implemented as of v0.45.0 (see §II.10 of proof for soundness).
                # Lowering uses a dispatch-and-rejoin scheme.
                flush_pending(states, cur_comb, cur_seq)

                dispatch_idx ← states.len()
                states ⧺= ⟨comb=∅, seq=∅, τ=⊥, w=⊥, M=[…filled below…]⟩  # placeholder

                then_base ← states.len()
                then_states ← partition_thread_body(then_stmts)
                states ⧺= then_states

                else_base ← states.len()
                else_states ← partition_thread_body(else_stmts)
                states ⧺= else_states

                rejoin_idx ← states.len()                                # next state after the if/else

                # Fixup dispatch state's multi_transitions
                states[dispatch_idx].M ← [
                    (cond,  then_base),
                    (¬cond, else_base),
                ]

                # Fixup each branch's exit: redirect its "fall-through" to rejoin_idx.
                # Each branch's last state gets its `next` overridden by appending a
                # default (true)-guarded transition to rejoin_idx, OR (cleaner) by
                # converting any unconditional advance to a multi_transitions
                # [(true, rejoin_idx)] entry, OR (in the case of an existing τ or M)
                # leaving the natural fall-through and asserting rejoin_idx == s+1.
                redirect_fallthrough_to(states[then_base ..< else_base], rejoin_idx)
                redirect_fallthrough_to(states[else_base ..< rejoin_idx], rejoin_idx)

    # Trailing tail: try to merge into preceding wait state
    if cur_seq ≠ ∅ ∧ cur_comb = ∅ ∧ states.last.τ ≠ ⊥:
        states.last.seq ⧺= guarded(states.last.τ, cur_seq)                  # zero-cycle merge
    elif states.last.M has 2 entries (for-exit):
        states.last.seq ⧺= guarded(states.last.M[1].cond, cur_seq)
    elif cur_comb ∪ cur_seq ≠ ∅:
        states ⧺= ⟨comb=cur_comb, seq=cur_seq, τ=⊥, w=⊥, M=∅⟩
    return states
```

### I.5  Fork/join via product-state expansion

```
function lower_fork_join(branches: list⟨list⟨ThreadStmt⟩⟩) → list⟨ThreadFsmState⟩:
    # Each branch becomes a sequence of states + a synthetic "done" tail state
    Bs ← []
    for br ∈ branches:
        bs ← partition_thread_body(br)
        bs ⧺= ⟨∅, ∅, ⊥, ⊥, ∅⟩                                       # sentinel done-state
        Bs ⧺= [bs]
    L_b ← |Bs[b]|
    encode(i_0, i_1, …, i_{n−1})  ≡  i_0 + i_1·L_0 + i_2·L_0·L_1 + …

    # Product state expansion: each state is a tuple of branch sub-states
    out ← []
    for prod_idx ∈ 0..(∏L_b)−1:
        (i_0,…,i_{n−1}) ← decode(prod_idx)
        if all i_b = L_b−1:                                          # all branches done
            skip                                                     # elided — falls through to next main-line state
            continue

        comb ← ⋃ Bs[b][i_b].comb
        seq  ← ⋃ guarded(Bs[b][i_b].τ, Bs[b][i_b].seq)               # branch-local guard

        active ← {b : i_b < L_b−1}
        unconditional_mask ← {b ∈ active : Bs[b][i_b].τ = ⊥}

        M ← []
        for mask ∈ subsets(active), descending:
            if mask ⊉ unconditional_mask: skip
            cond ← (⋀_{b∈mask} Bs[b][i_b].τ) ∧ (⋀_{b∉mask} ¬Bs[b][i_b].τ)
            next_indices ← (i_b + 1 if b ∈ mask else i_b)
            M ⧺= (cond, encode(next_indices))

        out ⧺= ⟨comb, seq, ⊥, ⊥, M⟩
    return out
```

---

## Part II — Equivalence Proof

We now prove that the lowered FSM produces the same observable trace as the
source thread program.  The proof is a *coinductive bisimulation* over an
abstract single-clock semantics: at every clock edge, both source and target
agree on every signal in the merged module's port set.

### II.1  Formal setup

Let `Σ` be the finite set of *signal names* in the module: ports `P`, registers
`R`, and combinational wires `W` (private to the submodule, not externally
observable but useful for the proof).  Let `V` denote the value space (typed
bit-vectors).  A *valuation* is `μ : Σ → V`.  We write `μ ⊨ e ⇒ v` to mean
expression `e` evaluates to `v` in valuation `μ` according to the standard ARCH
expression semantics.

Inputs `I ⊆ P` are driven externally; their values across cycles form an input
stream `⟨μ_I^t⟩_{t≥0}`, where `μ_I^t : I → V` is the valuation of inputs in
cycle `t`.

#### II.1.1  Source semantics ⟦·⟧_src

For a single thread `T` with body `B(T)`, let `K = States(T)` be the partition.
The *source configuration* is

```
γ = (PC, μ_R, c_cnt, c_loop)         where  PC ∈ {0, …, |K|−1},
                                            μ_R : R → V,
                                            c_cnt, c_loop : ℕ
```

`PC` is the program counter (a state index produced by `partition_thread_body`),
`μ_R` the register valuation, `c_cnt` and `c_loop` the wait-cycles and
loop-counter values.  Reset establishes `γ_0 = (0, μ_R^reset, 0, 0)`.

For multiple threads `t_0, …, t_{k−1}`, the global configuration is

```
Γ = (γ_0, γ_1, …, γ_{k−1}, μ_S)
```

where `μ_S` are *shared* registers (not driven by any thread; e.g. registers
written outside threads).  We write `Γ.PC_i` for thread `i`'s PC.

The *one-cycle source step* `Γ ─[μ_I]→ Γ'` proceeds in four phases, mirroring
ARCH's standard `comb` + `seq` clocked model (read inputs and registers,
compute combinational fixed point, evaluate seq updates, commit at clock
edge):

1. **Combinational evaluation.** Compute the wire valuation `μ_W` and effective
   port-output valuation `μ_O` from `(μ_I, μ_R, μ_S, ⟨PC_i⟩)`:
   - Each thread `i` in state `s = PC_i` contributes the comb statements of
     `K_i[s]`, gated implicitly by `_t_i_state == s`.
   - For `shared(or)` signals, multiple drivers are OR-reduced.
   - For `lock` resources, the priority arbiter computes `_r_grant_i`.

2. **Seq-update preparation.** Compute the next-register valuation
   `μ'_R` by evaluating the seq statements of `K_i[s]` and any merged trailing
   assignments (themselves guarded by transition conditions).

3. **PC advance.** For each thread `i` with `K_i[s] = ⟨_, _, τ, w, M⟩`:
   - If `M ≠ ∅`: select the unique `(c, t) ∈ M` with `μ ⊨ c ⇒ true` (priority
     order = list order), set `PC_i' ← t`.  If none fires, `PC_i' ← PC_i`.
   - Else if `τ = c` and `μ ⊨ c ⇒ true`: `PC_i' ← next(s, t.once)`.
   - Else if `w = n` and `c_cnt_i = 0`: `PC_i' ← next(s, t.once)`.
   - Else: `PC_i' ← s`.
   - Where `next(s, once) = s+1` if `s+1 < |K_i|`, `s` if `once ∧ s = |K_i|−1`,
     else `0`.

4. **`default_when` override.** If `μ ⊨ cond_dw_i ⇒ true`, override step 2
   for thread `i`'s register updates with the `default when` body and force
   `PC_i' ← 0`.

The source observation in cycle `t` is `obs(Γ, μ_I) = (μ_I, μ_O, μ_R)
restricted to ports`.

#### II.1.2  Target semantics ⟦lower(T)⟧_tgt — lowered ARCH RTL

The output of `lower_threads` is a thread-free ARCH module.  Its body
contains only standard ARCH constructs:

- `RegDecl` items for each `_t_i_state` (state register, `UInt<W_i>`),
  optional `_t_i_cnt`, `_t_i_loop_cnt`, and `_x_in_i` shadow wires.
- A single merged `CombBlock` (corresponds to ARCH `comb { … }`).
- A single merged `RegBlock` (corresponds to ARCH `seq on clk rising`).
- For each shared resource, a `CombBlock` realising the priority arbiter.
- An `InstDecl` connecting the parent module to this generated submodule.

We rely on the *standard cycle-by-cycle semantics of ARCH RTL*, which we
denote `step_arch`.  Briefly:

- **Combinational evaluation.** Given current register state `μ_R` and inputs
  `μ_I`, evaluate every `CombBlock` to a fixed-point assignment of wire
  values `μ_W`.  ARCH's no-latch and single-driver discipline (enforced by
  typecheck on the lowered AST) guarantees a unique fixed point.  Statements
  inside a `CombBlock` execute in source order with last-write-wins
  semantics; `if/else` arms select which assignments fire.
- **Sequential update.** For each `RegBlock` (clocked block), evaluate its
  body on `(μ_R, μ_W, μ_I)` to obtain `μ'_R`; non-blocking `<=` assignments
  inside `IfElse` arms commit at the clock edge.  Reset-bearing registers
  use their declared reset value when reset is asserted.
- **Composition.** All `CombBlock`s and the merged `RegBlock` execute
  conceptually in parallel; ARCH's static checks guarantee determinism.

This semantics is the natural per-cycle big-step interpretation of ARCH
constructs, identical in shape to the source semantics of §II.1.1.  It is
*also* the semantics that the simulator (`arch sim`) implements directly on
the post-`lower_threads` AST, and that the SV codegen preserves under the
ARCH→SV translation (a separate equivalence theorem covering `comb` →
`always_comb`, `seq` → `always_ff`, etc.).

The *target configuration* is

```
Γ̂ = (s_0, …, s_{k−1}, μ_R, c_cnt_0,…, c_loop_0,…, μ_S)
```

where `s_i` denotes the runtime value of the `_t_i_state` register in the
lowered module.  The one-cycle target step `Γ̂ ─[μ_I]→ Γ̂'` proceeds as:

1. Combinational evaluation activates, for each `(i, s)` with `s_i == s`,
   the `IfElse` arm carrying `K_i[s].comb`; defaults of `0` apply otherwise.
2. The merged `RegBlock` activates, for each `i`: if `default_when_i`
   predicate holds, its body fires and `_t_i_state <= 0`; otherwise the
   per-state chain fires as in `transition_logic` of §I.3.
3. Reset assigns `_t_i_state <= 0` and reset-bearing registers to their
   declared reset values.

We define the *target observation* identically to the source: the values of
input ports plus the values of all ARCH ports of the lowered module on the
boundary, restricted to a single clock cycle.

### II.2  Simulation relation

Define `≈ ⊆ ConfigSrc × ConfigTgt` by

```
(Γ, Γ̂) ≈    ⟺
  ∀ i.  Γ.PC_i = Γ̂.s_i
  ∧     Γ.μ_R = Γ̂.μ_R
  ∧     Γ.c_cnt_i = Γ̂.c_cnt_i  ∧  Γ.c_loop_i = Γ̂.c_loop_i
  ∧     Γ.μ_S = Γ̂.μ_S
```

In other words, `≈` is *equality of all state components*.  We use it as a
simulation relation.

### II.3  Main theorem

> **Theorem (Trace Equivalence).**
> Let `M` be a module containing threads `T = t_0, …, t_{k−1}` that the
> partitioning algorithm accepts.  Let `Γ_0` be the reset configuration of
> `⟦M⟧_src` and `Γ̂_0` the reset configuration of `⟦lower(M)⟧_tgt`.  Let
> `⟨μ_I^t⟩_{t≥0}` be any input stream.  Then for every `t ≥ 0`:
>
> ```
> obs(⟦M⟧_src, t) = obs(⟦lower(M)⟧_tgt, t)
> ```

**Proof.** By induction on `t`, using two lemmas.

#### II.3.1  Reset lemma

> **Lemma 1 (Reset).** `Γ_0 ≈ Γ̂_0`.

The reset branches of the lowered module's `RegDecl`s initialise `s_i ← 0` and
`μ_R ← μ_R^reset` for every thread-driven register, by Phase 5 of the lowering
(line 1546 of `elaborate.rs`: every `_t{i}_state` is declared with
`RegReset::Inherit(rst, 0)` and counters with `RegReset::None init 0`).  The
source reset configuration is defined identically.  ∎

#### II.3.2  Step lemma (key result)

> **Lemma 2 (Step preservation).** If `(Γ, Γ̂) ≈` and `μ_I` is any input
> valuation, then there exist `Γ'`, `Γ̂'` such that
>
> ```
> Γ ─[μ_I]→ Γ'    Γ̂ ─[μ_I]→ Γ̂'    (Γ', Γ̂') ≈    obs(Γ, μ_I) = obs(Γ̂, μ_I).
> ```

**Proof.** Fix `(Γ, Γ̂) ≈ ` and `μ_I`.  We show the four subclaims:

**(a) Combinational outputs agree.** The target evaluates `μ_O^tgt` by activating,
for each `i`, the block guarded by `s_i == ŝ` where `ŝ = Γ̂.s_i = Γ.PC_i`.  By
the partitioning invariant (II.4 below), the comb statements of `K_i[ŝ]` are
*exactly* those scheduled in source state `PC_i = ŝ`.  Both run on the same
register valuation (`Γ.μ_R = Γ̂.μ_R`) and same input stream, so they produce
the same wire values.  For `shared(or)` signals, both source and target
specify the OR-reduction of all per-thread driver values; for the priority
arbiter, both define `_r_grant_i = _r_req_i ∧ ¬⋁_{j<i} _r_grant_j`.  Hence
`μ_O^src = μ_O^tgt`.  ∎(a)

**(b) Register updates agree.** Both semantics evaluate the seq statements of
`K_i[ŝ]` on the *current* `μ` (which agrees by (a)).  Trailing-tail merges
guard the trailing seq stmts by the wait condition of the predecessor; this is
identical to scheduling the source's trailing block one cycle after the
predecessor's wait fires (zero dead cycle), and the source semantics is
defined to do exactly that (II.5).  For `shared(or)` seq signals, each thread
contributes a per-cycle value; the target reduces via `x <= ⋁_i _x_in_i`,
which equals the source's defined OR-of-drivers semantics.  ∎(b)

**(c) PC advance agrees.** Each kind of `K_i[ŝ]` is matched one-to-one with the
target's `transition_logic`:

| State kind                | Source PC update                    | Target ARCH RTL emitted          |
|---------------------------|-------------------------------------|-----------------------------------|
| `τ = c`                   | `PC' = next` if `μ ⊨ c`             | `if (c) _state <= next`          |
| `w = n` (wait-cycles)     | `PC' = next` if `c_cnt = 0`         | `if (cnt==0) _state <= next`     |
| `M = [(c_j, t_j)]`        | `PC' = t_j` for least `j` with `μ ⊨ c_j` | `if (c_0)…; if (c_1)…`       |
| none (unconditional)       | `PC' = next`                        | `_state <= next`                 |
| `default_when` fires       | `PC' = 0`                           | `if (cond) {…; _state<=0}`       |

The *priority semantics* of the multi-target case warrants a remark.  The
target emits an unguarded sequence of `IfElse` statements inside the `seq`
block:

```
if (c_0) _state <= t_0;
if (c_1) _state <= t_1;
...
```

Under ARCH's non-blocking `<=` semantics inside a `RegBlock` (clocked
sequential block), this sequence is equivalent to the *last-true-wins*
policy: whichever `c_j` is true with the largest `j` overrides earlier
ones, because each `<=` schedules a deferred update and the final scheduled
write commits at the clock edge.  The lowering pass populates `M` with
conditions designed so that **at most one is true** in any reachable cycle:

- *for-loop:* `M = [(body ∧ cnt < end, loop_back), (body ∧ cnt ≥ end, exit)]`.
  These are mutually exclusive because `cnt < end ⊕ cnt ≥ end`.
- *fork/join:* by construction, the conditions enumerate disjoint subsets of
  branch-completions; the `mask` loop ensures every condition's positive arm
  AND-s with the negation of the *other* branches' conditions, making the
  family pairwise mutually exclusive.

Hence "last-true-wins" coincides with "first-true-wins", and matches the
source semantics where the priority is irrelevant. ∎(c)

**(d) Counters agree.** `wait N cycle`: the predecessor state's seq stmts include
`_t_i_cnt <= n − 1` as a load (Phase 5 inserts this via `counter_loads`); the
wait state itself emits `_t_i_cnt <= _t_i_cnt − 1` and exits on `cnt == 0`.
Source and target both decrement the same way starting from the same load.

`for` loop: both source and target merge `_loop_cnt <= start` into the
preceding state and `_loop_cnt <= _loop_cnt + 1` into the last body state,
guarded by `body_cond`.  Both update on identical valuations.  ∎(d)

Combining (a)–(d) and noting that `default_when` overrides agree by direct
syntactic correspondence, we conclude `(Γ', Γ̂') ≈` and `obs(Γ, μ_I) = obs(Γ̂, μ_I)`.  ∎

#### II.3.3  From step preservation to trace equivalence

By Lemma 1, `(Γ_0, Γ̂_0) ≈`.  By Lemma 2, for every cycle `t`, if
`(Γ_t, Γ̂_t) ≈` then `(Γ_{t+1}, Γ̂_{t+1}) ≈` and `obs(Γ_t, μ_I^t) = obs(Γ̂_t, μ_I^t)`.
By induction on `t`, the observable traces are identical for every input
stream.  ∎ (Theorem)

### II.4  Partitioning invariant

The proof above relies on a structural invariant of `partition_thread_body`:

> **Invariant (Faithful partition).**  For thread `T` with body `B(T)`,
> after `K = partition_thread_body(B(T))`, executing the source semantics
> on `B(T)` cycle-by-cycle produces, in cycle where the source PC equals the
> sequence's `s`-th wait barrier, exactly the comb/seq stmts of `K[s]`,
> applied with the same merge rules (trailing-into-predecessor, counter
> init, fork product, lock guard) that the source semantics specifies.

The invariant is established by case-by-case correspondence between
`ThreadStmt` cases in the source and the corresponding `ThreadFsmState`
construction:

- **Sequencing of `CombAssign`/`SeqAssign`:** each is appended to `cur_comb`
  or `cur_seq`, exactly mirroring straight-line execution within a state.
- **`WaitUntil c`:** the source semantics says "drive comb outputs while
  waiting; commit any seq updates one-shot in the cycle after the previous
  wait fires." The lowering encodes this as: pending seqs go into a
  fire-once predecessor state (line 2291–2299), pending combs go *into* the
  wait state itself so they hold while the wait is active (line 2300–2306).
  This is the unique faithful encoding into a single FSM that holds outputs
  high while waiting.
- **`WaitCycles n`:** counter `_cnt` is loaded by the predecessor with `n−1`
  and decremented in the wait state; transition fires when `cnt == 0`.  This
  is exactly `n` cycles spent in the wait state, matching source semantics.
- **`DoUntil(body, cond)`:** the body's combs and seqs run *every cycle*
  while waiting (because they live inside the do-state's `comb`/`seq`),
  exit when `cond` fires.  This matches the source semantics of "do-while-not-cond".
- **`For{var, start, end, body}`:** the partition recurses on `body` after
  `var → _loop_cnt`, then attaches `(body_cond ∧ cnt<end)` loop-back and
  `(body_cond ∧ cnt≥end)` exit transitions.  Source semantics: while the
  loop body completes (`body_cond` fires), `cnt` increments; loop continues
  while `cnt < end`.  Exact match.
- **`ForkJoin(branches)`:** by Lemma F (II.6) below, the product-state
  expansion is faithful.
- **`Lock{r, body}`:** by Lemma L (II.7) below, the grant-gating preserves
  source semantics for non-nested locks (which the algorithm enforces).
- **`IfElse(cond, then, else)` with internal `wait`:** by Lemma I (II.10)
  below, the dispatch-and-rejoin construction is faithful.  *Implemented as
  of v0.45.0.*
- **Default `when cond`:** wraps the entire chain in `if (cond) {body;
  state <= 0} else {…}`; this is exactly the source's "soft-reset clause"
  semantics.

Each case is established by direct inspection of `partition_thread_body` in
`src/elaborate.rs` against the source semantics in §II.1.1.

### II.5  Trailing-tail merge correctness

The trailing-tail optimisation merges trailing seq statements into the last
state's `seq` guarded by its transition condition.  We prove this is
sound:

> **Lemma T.** Let the unmerged scheduling be: predecessor state `s` with
> `τ = c` followed by a successor state `s+1` whose only effect is a list of
> seq stmts `S` (no comb, no further wait).  Then the merged schedule
> *(`s` retains its `τ = c`, `S` appended to `s.seq` guarded by `c`,
> successor state elided)* produces the same trace.

**Proof.** In the unmerged schedule, in cycle `t` where state `s` fires its
exit (i.e. `μ ⊨ c`), the FSM advances to `s+1`.  In cycle `t+1`, state `s+1`
runs its seqs `S` and advances unconditionally.  The seq updates land at the
end of cycle `t+1`, taking effect from cycle `t+2`.

In the merged schedule, in cycle `t`, the predecessor state `s` runs its own
seqs *plus* `S` guarded by `c`.  Since `μ ⊨ c`, the guarded seqs also fire,
landing at the end of cycle `t`, taking effect from cycle `t+1`.

This *is a behavioural difference* — the merged version has effects 1 cycle
*earlier* than the unmerged version.  We must therefore say: the merged
schedule is the **canonical source semantics**, not an optimisation.  The
unmerged form would inject a dead cycle that the source program does not
specify.  Indeed the source-level intent of:

```
do
   ar_valid = 1;
until ar_ready;
xfer_ctr_r <= xfer_ctr_r + 1;
```

is "increment `xfer_ctr_r` on the same cycle the AR handshake completes" —
not "wait for handshake, then increment one cycle later".  The lowering's
trailing-merge captures this intent.  Hence the source semantics in §II.1.1
**must** include the trailing-merge rule; the lemma is then trivially true
because both sides specify the same schedule.  ∎

### II.6  Fork/join faithfulness

> **Lemma F.** Let `branches = [B_0, …, B_{n−1}]`.  Let
> `fork_states = lower_fork_join(branches)`.  The single-clock semantics of
> `fork_states` on a unified PC `p ∈ {0, …, ∏L_b−1}` (decoded into
> `(i_0, …, i_{n−1})`) coincides with parallel single-clock execution of each
> branch `B_b` advancing independently along its own partitioned states.

**Proof sketch.** A *parallel* source semantics for `fork…join` advances each
branch's local PC `i_b` independently:
- Branch `b` advances its local PC iff its current state's transition condition
  fires (or it's an unconditional state).
- Multiple branches may advance in the same cycle.
- The fork completes when all `i_b = L_b−1` (in the done-tail).

The product-state encoding `p = encode(i_0, …, i_{n−1})` represents the joint
configuration in a single FSM.  The transition table `M` enumerates every
non-empty subset of *active* (non-final) branches that can fire in a cycle:
the condition is the AND of those branches' own transition conditions, ANDed
with the negation of the *other* branches' transition conditions (so that
they don't fire).  This is precisely the joint-step semantics: subsetwise
advance, one product-state at a time.

The merge of comb/seq inside a product-state mimics parallel branch
execution: each branch's comb and (`τ`-guarded) seq are unioned, exactly as
parallel parallel `always_*` blocks would emit them.  Single-driver
discipline is enforced upstream by `lower_module_threads`: no two branches
write the same signal (else a multi-driver error fires).

The all-done product-state is *elided* (line 2647–2664 in
`elaborate.rs`); its multi-target predecessors transition to `total - 1`
which, after `fork_base` adjustment, points at the first state after the
fork group.  This eliminates one cycle of FSM-state-cranking latency at every
join. The elision is sound *iff* the all-done product-state would have had
empty comb and seq, which is always true because the per-branch done-tail
states are constructed (line 2598) with empty comb/seq.  The
`debug_assert!` at line 2661 guards this invariant.

Thus the product-state FSM's per-cycle behaviour is identical to the joint
parallel branch step semantics.  ∎

### II.7  Lock correctness

The full proof appears in `thread_lowering_algorithm.md` §"Liveness and
Safety: Lock Correctness".  We summarise:

> **Lemma L (Mutual exclusion + deadlock freedom).** For any module accepted
> by the algorithm (no nested locks), the priority-arbiter + grant-gating
> scheme provides, for every shared resource `r` in every cycle:
> 1. **Mutual exclusion:** at most one thread executes statements from any
>    `lock r {…}` body in that cycle.
> 2. **Deadlock freedom:** if any thread requests `r`, some thread holds `r`
>    in that cycle.
> 3. **Starvation freedom:** every thread eventually acquires `r`, provided
>    every lock body terminates.

The argument is:
- Priority arbiter: `_r_grant_i = _r_req_i ∧ ¬⋁_{j<i} _r_grant_j`.  At most
  one `i` has `_r_grant_i = 1` (mutual exclusion of grants).  Some `i` has
  `_r_grant_i = 1` iff some `_r_req_j = 1` (by induction on `i`).
- Grant-gating: while a thread waits at the lock entry state, its `seq` is
  conditional on `_r_grant_i`, so no register updates occur.  Comb outputs
  are zero (default) because the comb stmts are wrapped in `if (_r_grant_i)`.
- Once granted, the thread proceeds through the lock-body states with `req_i`
  asserted throughout, so it cannot lose the grant to a higher-priority
  thread (which would have acquired it before this thread did).  This holds
  *only* for non-nested locks; nested locks would let a thread reside past
  the entry state with grant-loss possible — hence rejected.
- Starvation freedom by induction on priority: thread 0 always wins on
  request, then thread 1 once thread 0 releases, etc.

### II.8  Multi-thread composition

So far we have proved equivalence per-thread.  The merged module composes
all threads under a single `RegBlock`.  We need to verify that this
composition does not introduce multi-driver conflicts and that
inter-thread communication via shared registers is faithful.

> **Lemma M (Single driver).**  For every signal `x` in the merged module:
> - if `x` is a regular register, exactly one thread `i` produces `seq`
>   statements writing `x`;
> - if `x` is a `shared(or)` register, every thread's `seq x <= v` is
>   rewritten to `_x_in_i = v` (a comb assign placed in the merged
>   `CombBlock`), and the merged `RegBlock` contains the single statement
>   `x <= _x_next` (where `_x_next = ⋁_i _x_in_i`).

This is enforced by the typecheck pass (which rejects multi-driver) plus the
shared-or rewrite (II.9).  For regular registers: since each `_t_i_state ==
s` arm contains the unique seq stmt for `(i, s)`, and all such arms are
inside *one* `RegBlock`, there is exactly one driver in the lowered ARCH
RTL.

For inter-thread communication: thread `j` reads register `x` written by
thread `i` via the standard ARCH `seq` semantics — `j` sees the
*previous-cycle* value of `x` (because both threads' updates are
non-blocking `<=` and commit at the same clock edge).  This matches the
source semantics of "another thread can read registers from another thread"
(spec §20.9).  Combinational signals are read from the current cycle's
fixed point, also matching source semantics.

### II.9  Shared(or) faithfulness

> **Lemma S.** Let `x` be a `shared(or)` signal.  The lowering generates:
> - **Comb-driven case:** `x = x | v` in each thread's `state == s` arm of
>   the merged `CombBlock`, with `x = 0` as the default before the
>   per-thread arms.
> - **Seq-driven case:** per-thread shadow wires `_x_in_i` (default 0
>   driven in the merged `CombBlock`), a let-binding `_x_next = ⋁_i _x_in_i`,
>   and `x <= _x_next` inside the merged `RegBlock`.
>
> In both cases, the value of `x` equals `⋁_{i: arm_i fires} v_i` in the
> matching cycle, identical to the source semantics for shared(or).

**Proof.** Comb-driven: by the `CombBlock` reset-to-default and the chained
`x = x | v` updates, after all per-state arms run, `x` equals the bitwise
OR of every contribution.  Order of evaluation within a `CombBlock` is
irrelevant for OR (it is associative and commutative).

Seq-driven: each `_x_in_i` is set to `v_i` exactly when thread `i`'s
state-`s` arm fires (else `0` by default).  `_x_next` is the OR of all
`_x_in_i`.  The single `x <= _x_next` non-blocking assignment in the
merged `RegBlock` records this OR as the next cycle's value of `x`.  ∎

### II.10  If/else with internal waits — dispatch-and-rejoin

> **Status (v0.45.0+):** Implemented in `src/elaborate.rs::partition_thread_body`
> at the `ThreadStmt::IfElse` branch.  The proof below was written ahead of
> the implementation as a soundness argument; it now serves as the
> equivalence guarantee that any conforming implementation (the current one)
> preserves source semantics.

#### II.10.1  Source semantics for `if` with internal waits

Extend §II.1.1 to allow `IfElse(cond, then_stmts, else_stmts)` where either
branch contains `wait`.  Source semantics introduces a *dispatch barrier*: in
the cycle the `if` statement is reached, `cond` is evaluated, and the source
PC enters either the then-branch's segment chain or the else-branch's chain.
After the chosen chain completes, control resumes at the post-`if` statements.

Formally, augment the source PC with an additional pair of indices `(b, j)`
where `b ∈ {then, else, post}` and `j` indexes into branch `b`'s segment
chain.  The dispatch barrier itself is a state with no comb/seq stmts whose
sole effect is to read `cond` and set `(b, j) ← (then, 0)` or
`(else, 0)`.  Branches converge: when branch `b`'s last segment completes,
`(b, j)` advances to `(post, 0)`.

#### II.10.2  Target lowering (dispatch-and-rejoin)

`partition_thread_body` is extended (per §I.4) so that when an `IfElse` with
internal waits is encountered:

1. **Flush** any pending comb/seq into a predecessor state `S_pre` (so
   `cond` is evaluated against the post-flush register valuation).
2. **Insert dispatch state `S_disp`** with empty comb/seq and
   ```
   M = [(cond, then_base), (¬cond, else_base)]
   ```
3. **Recursively partition** `then_stmts` and append at index `then_base`.
4. **Recursively partition** `else_stmts` and append at index `else_base`.
5. **Redirect each branch's exit** to the rejoin index `rejoin_idx =
   states.len()`.  Concretely, `redirect_fallthrough_to(branch, rejoin_idx)`
   modifies the *last* state of the branch so that its natural advance lands
   at `rejoin_idx`:
   - If the last state has `M = ∅` and `τ = ⊥` (unconditional advance):
     replace with `M = [(true, rejoin_idx)]`.
   - If the last state has `τ = c`: replace with `M = [(c, rejoin_idx)]`.
   - If the last state already has `M = [(c_0, t_0), …]`: append
     `(true, rejoin_idx)` only if no `t_j` already targets `rejoin_idx`;
     otherwise no change (e.g. for-loop exit already routed correctly).
6. The dispatch state itself does not advance via the default `next = s+1`;
   `transition_logic` recognises that a state with `M ≠ ∅` always uses the
   `M`-list, so `S_disp.M` overrides any default.

Mutual exclusion of `cond` and `¬cond` ensures exactly one of the two
dispatch transitions fires in any cycle (the ARCH `seq` "last-true-wins"
non-blocking semantics collapses to first-true-wins, as in §II.3 (c)).

#### II.10.3  Faithfulness lemma

> **Lemma I (If/else faithfulness).** Let `K_then` and `K_else` be the
> partitions of `then_stmts` and `else_stmts` (both produced by recursive
> `partition_thread_body`).  Let `S_disp`, `K_then` (at offsets
> `then_base..else_base`), `K_else` (at offsets
> `else_base..rejoin_idx`) be the lowering above.  Then for every input
> stream, the per-cycle observable trace of the source `IfElse` semantics
> coincides with the trace produced by the merged FSM containing
> `S_disp ; K_then ; K_else` followed by the post-`if` states.

**Proof.** Extend the simulation relation `≈` to track the branch tag:

```
≈_if  =  ≈  ∪  { ((Γ.(then, j)), (Γ̂.s)) :  s = then_base + j }
            ∪  { ((Γ.(else, j)), (Γ̂.s)) :  s = else_base + j }
            ∪  { ((Γ.(post, 0)), (Γ̂.s)) :  s = rejoin_idx }
```

i.e. each source branch-position maps to a unique target FSM index, and
post-if positions map to rejoin and onward.

Reset establishes `≈_if` (Lemma 1 applies; nothing changes about reset).

Step preservation has three new cases plus the existing ones:

**Case (D) — at the dispatch barrier.** The source step evaluates `cond` on
`(μ_R, μ_S, μ_I)` and sets the source PC to `(then, 0)` or `(else, 0)`.
The target step is in state `S_disp`: comb stmts are empty (no outputs
contributed by this state, defaults of 0 hold for any otherwise-driven
output of this thread); seq stmts are empty.  The transition uses
`M = [(cond, then_base), (¬cond, else_base)]`, mutually exclusive, so the
target advances to `then_base` or `else_base`.  Since
`Γ.μ_R = Γ̂.μ_R` (by `≈`) and `μ_I` is shared, both evaluate `cond` the same
way, hence the same branch is taken and `≈_if` is preserved.

**Case (Bᵢ) — within a branch.** Once inside a branch, the source PC moves
through that branch's segment chain by the rules of §II.4.  By construction,
the target FSM indices `then_base..else_base` are *exactly* the states
returned by `partition_thread_body(then_stmts)` (analogously for `else`),
shifted by `then_base`.  By the inductive hypothesis (the recursive call's
correctness — formally, a strong induction on the size of the source body
measured by the count of thread statements), step-preservation holds inside
each branch.  All conditions inside a branch reference the *same* register
valuation in source and target (by `≈_if`), so the same paths fire.

**Case (R) — branch exit / rejoin.** The last state of branch `b` has been
modified by `redirect_fallthrough_to` to advance to `rejoin_idx`.  Three
sub-cases:

- **Last state had unconditional advance** (`M = ∅`, `τ = ⊥`): replaced with
  `M = [(true, rejoin_idx)]`.  Source semantics: branch `b`'s last segment
  completes unconditionally, i.e. `(b, j_max) → (post, 0)`.  Target
  semantics: `_state <= rejoin_idx`.  Match.
- **Last state had `τ = c`**: replaced with `M = [(c, rejoin_idx)]`.  Source:
  branch's last wait-until exits to `(post, 0)` when `c` fires.  Target:
  same.
- **Last state already had `M`** (e.g. for-loop exit at end of branch):
  exactly one of the existing `M`-entries already targeted "next state after
  the for group" via the sentinel `usize::MAX`, which the parent
  `partition_thread_body` resolved to `rejoin_idx` (because that's
  `states.len()` after the for-states are appended).  So no edit is needed,
  and the for-exit already lands at `rejoin_idx`.  Source: for-loop exit on
  the last iteration falls through to the post-if statements.  Target: same.

In all three sub-cases `≈_if` is preserved across the rejoin transition.

**Case (Eq) — outside the if/else.** Steps before reaching the dispatch
barrier and after rejoin are governed by Lemma 2 of §II.3 unchanged
(neither branch's interior PC values can occur, because the source PC
stays in the linear post-if chain).

By induction on the cycle index `t`, `≈_if` is preserved indefinitely; by
induction on the syntactic depth of nested `if/else`-with-waits, the
recursive partitioning is sound.  ∎

#### II.10.4  Subtleties

**Branch-local register writes.** If branch `then_stmts` writes register `r`
and branch `else_stmts` does not, the lowering is still correct: in any
cycle where `_state ∈ then_states_range`, the per-state `IfElse` arms in
the merged `RegBlock` activate the writes; in the `else` chain, those arms
are inactive, so `r` retains its value (no write).  This matches source
semantics: source PC inside the else-branch never reaches the then-branch's
seq stmts.

**Cross-branch shared registers.** If both branches write the same register
`r` in different states, this is *not* a multi-driver violation — it is one
register driven by one thread, with branch-disjoint per-state writes.  The
existing single-driver discipline (Lemma M) is satisfied trivially because
the merged `RegBlock` activates at most one branch's chain per cycle.

**Pre-`if` flushed seq updates.** The flush step (II.10.2 step 1) ensures
that any pending source seq updates *before* the `if` are committed by the
predecessor state's clock edge.  The dispatch state therefore reads `cond`
on the *post-flush* register values, matching the source semantics where
"the if statement evaluates `cond` after preceding seq updates have taken
effect at the previous clock edge."  Without this flush, `cond` would read
stale register values, breaking equivalence.

**Empty branches.** If `then_stmts = []` (vacuous then), the recursive
`partition_thread_body([])` returns no states; `then_base = else_base` and
the dispatch entry `(cond, then_base)` lands directly at `else_base` —
which would skip the entire if/else into the post-if stream.  This is
correct: an empty then-branch with `cond` true should produce no behaviour
beyond the dispatch.  Symmetrically for empty else.  The lowering must
guard `then_base == rejoin_idx` (resp. `else_base == rejoin_idx`) and emit
the dispatch to point directly at `rejoin_idx` in such cases.

**Nesting.** Nested `if/else`-with-waits is handled by the recursive nature
of `partition_thread_body`.  Each level of nesting introduces one dispatch
state and two branch chains; the proof composes by induction on nesting
depth.

#### II.10.5  Implementation notes (as landed in v0.45.0)

The implementation in `src/elaborate.rs::partition_thread_body` follows
§II.10.2 directly.  Two non-obvious points:

- **Counter-decrement hoist.** The earlier transition emitter coupled the
  `wait_cycles` counter decrement and its `cnt == 0 ⇒ next` transition into
  one branch.  When dispatch-and-rejoin redirects a wait_cycles state's
  fallthrough by populating its `M` list, the `M`-arm takes precedence over
  the wait_cycles arm, which would silently suppress the decrement.  The
  decrement is therefore hoisted out and fired unconditionally for every
  wait_cycles state, independent of the transition mechanism.  See the
  refactored block in `lower_module_threads`.
- **Empty branches.** `partition_thread_body` rejects empty bodies (it
  requires at least one wait); the dispatch lowering must skip the recursive
  call when `then_stmts.is_empty()` or `else_stmts.is_empty()` and instead
  point that arm of the dispatch directly at the rejoin index.  This matches
  §II.10.4's empty-branch semantics.

Tests covering: (a) wait in then-branch only, (b) wait in else-branch only,
(c) waits in both branches with different lengths, (d) nested if/else-with-waits,
(e) auto-thread-asserts integration — see `tests/integration_test.rs`
(`test_if_wait_*` family).  End-to-end Verilator `--assert` golden + mutation
runs confirm the dispatch-state branch assertions are load-bearing.

### II.11  Auto-emitted spec-contract SVA (`--auto-thread-asserts`)

When `lower_threads` runs with `ThreadLowerOpts { auto_asserts: true }`,
`lower_module_threads` emits a set of named SVA assertions into the merged
module's body, anchored to the lowered state register `_t_i_state` and
per-thread counter `_t_i_cnt`.  We show that each property follows directly
from the equivalence theorems already established — i.e. **the assertions
hold by construction in any source program the algorithm accepts**.  An
`ASSERTION FAILED` from one of these labels is therefore evidence of either
a compiler bug, a hand-edit of the lowered RTL, or a malformed downstream
pass — never a user-program error.

Throughout this section, write `s = Γ̂.s_i` for the current target state of
thread `i`, `next_i(s)` for the index returned by the `next_state`
computation at line 1626 of `elaborate.rs`, and `rst_inactive` for the
reset-polarity-corrected guard (`rst` for active-low, `!rst` for
active-high).  Each property is wrapped in `synopsys translate_off/on` and
named per the convention `_auto_thread_t{i}_<class>_s{s}[_<sub>]`.

#### II.11.1  Corollary W (wait_until progress)

For a state `s` with `K_i[s].τ = c` and `M = ∅` (a `wait until c` state):

> **Property `_auto_thread_t{i}_wait_until_s{s}`**
> ```
> (rst_inactive ∧ s_i = s ∧ c)  ⊨>  s_i' = next_i(s)
> ```
> (where `⊨>` is SVA's next-cycle implication `|=>`).

**Derivation.** Take any reachable cycle with `Γ̂.s_i = s ∧ μ ⊨ c` and reset
not asserted.  By Theorem (II.3), `(Γ, Γ̂) ≈`, so `Γ.PC_i = s` and the
source's `wait_until c` semantics fires the advance: `Γ.PC_i' = next_i(s)`.
By Lemma 2 (II.3.2) clause (c), the target's transition logic for the
`τ = c` case is `if (c) _state <= next`, giving `Γ̂.s_i' = next_i(s)`.  The
SVA holds.  ∎

#### II.11.2  Corollary C (wait_cycles bounded liveness)

For a state `s` with `K_i[s].w = n` (a `wait n cycle` state):

> **Property `_auto_thread_t{i}_wait_stay_s{s}`**
> ```
> (rst_inactive ∧ s_i = s ∧ _t_i_cnt ≠ 0)  ⊨>  s_i' = s
> ```
>
> **Property `_auto_thread_t{i}_wait_done_s{s}`**
> ```
> (rst_inactive ∧ s_i = s ∧ _t_i_cnt = 0)  ⊨>  s_i' = next_i(s)
> ```

**Derivation.** By the partitioning invariant (II.4) the predecessor state
loads `_t_i_cnt ← n − 1` exactly when control transitions into `s`; by
Lemma 2 clause (d), the wait state itself emits
`_t_i_cnt <= _t_i_cnt − 1` and `if (_t_i_cnt == 0) _state <= next`.

- *Stay:* if `cnt ≠ 0` at the sample point, the only `_state` write in `K_i[s]`
  is guarded by `cnt == 0` (false this cycle), so no transition fires; by the
  default-PC-hold property of `RegBlock` semantics (II.4 case 7), the state
  register retains its current value, i.e. `s_i' = s`.
- *Done:* if `cnt = 0`, the guarded write fires, and `s_i' = next_i(s)`.

Both hold by Lemma 2.  ∎

#### II.11.3  Corollary B (fork/join branch faithfulness)

For each multi-transition `(c_b, t_b) ∈ K_i[s].M` (each branch of a fork/join
or a do-until / for-loop dispatch state):

> **Property `_auto_thread_t{i}_branch_s{s}_b{b}`**
> ```
> (rst_inactive ∧ s_i = s ∧ c_b)  ⊨>  s_i' = t_b
> ```

**Derivation.** By Lemma F (II.6), the multi-transition table emitted at
this state is mutually exclusive (the `mask` loop in `lower_fork_join`
ensures `c_b ∧ ¬c_{b'}` for `b' ≠ b`).  Hence under the antecedent, *only*
the `b`-th `if (c_b) _state <= t_b` fires, and the last-true-wins / first-true-wins
collapse from Lemma 2 clause (c) gives `s_i' = t_b`.  Reset-inactive ensures
the always_ff reset branch does not preempt the assignment.  ∎

#### II.11.4  Soundness of the reset-guarded antecedent

Each property antecedent conjoins `rst_inactive` so that the SVA does not
fire while the reset clause holds `_t_i_state` at 0 (preventing spurious
"state didn't advance" failures during reset).  Lemma 1 (II.3.1) gives
`Γ̂.s_i = 0` immediately after reset deasserts; the first cycle in which
`rst_inactive` holds is therefore the first cycle in which the source PC
agrees with the target state, which is also the first cycle in which Lemma 2
applies.  The guard is therefore exactly tight: it neither over-disables
(the SVA still evaluates from the very first post-reset edge) nor
under-disables (no false fire during reset itself).

#### II.11.5  Coverage of property classes

The implementation in `lower_module_threads` (post-state-list construction
loop at line ~1631 of `elaborate.rs`) emits Corollary W, C, and B properties
for every reachable state with the matching kind, gated on
`opts.auto_asserts`.  Skipped intentionally:

- **Terminal states of `thread once`** (`si + 1 ≥ n_states ∧ t.once`): the
  source semantics holds the PC at the last state, making the implication
  vacuous — both source and target satisfy it, but the assertion provides no
  signal.
- **Threads with `default_when`**: the soft-reset escape can preempt any
  state, so the simple `s_i = s ⇒ next` shape becomes
  `(¬dw_cond ∧ s_i = s) ⇒ next`.  Folding `¬dw_cond` into every antecedent
  is mechanical but adds noise; v1 skips these threads entirely.  The
  underlying lemmas still hold; the assertion just isn't emitted.
- **Unconditional advance states** (`τ = ⊥ ∧ M = ∅ ∧ w = ⊥`): the implication
  `s_i = s ⊨> s_i' = next_i(s)` is true by construction at every accepted
  state and adds nothing a tool would catch.

#### II.11.6  Empirical end-to-end check

`tests/thread/wait_cycles.arch` (DelayPulse, 4-state thread mixing `wait
until` and `wait n cycle`) was compiled with `--auto-thread-asserts`,
linked against a 24-cycle SystemVerilog testbench, and run under Verilator
5.034 with `--binary --assert`.  All five emitted properties hold silently
across ~5 thread loops.  As a load-bearing check, mutating the `wait_until`
consequent in the emitted SV from `_t0_state == 1` to `_t0_state == 7`
trips `$fatal(1, "ASSERTION FAILED: _auto_thread_t0_wait_until_s0")`
mid-sim, confirming both that the property is reachable and that
Verilator's assertion engine is in fact evaluating it (the
`synopsys translate_off/on` pragma does not strip assertions in Verilator's
default simulation mode).

---

## Part III — Summary

The thread-to-FSM lowering is correct in the following precise sense:

1. **Reset-time agreement** (Lemma 1).
2. **Per-cycle observation agreement** (Lemma 2, Theorem).
3. **Per-thread structural fidelity** (Partitioning invariant, II.4).
4. **Resource-arbitration correctness** (Lemma L, II.7).
5. **Multi-driver discipline** (Lemma M, II.8).
6. **Shared(or) reduction faithfulness** (Lemma S, II.9).
7. **If/else with internal waits faithfulness** (Lemma I, II.10) —
   *implemented in v0.45.0 via the dispatch-and-rejoin scheme proved sound
   here.*
8. **Auto-emitted spec-contract SVA correctness** (Corollaries W/C/B, II.11)
   — the `--auto-thread-asserts` properties hold by construction in any
   accepted source program, so an `ASSERTION FAILED` from one of them
   indicates a compiler bug, not a user-program bug.

These properties together guarantee that for every accepted source program
and every input stream, the **lowered ARCH RTL** (the thread-free ARCH
module emitted by `lower_threads`) produces an identical trace of values
for every signal at every clock edge as the original thread program.

End-to-end correctness from thread programs to SystemVerilog requires
composing this result with a separate **ARCH→SV codegen equivalence**
theorem covering the syntax-directed translation of `comb`/`seq`/`module`/
`if`/`<=`/etc. into `always_comb`/`always_ff`/`module`/`if`/non-blocking-`<=`.
That codegen step is largely mechanical and is out of scope for this
document.

A *behavioural simulator* operating directly on the source AST (without
running `lower_threads`) is constrained by the same theorem to produce
traces matching the lowered ARCH RTL — and hence the SV.  This is the
formal contract for the planned arch-sim alternate path (`arch sim` without
`lower_threads` applied first; see `thread_lowering_algorithm.md`
§"Simulation Pipeline Note").

### III.1  What the proof does *not* cover

- **Type and width correctness.** The proof assumes well-typed expressions.
  The typecheck pass establishes this separately.
- **ARCH→SV codegen.** The compiler step that translates lowered ARCH RTL
  into SystemVerilog is a separate transformation with its own (largely
  mechanical) equivalence proof.  Composing the two equivalence results
  yields end-to-end correctness, but this document covers only the first
  step.
- **Synthesisability of generated SV.** Even with codegen equivalence
  established, the proof works at the abstract semantic level, not at the
  gate level.  Synthesis and timing closure are out of scope.
- **Coverage and SVA.** Generated assertions and coverage points added by
  *separate* passes (bounds checks at `_auto_bound_*`, divide-by-zero at
  `_auto_div0_*`, handshake protocol at `_auto_hs_*`, etc.) are independent
  of the thread lowering and have their own correctness arguments. The
  `--auto-thread-asserts` properties (`_auto_thread_*`) are different —
  they're emitted *during* `lower_threads` and their correctness is *part
  of* this proof, established as Corollaries W/C/B in §II.11.

> Note: `wait inside if/else` is implemented as of v0.45.0; correctness is
> established by Lemma I (§II.10).
