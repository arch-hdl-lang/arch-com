# Enhancement: Cross-module dead-skid comb-feedback lint + Thread Map visualization

**Date:** 2026-05-31  
**Status:** Proposal — ready for implementation after SFG Phase B scaffolding exists  
**Relates to:** #245 (dead-skid lint), `ideas/2026-05-28-signal-flow-graph.md` (SFG roadmap),
  PR #483 (thread map HTML), PR #470 (SFG Phase A)

---

## Problem

Issue #245 and the SFG roadmap (ideas/2026-05-28-signal-flow-graph.md §Check 2)
both describe a dead-skid lint. The existing plan uses BFS through `CombBlock`
edges "starting from `writes_t`" — but this only works for intra-module comb
paths. **The arch-ibex repro that cost three implementation iterations was
entirely cross-module:**

```
thread (in TopModule)
  writes: alu_operand_a_o, alu_operand_b_o    ← output ports of TopModule

child inst (IbexAlu):
  comb: equal_to_zero_i = (adder_result == 0) ← function of alu_operand_*

thread reads: equal_to_zero_i                 ← IbexAlu output port, fed back to TopModule
```

During dead-skid cycles, the thread drops `alu_operand_*` to zero → the ALU
computes `0+0=0` → `equal_to_zero_i` asserts → the thread's guard fires
spuriously. This feedback path **crosses a module instantiation boundary** and
is invisible to the current intra-module SFG.

The existing `comb_graph.rs` already computes per-module `CombInfo`
(`comb_outputs` / `comb_dep_inputs`) for the sim settle-order analysis —
exactly the data needed to trace cross-module comb dependencies. This proposal
shows how to wire `CombInfo` into the dead-skid lint and surface the result in
the Thread Map HTML output.

---

## Background: what the current SFG plan covers (and doesn't)

The SFG ideas doc plans Check 2 as:

> 1. `writes_t` = all signals with a `DriveEdge` from any `ThreadState { thread_idx: t.idx }`.
> 2. **Comb-only reachability**: BFS through `CombBlock` edges only, starting from `writes_t`.
> 3. `reads_t` = all signals referenced in read position within thread states.
> 4. If `comb_reachable(writes_t) ∩ reads_t ≠ ∅`, emit a warning.

Step 2's BFS is scoped to the current module's `CombBlock` edges. It would
catch:

```arch
module M
  comb x = a + b; end comb   // x is comb function of a,b
  thread on clk ...
    a <= ...; b <= ...;       // thread writes a, b
    wait until x == 0;        // thread reads x  ← CAUGHT: x is comb(a,b) intra-module
  end thread
end module M
```

It would **miss**:

```arch
module M
  inst alu: IbexAlu clk <- clk; a <- alu_a; b <- alu_b; zero -> equal_to_zero; end inst
  thread on clk ...
    alu_a <= ...; alu_b <= ...;      // thread writes alu_a, alu_b (output ports)
    wait until equal_to_zero;        // thread reads equal_to_zero
  end thread
end module M

module IbexAlu
  comb equal_to_zero = (a + b == 0); end comb  // cross-module boundary
end module IbexAlu
```

Because `equal_to_zero` flows in through an `InstConn` edge, not a `CombBlock`
edge. The BFS stops at the module boundary.

---

## Proposed extension: hierarchical comb-reachability

### New function: `comb_reachable_hierarchical`

```rust
/// Starting from a set of signals (typically a thread's driven output ports),
/// BFS through both CombBlock edges (intra-module) AND InstConn edges that
/// represent outputs of a child module whose outputs are comb functions of
/// the supplied signals (cross-module).
///
/// Uses the per-module `CombInfo` already computed by `comb_graph::collect_comb_info`
/// to determine, for each child inst, whether any of its output ports are comb
/// functions of its input ports that connect to signals in the current reachable set.
pub fn comb_reachable_hierarchical(
    start: &HashSet<SignalId>,
    sfg: &SignalFlowGraph,
    comb_infos: &HashMap<String, CombInfo>, // keyed by module name
    insts: &[InstDecl],
) -> HashSet<SignalId> {
    let mut reachable = start.clone();
    let mut queue: VecDeque<SignalId> = start.iter().cloned().collect();

    while let Some(sig) = queue.pop_front() {
        // Step 1: intra-module comb propagation (existing SFG CombBlock edges)
        for edge in sfg.drives.iter().filter(|e| {
            e.source == sig && matches!(e.context, DriveContext::CombBlock(_))
        }) {
            if reachable.insert(edge.target) {
                queue.push_back(edge.target);
            }
        }

        // Step 2: cross-module propagation via child instance CombInfo
        for inst in insts {
            let Some(info) = comb_infos.get(&inst.module_name) else { continue };
            // sig is in the parent; does it connect to an input of this inst?
            for conn in inst.connections.iter().filter(|c| c.dir == ConnectDir::In) {
                if parent_signal_of_conn(conn) != sig { continue }
                let child_input_port = &conn.port_name;
                if !info.comb_dep_inputs.contains(child_input_port) { continue }
                // The child's comb block reads this input.
                // All of the child's comb_outputs are now reachable in the parent
                // (they flow back via output connections).
                for out_conn in inst.connections.iter().filter(|c| c.dir == ConnectDir::Out) {
                    if info.comb_outputs.contains(&out_conn.port_name) {
                        let parent_sig = parent_signal_of_conn(out_conn);
                        if reachable.insert(parent_sig) {
                            queue.push_back(parent_sig);
                        }
                    }
                }
            }
        }
    }
    reachable
}
```

This extends the BFS naturally: when a reachable signal feeds an input port of a
child instance, and that child's `CombInfo` records that the output ports are
combinationally dependent on that input, the output ports (mapped back to parent
wires through the instance's connection list) are added to the reachable set.

**Key invariant**: `CombInfo` is already computed per-module by
`comb_graph::collect_comb_info` during the sim settle-order analysis. It is
available at the point `arch check` runs; no new traversal is needed.

### Updated Check 2 algorithm

Replace the intra-module BFS with `comb_reachable_hierarchical`:

```rust
fn check_dead_skid_feedback(
    module: &ModuleDecl,
    sfg: &SignalFlowGraph,
    comb_infos: &HashMap<String, CombInfo>,
) -> Vec<CompileError> {
    let mut errors = Vec::new();
    for (thread_idx, thread) in module.threads().enumerate() {
        let writes_t: HashSet<SignalId> = sfg.drives.iter()
            .filter(|e| matches!(e.context, DriveContext::ThreadState { thread_idx: ti, .. } if ti == thread_idx))
            .map(|e| e.target)
            .collect();
        let reads_t: HashSet<SignalId> = collect_thread_reads(thread, sfg);
        let reachable = comb_reachable_hierarchical(
            &writes_t, sfg, comb_infos, &module.insts
        );
        for hazard_sig in reachable.intersection(&reads_t) {
            let path = find_comb_path(&writes_t, *hazard_sig, sfg, comb_infos, &module.insts);
            errors.push(make_dead_skid_warning(thread, *hazard_sig, path));
        }
    }
    errors
}
```

### Warning shape

```
warning[dead-skid-feedback]: thread `consumer` reads `equal_to_zero_i` which
  is a combinational function of `alu_operand_b_o` that `consumer` drives.
  During dead-skid cycles, `consumer`'s outputs fall to their default (0);
  `equal_to_zero_i` may assert spuriously.

  ╭─[TopModule.arch:42:14]
  42│   wait until equal_to_zero_i;
     ·              ──────┬──────
     ·                    ╰── reads here
  ╰────

  note: comb path: consumer writes `alu_operand_b_o`
        → IbexAlu inst `alu` input `b`
        → IbexAlu comb block → `equal_to_zero_i` output
        → parent wire `equal_to_zero_i`

  help: read the upstream input (`op_b_i`) directly instead of the
        routed combinational output, or suppress with
        `// arch: allow dead_skid_feedback` on the wait statement.
```

---

## Thread Map HTML integration

The just-shipped PR #483 added a `ThreadMap` / `ThreadMapState` structure and
an HTML renderer that shows state machine nodes alongside source line annotations.
This is the natural surface to make the dead-skid hazard **visible** rather than
just text-warned.

### Data model additions

```rust
// In src/thread_map.rs

#[derive(Debug, Clone)]
pub struct CombFeedbackHazard {
    /// Human-readable comb path from the thread output to the hazardous read.
    pub path_summary: Vec<String>,
    /// The signal name the thread reads at the end of the path.
    pub hazardous_read: String,
    /// Source span of the read site (for HTML highlighting).
    pub read_span: Span,
}

// Add to ThreadMapState:
pub hazards: Vec<CombFeedbackHazard>,  // non-empty = this state has dead-skid risk
```

### HTML rendering addition

In `render_html`, when a state has `hazards.len() > 0`:

- Add a `⚠` badge in the state chip column.
- Add a collapsible "Comb feedback paths" row under the state table entry
  listing each `path_summary` in a monospace block.
- In the source partition panel, add a `hazard` CSS class to source lines
  that overlap a `read_span`, highlighting them amber/red.

This transforms the Thread Map HTML from a pure documentation tool into an
actionable design-review checklist: open the HTML, scan for ⚠ badges, and
see exactly which source lines are at risk without running a simulation.

---

## Implementation plan

Three tightly-scoped PRs:

### PR 1 — Cross-module BFS in `comb_reachable_hierarchical` (~150 LoC)

| Sub-task | File(s) |
|----------|---------|
| Add `comb_reachable_hierarchical` using existing `CombInfo` from `comb_graph.rs` | `src/signal_flow.rs` |
| Thread read-set collector `collect_thread_reads` (walks `ThreadState` edges in read position) | `src/signal_flow.rs` |
| Unit tests: intra-module case, cross-module case (1-hop), cross-module case (2-hop through chain), non-hazardous case (thread reads upstream input, not routed comb output) | `tests/` |

No user-visible change yet — pure infrastructure.

### PR 2 — Dead-skid lint wired into `arch check` (~120 LoC)

| Sub-task | File(s) |
|----------|---------|
| `check_dead_skid_feedback` calling PR 1's BFS | `src/signal_flow.rs` or `src/typecheck.rs` |
| `make_dead_skid_warning` with path display and suppression annotation | `src/diagnostics.rs` |
| Wire into `arch check` after SFG Phase A runs | `src/typecheck.rs` |
| Integration tests: arch-ibex-style cross-module repro (mirrors the pitfall in #245), intra-module repro, suppression via `allow` comment | `tests/` |
| Update `doc/COMPILER_STATUS.md` with the new check | `doc/COMPILER_STATUS.md` |

Delivers the user-visible lint. Closes #245.

### PR 3 — Thread Map HTML hazard overlay (~80 LoC)

| Sub-task | File(s) |
|----------|---------|
| Add `CombFeedbackHazard` + `hazards: Vec<..>` to `ThreadMapState` | `src/thread_map.rs` |
| Populate `hazards` from the dead-skid check output before HTML rendering | `src/main.rs` (build/check command) |
| HTML: ⚠ badge on hazardous state chips, amber source line highlight, path summary row | `src/thread_map.rs::render_html` |
| Snapshot test covering HTML output with and without hazards | `tests/` |

---

## Acceptance criteria

- `arch check` on the arch-ibex `IbexMultdivFast` cross-module repro emits the dead-skid warning with the correct cross-module path.
- `arch check` on a design where the thread reads an upstream input (not the routed comb output) emits no warning.
- `arch build --thread-map` on a hazardous design renders ⚠ badges in the HTML; a non-hazardous design renders no badges.
- Existing `cargo test` suite continues to pass with no new false positives on the nic400 / NIC-400 test suite.
- Suppression via `// arch: allow dead_skid_feedback` on the `wait until` statement silences the warning.

---

## Why this enhancement, why now

1. **Highest-leverage unsolved bug class**: Issue #245 explicitly calls the dead-skid trap "the most expensive single class of bug" in the arch-ibex Phase A work. It cost three implementation iterations on `IbexMultdivFast`. The fix at source is always a 1-line change (read the upstream input instead of the routed comb output); the hard part is knowing you need to.

2. **Infrastructure is ready**: PR #470 shipped the SFG builder. PR #483 shipped the Thread Map HTML. Both are the direct prerequisites for this work. The timing is ideal.

3. **Cross-module gap is the critical missing piece**: The existing SFG plan's Check 2 BFS is intra-module only. The arch-ibex repro and the general NIC-400 class of designs (where threads drive bus channels into child arbiters/queues and read back status signals) are all cross-module. Implementing intra-module only would miss exactly the cases that hurt most.

4. **Visualization multiplies the value**: Surfacing the warning in the Thread Map HTML — a file users already open for design review — makes the hazard impossible to miss, even for designers who don't read compiler warning output carefully.

5. **Not a language change**: No new syntax, no semantic changes, no emitted SV changes. Pure diagnostic improvement. Ship risk is low.

---

## What this does NOT do

- Does not change the emitted SV or simulation semantics.
- Does not fix the underlying dead-skid behavior (that requires the thread timing fix from #306, which is a larger language-level change).
- Does not detect dead-skid through `always_ff` feedback (reg reads in the same cycle they're written — that's a different class and not a dead-skid issue).
- Does not trace deeper than 2-hop cross-module paths in the first implementation; a recursion limit is acceptable to avoid exponential traversal on deeply nested hierarchies.
