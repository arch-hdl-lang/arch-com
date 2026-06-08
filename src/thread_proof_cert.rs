//! Machine-readable thread-lowering proof certificate sidecars.
//!
//! This is intentionally a small JSON emitter over `thread_map::ThreadMap`.
//! The map is populated from the real lowered `ThreadFsmState` table after
//! lowering optimizations such as folded wait-exit assignments, so it is a
//! useful first Rust-side artifact for Lean/certificate tooling to consume.

use crate::thread_map::{
    ThreadMap, ThreadMapAssignment, ThreadMapGuardExpr, ThreadMapNatExpr, ThreadMapState,
    ThreadMapThread, ThreadMapTransition,
};
use std::collections::{HashMap, HashSet};

pub fn render_json(map: &ThreadMap) -> String {
    let mut out = String::new();
    out.push_str("{\n");
    out.push_str("  \"schema\": \"arch.thread_lowering_proof.v5\",\n");
    out.push_str("  \"modules\": [\n");
    for (mi, module) in map.modules.iter().enumerate() {
        if mi > 0 {
            out.push_str(",\n");
        }
        out.push_str("    {\n");
        push_json_field(&mut out, 6, "module_name", &module.module_name, true);
        push_json_field(
            &mut out,
            6,
            "generated_module_name",
            &module.generated_module_name,
            true,
        );
        out.push_str("      \"threads\": [\n");
        for (ti, thread) in module.threads.iter().enumerate() {
            if ti > 0 {
                out.push_str(",\n");
            }
            out.push_str("        {\n");
            push_json_field(&mut out, 10, "name", &thread.name, true);
            out.push_str(&format!("          \"index\": {},\n", thread.index));
            out.push_str(&format!("          \"once\": {},\n", thread.once));
            out.push_str("          \"states\": [\n");
            for (si, state) in thread.states.iter().enumerate() {
                if si > 0 {
                    out.push_str(",\n");
                }
                push_state(&mut out, state);
            }
            out.push_str("\n          ]\n");
            out.push_str("        }");
        }
        out.push_str("\n      ]\n");
        out.push_str("    }");
    }
    out.push_str("\n  ]\n");
    out.push_str("}\n");
    out
}

pub fn render_lean_checked(map: &ThreadMap) -> Result<String, String> {
    validate_lean_render_map(map)?;
    Ok(render_lean(map))
}

pub fn render_lean(map: &ThreadMap) -> String {
    render_lean_unchecked(map)
}

fn render_lean_unchecked(map: &ThreadMap) -> String {
    let mut ctx = LeanRenderCtx::default();
    let mut chunks = vec![
        "import ArchThreadLoweringProof.CountedWait".to_string(),
        "import ArchThreadLoweringProof.FoldedExit".to_string(),
        String::new(),
        "set_option linter.unusedSimpArgs false".to_string(),
        String::new(),
        "namespace Arch.ThreadLoweringProof.Generated".to_string(),
        "open Arch.ThreadLoweringProof.CountedWait".to_string(),
        String::new(),
        "def updateById (id : Nat) : Arch.ThreadLoweringProof.FoldedExit.Update :=".to_string(),
        "  fun store var => if var = id then id else store var".to_string(),
        String::new(),
    ];

    for module in &map.modules {
        for thread in &module.threads {
            chunks.push(render_lean_thread(
                &module.module_name,
                thread,
                &mut ctx.guard_ids,
                &mut ctx.nat_ids,
                &mut ctx.action_ids,
            ));
            chunks.push(String::new());
        }
    }

    let mut seq_ordinal = 0usize;
    for module in &map.modules {
        for thread in &module.threads {
            for state in &thread.states {
                if !state.emitted || state.seq_assignments.is_empty() {
                    continue;
                }
                chunks.push(render_seq_assignment_store_lean_proof(
                    &module.module_name,
                    thread,
                    state,
                    seq_ordinal,
                    &mut ctx,
                ));
                chunks.push(String::new());
                seq_ordinal += 1;
            }
        }
    }

    let mut folded_ordinal = 0usize;
    for module in &map.modules {
        for thread in &module.threads {
            for state in &thread.states {
                if state.folded_exit_updates.is_empty() {
                    continue;
                }
                chunks.push(render_folded_exit_lean_proof(
                    &module.module_name,
                    thread,
                    state,
                    folded_ordinal,
                    &mut ctx,
                ));
                chunks.push(String::new());
                folded_ordinal += 1;
            }
        }
    }

    chunks.push("end Arch.ThreadLoweringProof.Generated".to_string());
    chunks.push(String::new());
    chunks.join("\n")
}

#[derive(Default)]
struct LeanRenderCtx {
    guard_ids: SymbolTable,
    nat_ids: SymbolTable,
    action_ids: SymbolTable,
    update_ids: SymbolTable,
    var_ids: SymbolTable,
    value_ids: SymbolTable,
}

fn validate_lean_render_map(map: &ThreadMap) -> Result<(), String> {
    for module in &map.modules {
        for thread in &module.threads {
            validate_lean_render_thread(&module.module_name, thread)?;
        }
    }
    Ok(())
}

fn validate_lean_render_thread(module_name: &str, thread: &ThreadMapThread) -> Result<(), String> {
    let base = lean_ident(&format!("{}_{}_{}", module_name, thread.name, thread.index));
    validate_raw_state_table(&base, thread)?;
    let states: Vec<&ThreadMapState> = thread.states.iter().filter(|s| s.emitted).collect();
    if states.is_empty() {
        return Err(format!("{base}: certificate has no emitted states"));
    }

    let mut seen_indices = HashSet::new();
    let mut index_map = HashMap::new();
    for (compact, state) in states.iter().enumerate() {
        if !seen_indices.insert(state.index) {
            return Err(format!(
                "{base}: duplicate emitted state index {} ({})",
                state.index, state.state_name
            ));
        }
        index_map.insert(state.index, compact);
    }

    for state in &states {
        validate_source_next(
            &base,
            state,
            states.len(),
            &index_map,
            thread.once,
            &thread.states,
        )?;
        validate_counted_wait(&base, state)?;
        let source_transitions = if state.source_transitions.is_empty() {
            &state.transitions
        } else {
            &state.source_transitions
        };
        validate_transitions(
            &base,
            state,
            "source transition",
            source_transitions,
            &index_map,
            states.len(),
            thread.once,
            &thread.states,
        )?;
        validate_transitions(
            &base,
            state,
            "transition",
            &state.transitions,
            &index_map,
            states.len(),
            thread.once,
            &thread.states,
        )?;
        validate_role_shape(&base, state, source_transitions)?;
        validate_assignment_coverage(&base, state)?;
        validate_folded_exit_transition(&base, state)?;
    }

    Ok(())
}

fn validate_raw_state_table(base: &str, thread: &ThreadMapThread) -> Result<(), String> {
    for (raw_idx, state) in thread.states.iter().enumerate() {
        if state.index != raw_idx {
            return Err(format!(
                "{base}: raw state table is not contiguous at position {raw_idx}; found state index {} ({})",
                state.index, state.state_name
            ));
        }
        if state.source_transition_origin != "pre_fold_snapshot" {
            return Err(format!(
                "{base}: {} has unsupported source_transition_origin {:?}",
                state.state_name, state.source_transition_origin
            ));
        }
    }
    if let Some(first) = thread.states.first() {
        if first.index != 0 || !first.emitted {
            return Err(format!(
                "{base}: raw state 0 must be the first emitted compact state"
            ));
        }
    }
    Ok(())
}

fn validate_source_next(
    base: &str,
    state: &ThreadMapState,
    num_states: usize,
    index_map: &HashMap<usize, usize>,
    once: bool,
    raw_states: &[ThreadMapState],
) -> Result<(), String> {
    let compact_idx = *index_map.get(&state.index).ok_or_else(|| {
        format!(
            "{base}: emitted state {} ({}) is missing from compact index map",
            state.index, state.state_name
        )
    })?;
    let inferred = compact_next(compact_idx, num_states, once);
    let explicit = if let Some(compact) = index_map.get(&state.source_next_index) {
        *compact
    } else if is_once_terminal_raw_target(state.source_next_index, raw_states, once) {
        inferred
    } else {
        return Err(format!(
            "{base}: {} source_next targets non-emitted or unknown state {} ({})",
            state.state_name, state.source_next_index, state.source_next_name
        ));
    };
    if explicit != inferred {
        return Err(format!(
            "{base}: {} source_next compact state {}, expected natural compact next {}",
            state.state_name, explicit, inferred
        ));
    }
    Ok(())
}

fn validate_counted_wait(base: &str, state: &ThreadMapState) -> Result<(), String> {
    if state.role != "wait_cycles" {
        return Ok(());
    }
    let Some(count) = &state.wait_cycles_count else {
        return Err(format!(
            "{base}: {} wait_cycles state missing structured wait_cycles_count",
            state.state_name
        ));
    };
    if !count.chars().all(|ch| ch.is_ascii_digit()) {
        return Err(format!(
            "{base}: {} non-literal wait_cycles_count {:?}",
            state.state_name, count
        ));
    }
    Ok(())
}

fn validate_transitions(
    base: &str,
    state: &ThreadMapState,
    label: &str,
    transitions: &[ThreadMapTransition],
    index_map: &HashMap<usize, usize>,
    num_states: usize,
    once: bool,
    raw_states: &[ThreadMapState],
) -> Result<(), String> {
    for (idx, transition) in transitions.iter().enumerate() {
        if resolve_target_checked(state, transition, index_map, num_states, once, raw_states)
            .is_none()
        {
            return Err(format!(
                "{base}: {} {} targets non-emitted or unknown state {} ({})",
                state.state_name, label, transition.target_index, transition.target_name
            ));
        }
        if state.role != "wait_cycles" && transition.condition_guard.is_none() {
            return Err(format!(
                "{base}: {} {} {idx} missing structured condition_guard",
                state.state_name, label
            ));
        }
    }
    Ok(())
}

fn validate_role_shape(
    base: &str,
    state: &ThreadMapState,
    source_transitions: &[ThreadMapTransition],
) -> Result<(), String> {
    let lowered_count = state.transitions.len();
    let source_count = source_transitions.len();
    if state.role == "dispatch" {
        if lowered_count < 2 || source_count < 2 {
            return Err(format!(
                "{base}: {} dispatch state must have at least two source and lowered transitions",
                state.state_name
            ));
        }
    } else if lowered_count != 1 || source_count != 1 {
        return Err(format!(
            "{base}: {} non-dispatch state must have exactly one source and lowered transition",
            state.state_name
        ));
    }
    Ok(())
}

fn validate_assignment_coverage(base: &str, state: &ThreadMapState) -> Result<(), String> {
    if !state.seq_assignments.is_empty() && state.seq_assignments.len() != state.seq_updates.len() {
        return Err(format!(
            "{base}: {} seq_assignments only cover {}/{} seq_updates",
            state.state_name,
            state.seq_assignments.len(),
            state.seq_updates.len()
        ));
    }
    if !state.folded_exit_updates.is_empty()
        && state.folded_exit_assignments.len() != state.folded_exit_updates.len()
    {
        return Err(format!(
            "{base}: {} folded_exit_assignments only cover {}/{} folded_exit_updates",
            state.state_name,
            state.folded_exit_assignments.len(),
            state.folded_exit_updates.len()
        ));
    }
    Ok(())
}

fn validate_folded_exit_transition(base: &str, state: &ThreadMapState) -> Result<(), String> {
    if state.folded_exit_updates.is_empty() {
        return Ok(());
    }
    if let Some(transition) = state.transitions.first() {
        if transition.condition_guard.is_none() {
            return Err(format!(
                "{base}: {} folded-exit transition missing structured condition_guard",
                state.state_name
            ));
        }
    }
    Ok(())
}

#[derive(Default)]
struct SymbolTable {
    ids: HashMap<String, usize>,
}

impl SymbolTable {
    fn id(&mut self, value: &str) -> usize {
        if let Some(id) = self.ids.get(value) {
            return *id;
        }
        let id = self.ids.len();
        self.ids.insert(value.to_string(), id);
        id
    }
}

#[derive(Clone)]
struct LeanArm {
    idx: usize,
    actions: String,
    source_control: String,
    fsm_control: String,
    target: usize,
}

fn render_lean_thread(
    module_name: &str,
    thread: &ThreadMapThread,
    guard_ids: &mut SymbolTable,
    nat_ids: &mut SymbolTable,
    action_ids: &mut SymbolTable,
) -> String {
    let base = lean_ident(&format!("{}_{}_{}", module_name, thread.name, thread.index));
    let states: Vec<&ThreadMapState> = thread.states.iter().filter(|s| s.emitted).collect();
    assert!(
        !states.is_empty(),
        "{base}: certificate has no emitted states"
    );
    let index_map: HashMap<usize, usize> = states
        .iter()
        .enumerate()
        .map(|(compact, state)| (state.index, compact))
        .collect();
    let num_states = states.len();
    let has_dispatch = states.iter().any(|state| state.role == "dispatch");

    let arms: Vec<LeanArm> = states
        .iter()
        .enumerate()
        .map(|(idx, state)| {
            let source_next =
                source_next_for_state(state, num_states, &index_map, thread.once, &thread.states);
            let source_transitions = if state.source_transitions.is_empty() {
                &state.transitions
            } else {
                &state.source_transitions
            };
            let source_control = lean_control_for_state(
                state,
                source_transitions,
                &index_map,
                guard_ids,
                nat_ids,
                source_next,
                num_states,
                thread.once,
                &thread.states,
            );
            let fsm_control = lean_control_for_state(
                state,
                &state.transitions,
                &index_map,
                guard_ids,
                nat_ids,
                source_next,
                num_states,
                thread.once,
                &thread.states,
            );
            let lowered = state.transitions.first();
            let has_guarded_lowered = lowered
                .map(|tr| {
                    state.role != "dispatch"
                        && is_guarded_non_dispatch_transition(
                            state,
                            tr,
                            &index_map,
                            source_next,
                            guard_ids,
                            nat_ids,
                        )
                })
                .unwrap_or(false);
            let has_jump_lowered = lowered
                .map(|tr| {
                    state.role != "dispatch"
                        && is_jump_non_dispatch_transition(
                            state,
                            tr,
                            &index_map,
                            source_next,
                            guard_ids,
                            nat_ids,
                            num_states,
                            thread.once,
                            &thread.states,
                        )
                })
                .unwrap_or(false);
            let mut target =
                target_for_state(state, num_states, &index_map, thread.once, &thread.states);
            if state.role == "dispatch" || has_guarded_lowered || has_jump_lowered {
                target = source_next;
            }
            LeanArm {
                idx,
                actions: lean_action_list_for_state(state, action_ids),
                source_control,
                fsm_control,
                target,
            }
        })
        .collect();

    let mut source_lines = vec![
        format!("def {base}Source : SourceThread :="),
        format!("  {{ numStates := {num_states}"),
        format!("    once := {}", if thread.once { "true" } else { "false" }),
        "    state := fun pc =>".to_string(),
    ];
    for arm in &arms {
        let prefix = if arm.idx == 0 {
            "      if"
        } else {
            "      else if"
        };
        source_lines.push(format!("{prefix} pc = {} then", arm.idx));
        source_lines.push(format!(
            "        {{ actions := {}, control := {} }}",
            arm.actions, arm.source_control
        ));
    }
    source_lines.push("      else".to_string());
    source_lines.push("        { actions := [], control := Control.advance } }".to_string());

    let mut fsm_lines = vec![
        format!("def {base}Fsm : LoweredFsm :="),
        "  { state := fun pc =>".to_string(),
    ];
    for arm in &arms {
        let prefix = if arm.idx == 0 {
            "      if"
        } else {
            "      else if"
        };
        fsm_lines.push(format!("{prefix} pc = {} then", arm.idx));
        fsm_lines.push(format!(
            "        {{ actions := {}, control := {}, target := {} }}",
            arm.actions, arm.fsm_control, arm.target
        ));
    }
    fsm_lines.push("      else".to_string());
    fsm_lines.push(format!(
        "        {{ actions := [], control := Control.advance, target := sourceNext {base}Source pc }} }}"
    ));

    let cert_tactic = cert_field_tactic(&base, num_states, false);
    let dispatch_cert_tactic = cert_field_tactic(&base, num_states, has_dispatch);
    let proof = vec![
        format!("example : LoweringCertifies {base}Source {base}Fsm := by"),
        "  refine".to_string(),
        "    { actions_ok := ?_".to_string(),
        "      control_ok := ?_".to_string(),
        "      dispatch_branches_ok := ?_".to_string(),
        "      target_ok := ?_ }".to_string(),
        "  · intro pc".to_string(),
        indent_block(&cert_tactic, 4),
        "  · intro pc".to_string(),
        indent_block(&cert_tactic, 4),
        "  · intro pc".to_string(),
        indent_block(&dispatch_cert_tactic, 4),
        "  · intro pc".to_string(),
        indent_block(&cert_tactic, 4),
        String::new(),
        format!("example : StepEffectFaithful {base}Source {base}Fsm :="),
        "  step_effect_faithful (by".to_string(),
        "    refine".to_string(),
        "      { actions_ok := ?_".to_string(),
        "        control_ok := ?_".to_string(),
        "        dispatch_branches_ok := ?_".to_string(),
        "        target_ok := ?_ }".to_string(),
        "    · intro pc".to_string(),
        indent_block(&cert_tactic, 6),
        "    · intro pc".to_string(),
        indent_block(&cert_tactic, 6),
        "    · intro pc".to_string(),
        indent_block(&dispatch_cert_tactic, 6),
        "    · intro pc".to_string(),
        indent_block(&cert_tactic, 6),
        "  )".to_string(),
        String::new(),
        format!("example (inputs : Nat -> Env) (natInputs : Nat -> NatEnv) (cfg0 : Config) :"),
        format!(
            "    forall t, sourceTraceObs {base}Source inputs natInputs cfg0 t = fsmTraceObs {base}Fsm inputs natInputs cfg0 t :="
        ),
        "  trace_equiv (by".to_string(),
        "    refine".to_string(),
        "      { actions_ok := ?_".to_string(),
        "        control_ok := ?_".to_string(),
        "        dispatch_branches_ok := ?_".to_string(),
        "        target_ok := ?_ }".to_string(),
        "    · intro pc".to_string(),
        indent_block(&cert_tactic, 6),
        "    · intro pc".to_string(),
        indent_block(&cert_tactic, 6),
        "    · intro pc".to_string(),
        indent_block(&dispatch_cert_tactic, 6),
        "    · intro pc".to_string(),
        indent_block(&cert_tactic, 6),
        "  ) inputs natInputs cfg0".to_string(),
    ];

    [
        source_lines,
        vec![String::new()],
        fsm_lines,
        vec![String::new()],
        proof,
    ]
    .concat()
    .join("\n")
}

fn push_state(out: &mut String, state: &ThreadMapState) {
    out.push_str("            {\n");
    out.push_str(&format!("              \"index\": {},\n", state.index));
    push_json_field(out, 14, "state_name", &state.state_name, true);
    push_json_field(out, 14, "role", &state.role, true);
    out.push_str(&format!("              \"emitted\": {},\n", state.emitted));
    out.push_str(&format!(
        "              \"source_next_index\": {},\n",
        state.source_next_index
    ));
    push_json_field(out, 14, "source_next_name", &state.source_next_name, true);
    out.push_str("              \"labels\": [");
    for (i, label) in state.labels.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        push_json_string(out, label);
    }
    out.push_str("],\n");
    out.push_str("              \"wait_cycles_count\": ");
    if let Some(count) = &state.wait_cycles_count {
        push_json_string(out, count);
    } else {
        out.push_str("null");
    }
    out.push_str(",\n");
    out.push_str("              \"seq_updates\": [");
    push_json_string_array(out, &state.seq_updates);
    out.push_str("],\n");
    out.push_str("              \"seq_assignments\": [");
    push_assignments(out, &state.seq_assignments);
    out.push_str("],\n");
    out.push_str("              \"folded_exit_updates\": [");
    push_json_string_array(out, &state.folded_exit_updates);
    out.push_str("],\n");
    out.push_str("              \"folded_exit_assignments\": [");
    push_assignments(out, &state.folded_exit_assignments);
    out.push_str("],\n");
    out.push_str("              \"source_transitions\": [");
    push_transitions(out, &state.source_transitions);
    out.push_str("],\n");
    push_json_field(
        out,
        14,
        "source_transition_origin",
        &state.source_transition_origin,
        true,
    );
    out.push_str("              \"transitions\": [");
    push_transitions(out, &state.transitions);
    out.push_str("]\n");
    out.push_str("            }");
}

fn render_folded_exit_lean_proof(
    module_name: &str,
    thread: &ThreadMapThread,
    state: &ThreadMapState,
    ordinal: usize,
    ctx: &mut LeanRenderCtx,
) -> String {
    let base = lean_ident(&format!(
        "{}_{}_{}_{}_folded_{}",
        module_name, thread.name, thread.index, state.state_name, ordinal
    ));
    let fe = "Arch.ThreadLoweringProof.FoldedExit";
    let guard = state
        .transitions
        .first()
        .map(|tr| {
            render_guard_expr_for_namespace(
                &transition_guard_expr(tr, &mut ctx.guard_ids, &mut ctx.nat_ids),
                fe,
            )
        })
        .unwrap_or_else(|| format!("{fe}.GuardExpr.trueLit"));
    let updates = lean_update_list(
        &state.folded_exit_updates,
        &state.folded_exit_assignments,
        &mut ctx.update_ids,
        &mut ctx.var_ids,
        &mut ctx.value_ids,
    );

    let mut lines = vec![
        format!("def {base}Source : {fe}.SourceThread :="),
        "  { numStates := 2".to_string(),
        "    state := fun pc =>".to_string(),
        "      if pc = 0 then".to_string(),
        "        { updates := []".to_string(),
        format!("          exitUpdates := {updates}"),
        format!("          control := {fe}.Control.waitUntil {guard} }}"),
        "      else".to_string(),
        "        { updates := []".to_string(),
        "          exitUpdates := []".to_string(),
        format!("          control := {fe}.Control.advance }} }}"),
        String::new(),
        format!("def {base}Fsm : {fe}.LoweredFsm :="),
        "  { state := fun pc =>".to_string(),
        "      if pc = 0 then".to_string(),
        "        { updates := []".to_string(),
        format!("          foldedExitUpdates := {updates}"),
        format!("          control := {fe}.Control.waitUntil {guard}"),
        "          target := 1".to_string(),
        "          foldedTarget := some 1 }".to_string(),
        "      else".to_string(),
        "        { updates := []".to_string(),
        "          foldedExitUpdates := []".to_string(),
        format!("          control := {fe}.Control.advance"),
        format!("          target := {fe}.sourceNext {base}Source pc"),
        "          foldedTarget := none } }".to_string(),
        String::new(),
        format!("example : {fe}.LoweringCertifies {base}Source {base}Fsm := by"),
        "  refine".to_string(),
        "    { updates_ok := ?_".to_string(),
        "      control_ok := ?_".to_string(),
        "      target_ok := ?_".to_string(),
        "      folded_updates_ok := ?_".to_string(),
        "      folded_target_ok := ?_".to_string(),
        "      folded_target_some_ok := ?_ }".to_string(),
        "  · intro pc".to_string(),
        format!("    by_cases h : pc = 0 <;> simp [{base}Source, {base}Fsm, h]"),
        "  · intro pc".to_string(),
        format!("    by_cases h : pc = 0 <;> simp [{base}Source, {base}Fsm, h]"),
        "  · intro pc".to_string(),
        format!("    by_cases h : pc = 0 <;> simp [{base}Source, {base}Fsm, {fe}.sourceNext, h]"),
        "  · intro pc".to_string(),
        format!("    by_cases h : pc = 0 <;> simp [{base}Source, {base}Fsm, h]"),
        "  · intro pc hupdates".to_string(),
        format!(
            "    by_cases h : pc = 0 <;> simp [{base}Source, {base}Fsm, {fe}.sourceNext, h] at *"
        ),
        "  · intro pc target hfold".to_string(),
        format!(
            "    by_cases h : pc = 0 <;> simp [{base}Source, {base}Fsm, {fe}.sourceNext, h] at *"
        ),
        "    exact hfold.symm".to_string(),
        String::new(),
        format!("example (env : {fe}.Env) (natEnv : {fe}.NatEnv) (cfg : {fe}.Config) :"),
        format!(
            "    {fe}.sourceStep {base}Source env natEnv cfg = {fe}.fsmStep {base}Fsm env natEnv cfg :="
        ),
        format!("  {fe}.one_step_equiv (by"),
        "    refine".to_string(),
        "      { updates_ok := ?_".to_string(),
        "        control_ok := ?_".to_string(),
        "        target_ok := ?_".to_string(),
        "        folded_updates_ok := ?_".to_string(),
        "        folded_target_ok := ?_".to_string(),
        "        folded_target_some_ok := ?_ }".to_string(),
        format!("    · intro pc; by_cases h : pc = 0 <;> simp [{base}Source, {base}Fsm, h]"),
        format!("    · intro pc; by_cases h : pc = 0 <;> simp [{base}Source, {base}Fsm, h]"),
        format!("    · intro pc; by_cases h : pc = 0 <;> simp [{base}Source, {base}Fsm, {fe}.sourceNext, h]"),
        format!("    · intro pc; by_cases h : pc = 0 <;> simp [{base}Source, {base}Fsm, h]"),
        format!("    · intro pc hupdates; by_cases h : pc = 0 <;> simp [{base}Source, {base}Fsm, {fe}.sourceNext, h] at *"),
        format!("    · intro pc target hfold; by_cases h : pc = 0 <;> simp [{base}Source, {base}Fsm, {fe}.sourceNext, h] at *; exact hfold.symm"),
        "  ) env natEnv cfg".to_string(),
    ];

    let mut seen_targets = HashSet::new();
    let mut final_writes = Vec::new();
    for assignment in state.folded_exit_assignments.iter().rev() {
        if seen_targets.insert(assignment.target.clone()) {
            final_writes.push(assignment);
        }
    }
    final_writes.reverse();

    for assignment in final_writes {
        let target = ctx.var_ids.id(&assignment.target);
        let value = ctx.value_ids.id(&assignment.value);
        lines.extend([
            String::new(),
            format!("example (env : {fe}.Env) (natEnv : {fe}.NatEnv) (store : {fe}.Store)"),
            format!("    (hguard : {fe}.GuardExpr.eval env {guard} natEnv = true) :"),
            format!("    let cfg : {fe}.Config := {{ pc := 0, store := store }}"),
            format!(
                "    (({fe}.sourceStep {base}Source env natEnv cfg).store {target} = {value}) /\\"
            ),
            format!(
                "      (({fe}.fsmStep {base}Fsm env natEnv cfg).store {target} = {value}) := by"
            ),
            format!("  simp [{fe}.sourceStep, {fe}.fsmStep, {base}Source, {base}Fsm,"),
            format!("    {fe}.sourceAdvanceTo, {fe}.fsmAdvanceTo, {fe}.applyUpdates, {fe}.setVar, hguard]"),
        ]);
    }

    lines.join("\n")
}

fn render_seq_assignment_store_lean_proof(
    module_name: &str,
    thread: &ThreadMapThread,
    state: &ThreadMapState,
    ordinal: usize,
    ctx: &mut LeanRenderCtx,
) -> String {
    let base = lean_ident(&format!(
        "{}_{}_{}_{}_seq_{}",
        module_name, thread.name, thread.index, state.state_name, ordinal
    ));
    let fe = "Arch.ThreadLoweringProof.FoldedExit";
    let updates = state
        .seq_assignments
        .iter()
        .map(|assignment| {
            let target = ctx.var_ids.id(&assignment.target);
            let value = ctx.value_ids.id(&assignment.value);
            format!("{fe}.setVar {target} {value}")
        })
        .collect::<Vec<_>>()
        .join(", ");

    let mut lines = vec![
        format!("def {base}Updates : List {fe}.Update :="),
        format!("  [{updates}]"),
    ];

    let mut seen_targets = HashSet::new();
    let mut final_writes = Vec::new();
    for assignment in state.seq_assignments.iter().rev() {
        if seen_targets.insert(assignment.target.clone()) {
            final_writes.push(assignment);
        }
    }
    final_writes.reverse();

    for assignment in final_writes {
        let target = ctx.var_ids.id(&assignment.target);
        let value = ctx.value_ids.id(&assignment.value);
        lines.extend([
            String::new(),
            format!("example (store : {fe}.Store) :"),
            format!("    {fe}.applyUpdates {base}Updates store {target} = {value} := by"),
            format!("  simp [{base}Updates, {fe}.applyUpdates, {fe}.setVar]"),
        ]);
    }

    lines.join("\n")
}

fn lean_ident(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() || out.as_bytes()[0].is_ascii_digit() {
        out = format!("cert_{out}");
    }
    out
}

fn render_nat_expr(expr: &ThreadMapNatExpr, namespace: Option<&str>) -> String {
    let prefix = namespace
        .map(|ns| format!("{ns}.NatExpr"))
        .unwrap_or_else(|| "NatExpr".to_string());
    match expr {
        ThreadMapNatExpr::Var(name) => format!("({prefix}.var {})", lean_symbol_id(name)),
        ThreadMapNatExpr::Const(value) => format!("({prefix}.const {value})"),
    }
}

fn render_guard_expr(expr: &ThreadMapGuardExpr) -> String {
    render_guard_expr_for_namespace(expr, "")
}

fn render_guard_expr_for_namespace(expr: &ThreadMapGuardExpr, namespace: &str) -> String {
    let guard_prefix = if namespace.is_empty() {
        "GuardExpr".to_string()
    } else {
        format!("{namespace}.GuardExpr")
    };
    let nat_namespace = (!namespace.is_empty()).then_some(namespace);
    match expr {
        ThreadMapGuardExpr::Atom(name) => format!("({guard_prefix}.atom {})", lean_symbol_id(name)),
        ThreadMapGuardExpr::True => format!("{guard_prefix}.trueLit"),
        ThreadMapGuardExpr::False => format!("{guard_prefix}.falseLit"),
        ThreadMapGuardExpr::Not(inner) => {
            format!(
                "({guard_prefix}.neg {})",
                render_guard_expr_for_namespace(inner, namespace)
            )
        }
        ThreadMapGuardExpr::And(lhs, rhs) => format!(
            "({guard_prefix}.and {} {})",
            render_guard_expr_for_namespace(lhs, namespace),
            render_guard_expr_for_namespace(rhs, namespace)
        ),
        ThreadMapGuardExpr::Or(lhs, rhs) => format!(
            "({guard_prefix}.or {} {})",
            render_guard_expr_for_namespace(lhs, namespace),
            render_guard_expr_for_namespace(rhs, namespace)
        ),
        ThreadMapGuardExpr::Lt(lhs, rhs) => format!(
            "({guard_prefix}.lt {} {})",
            render_nat_expr(lhs, nat_namespace),
            render_nat_expr(rhs, nat_namespace)
        ),
        ThreadMapGuardExpr::Ge(lhs, rhs) => format!(
            "({guard_prefix}.ge {} {})",
            render_nat_expr(lhs, nat_namespace),
            render_nat_expr(rhs, nat_namespace)
        ),
        ThreadMapGuardExpr::Eq(lhs, rhs) => format!(
            "({guard_prefix}.eq {} {})",
            render_nat_expr(lhs, nat_namespace),
            render_nat_expr(rhs, nat_namespace)
        ),
        ThreadMapGuardExpr::Ne(lhs, rhs) => format!(
            "({guard_prefix}.ne {} {})",
            render_nat_expr(lhs, nat_namespace),
            render_nat_expr(rhs, nat_namespace)
        ),
    }
}

fn transition_guard_expr(
    transition: &ThreadMapTransition,
    guard_ids: &mut SymbolTable,
    nat_ids: &mut SymbolTable,
) -> ThreadMapGuardExpr {
    if let Some(guard) = &transition.condition_guard {
        return intern_guard_expr(guard, guard_ids, nat_ids);
    }
    panic!(
        "transition {:?} -> {} missing structured condition_guard",
        transition.condition, transition.target_name
    )
}

fn intern_guard_expr(
    expr: &ThreadMapGuardExpr,
    guard_ids: &mut SymbolTable,
    nat_ids: &mut SymbolTable,
) -> ThreadMapGuardExpr {
    match expr {
        ThreadMapGuardExpr::Atom(name) => ThreadMapGuardExpr::Atom(guard_ids.id(name).to_string()),
        ThreadMapGuardExpr::True => ThreadMapGuardExpr::True,
        ThreadMapGuardExpr::False => ThreadMapGuardExpr::False,
        ThreadMapGuardExpr::Not(inner) => {
            ThreadMapGuardExpr::Not(Box::new(intern_guard_expr(inner, guard_ids, nat_ids)))
        }
        ThreadMapGuardExpr::And(lhs, rhs) => ThreadMapGuardExpr::And(
            Box::new(intern_guard_expr(lhs, guard_ids, nat_ids)),
            Box::new(intern_guard_expr(rhs, guard_ids, nat_ids)),
        ),
        ThreadMapGuardExpr::Or(lhs, rhs) => ThreadMapGuardExpr::Or(
            Box::new(intern_guard_expr(lhs, guard_ids, nat_ids)),
            Box::new(intern_guard_expr(rhs, guard_ids, nat_ids)),
        ),
        ThreadMapGuardExpr::Lt(lhs, rhs) => {
            ThreadMapGuardExpr::Lt(intern_nat_expr(lhs, nat_ids), intern_nat_expr(rhs, nat_ids))
        }
        ThreadMapGuardExpr::Ge(lhs, rhs) => {
            ThreadMapGuardExpr::Ge(intern_nat_expr(lhs, nat_ids), intern_nat_expr(rhs, nat_ids))
        }
        ThreadMapGuardExpr::Eq(lhs, rhs) => {
            ThreadMapGuardExpr::Eq(intern_nat_expr(lhs, nat_ids), intern_nat_expr(rhs, nat_ids))
        }
        ThreadMapGuardExpr::Ne(lhs, rhs) => {
            ThreadMapGuardExpr::Ne(intern_nat_expr(lhs, nat_ids), intern_nat_expr(rhs, nat_ids))
        }
    }
}

fn intern_nat_expr(expr: &ThreadMapNatExpr, nat_ids: &mut SymbolTable) -> ThreadMapNatExpr {
    match expr {
        ThreadMapNatExpr::Var(name) => ThreadMapNatExpr::Var(nat_ids.id(name).to_string()),
        ThreadMapNatExpr::Const(value) => ThreadMapNatExpr::Const(*value),
    }
}

fn lean_symbol_id(value: &str) -> String {
    value.parse::<usize>().unwrap_or(0).to_string()
}

fn transition_is_unconditional(
    transition: &ThreadMapTransition,
    guard_ids: &mut SymbolTable,
    nat_ids: &mut SymbolTable,
) -> bool {
    matches!(
        transition_guard_expr(transition, guard_ids, nat_ids),
        ThreadMapGuardExpr::True
    )
}

fn lean_control_for_state(
    state: &ThreadMapState,
    transitions: &[ThreadMapTransition],
    index_map: &HashMap<usize, usize>,
    guard_ids: &mut SymbolTable,
    nat_ids: &mut SymbolTable,
    source_next: usize,
    num_states: usize,
    once: bool,
    raw_states: &[ThreadMapState],
) -> String {
    match state.role.as_str() {
        "wait_until" => {
            let guard = transitions
                .first()
                .map(|tr| render_guard_expr(&transition_guard_expr(tr, guard_ids, nat_ids)))
                .unwrap_or_else(|| "GuardExpr.trueLit".to_string());
            format!("Control.waitUntil {guard}")
        }
        "wait_cycles" => {
            let count = state.wait_cycles_count.as_deref().unwrap_or_else(|| {
                panic!(
                    "{}: wait_cycles state missing structured wait_cycles_count",
                    state.state_name
                )
            });
            assert!(
                count.chars().all(|ch| ch.is_ascii_digit()),
                "{}: non-literal wait_cycles_count {:?}",
                state.state_name,
                count
            );
            format!("Control.waitCycles {count}")
        }
        "dispatch" => {
            let branches = transitions
                .iter()
                .map(|tr| {
                    let guard = render_guard_expr(&transition_guard_expr(tr, guard_ids, nat_ids));
                    let target = resolve_target(state, tr, index_map, num_states, once, raw_states);
                    format!("{{ guard := {guard}, target := {target} }}")
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("Control.dispatch [{branches}]")
        }
        _ => {
            if transitions.len() == 1 {
                let tr = &transitions[0];
                let target = resolve_target(state, tr, index_map, num_states, once, raw_states);
                if transition_is_unconditional(tr, guard_ids, nat_ids) {
                    if target != source_next {
                        return format!("Control.jump {target}");
                    }
                } else {
                    let guard = render_guard_expr(&transition_guard_expr(tr, guard_ids, nat_ids));
                    return format!("Control.guarded {guard} {target}");
                }
            }
            "Control.advance".to_string()
        }
    }
}

fn resolve_target(
    state: &ThreadMapState,
    transition: &ThreadMapTransition,
    index_map: &HashMap<usize, usize>,
    num_states: usize,
    once: bool,
    raw_states: &[ThreadMapState],
) -> usize {
    resolve_target_checked(state, transition, index_map, num_states, once, raw_states)
        .unwrap_or_else(|| {
            panic!(
                "{}: transition targets non-emitted or unknown state {} ({})",
                state.state_name, transition.target_index, transition.target_name
            )
        })
}

fn resolve_target_checked(
    state: &ThreadMapState,
    transition: &ThreadMapTransition,
    index_map: &HashMap<usize, usize>,
    num_states: usize,
    once: bool,
    raw_states: &[ThreadMapState],
) -> Option<usize> {
    if let Some(compact) = index_map.get(&transition.target_index) {
        Some(*compact)
    } else if is_once_terminal_raw_target(transition.target_index, raw_states, once) {
        let compact_idx = *index_map.get(&state.index)?;
        Some(compact_next(compact_idx, num_states, once))
    } else {
        None
    }
}

fn source_next_for_state(
    state: &ThreadMapState,
    num_states: usize,
    index_map: &HashMap<usize, usize>,
    once: bool,
    raw_states: &[ThreadMapState],
) -> usize {
    let compact_idx = *index_map.get(&state.index).unwrap_or(&0);
    let inferred = compact_next(compact_idx, num_states, once);
    let explicit = if let Some(compact) = index_map.get(&state.source_next_index) {
        *compact
    } else if is_once_terminal_raw_target(state.source_next_index, raw_states, once) {
        inferred
    } else {
        panic!(
            "{}: source_next targets non-emitted or unknown state {} ({})",
            state.state_name, state.source_next_index, state.source_next_name
        );
    };
    assert_eq!(
        explicit, inferred,
        "{}: source_next compact state {}, expected natural compact next {}",
        state.state_name, explicit, inferred
    );
    explicit
}

fn target_for_state(
    state: &ThreadMapState,
    num_states: usize,
    index_map: &HashMap<usize, usize>,
    once: bool,
    raw_states: &[ThreadMapState],
) -> usize {
    state
        .transitions
        .first()
        .map(|tr| resolve_target(state, tr, index_map, num_states, once, raw_states))
        .unwrap_or_else(|| {
            let compact_idx = *index_map.get(&state.index).unwrap_or(&0);
            compact_next(compact_idx, num_states, once)
        })
}

fn is_once_terminal_raw_target(
    raw_target: usize,
    raw_states: &[ThreadMapState],
    once: bool,
) -> bool {
    once && raw_states
        .get(raw_target)
        .map(|state| {
            !state.emitted
                && state.source_next_index == raw_target
                && state
                    .source_transitions
                    .iter()
                    .all(|transition| transition.target_index == raw_target)
        })
        .unwrap_or(false)
}

fn compact_next(idx: usize, num_states: usize, once: bool) -> usize {
    if idx + 1 < num_states {
        idx + 1
    } else if once {
        idx
    } else {
        0
    }
}

fn is_guarded_non_dispatch_transition(
    state: &ThreadMapState,
    transition: &ThreadMapTransition,
    _index_map: &HashMap<usize, usize>,
    _source_next: usize,
    guard_ids: &mut SymbolTable,
    nat_ids: &mut SymbolTable,
) -> bool {
    state.role != "dispatch"
        && state.role != "wait_cycles"
        && !transition_is_unconditional(transition, guard_ids, nat_ids)
}

fn is_jump_non_dispatch_transition(
    state: &ThreadMapState,
    transition: &ThreadMapTransition,
    index_map: &HashMap<usize, usize>,
    source_next: usize,
    guard_ids: &mut SymbolTable,
    nat_ids: &mut SymbolTable,
    num_states: usize,
    once: bool,
    raw_states: &[ThreadMapState],
) -> bool {
    state.role != "dispatch"
        && state.role != "wait_cycles"
        && resolve_target(state, transition, index_map, num_states, once, raw_states) != source_next
        && transition_is_unconditional(transition, guard_ids, nat_ids)
}

fn lean_action_list_for_state(state: &ThreadMapState, action_ids: &mut SymbolTable) -> String {
    let ids = if !state.seq_assignments.is_empty() {
        state
            .seq_assignments
            .iter()
            .map(|assignment| {
                action_ids.id(&format!("{} := {}", assignment.target, assignment.value))
            })
            .collect::<Vec<_>>()
    } else {
        state
            .seq_updates
            .iter()
            .map(|label| action_ids.id(label))
            .collect::<Vec<_>>()
    };
    lean_nat_list(&ids)
}

fn lean_update_list(
    labels: &[String],
    assignments: &[ThreadMapAssignment],
    update_ids: &mut SymbolTable,
    var_ids: &mut SymbolTable,
    value_ids: &mut SymbolTable,
) -> String {
    if !assignments.is_empty() {
        let fe = "Arch.ThreadLoweringProof.FoldedExit";
        let updates = assignments
            .iter()
            .map(|assignment| {
                let target = var_ids.id(&assignment.target);
                let value = value_ids.id(&assignment.value);
                format!("{fe}.setVar {target} {value}")
            })
            .collect::<Vec<_>>()
            .join(", ");
        return format!("[{updates}]");
    }
    let updates = labels
        .iter()
        .map(|label| format!("updateById {}", update_ids.id(label)))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{updates}]")
}

fn lean_nat_list(values: &[usize]) -> String {
    if values.is_empty() {
        "[]".to_string()
    } else {
        format!(
            "[{}]",
            values
                .iter()
                .map(|value| value.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

fn cert_field_tactic(base: &str, num_states: usize, include_dispatch_branches: bool) -> String {
    let extra = if include_dispatch_branches {
        ", dispatchBranches"
    } else {
        ""
    };
    if num_states == 0 {
        return format!("simp [{base}Source, {base}Fsm, sourceNext{extra}]");
    }

    fn simp_line(base: &str, extra: &str, hyps: &[String]) -> String {
        let hyp_args = hyps
            .iter()
            .map(|hyp| format!(", {hyp}"))
            .collect::<String>();
        format!("simp [{base}Source, {base}Fsm, sourceNext{extra}{hyp_args}]")
    }

    fn go(
        base: &str,
        extra: &str,
        idx: usize,
        num_states: usize,
        hyps: Vec<String>,
        depth: usize,
        lines: &mut Vec<String>,
    ) {
        let indent = "  ".repeat(depth);
        if idx >= num_states {
            lines.push(format!("{indent}{}", simp_line(base, extra, &hyps)));
            return;
        }
        let hyp = format!("h{idx}");
        lines.push(format!("{indent}by_cases {hyp} : pc = {idx}"));
        let mut with_hyp = hyps.clone();
        with_hyp.push(hyp.clone());
        lines.push(format!("{indent}· {}", simp_line(base, extra, &with_hyp)));
        lines.push(format!("{indent}·"));
        let mut without = hyps;
        without.push(hyp);
        go(base, extra, idx + 1, num_states, without, depth + 1, lines);
    }

    let mut lines = Vec::new();
    go(base, extra, 0, num_states, Vec::new(), 0, &mut lines);
    lines.join("\n")
}

fn indent_block(text: &str, spaces: usize) -> String {
    let prefix = " ".repeat(spaces);
    text.lines()
        .map(|line| {
            if line.is_empty() {
                String::new()
            } else {
                format!("{prefix}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn push_transitions(out: &mut String, transitions: &[ThreadMapTransition]) {
    for (i, tr) in transitions.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str("{\"condition\": ");
        push_json_string(out, &tr.condition);
        if let Some(guard) = &tr.condition_guard {
            out.push_str(", \"condition_guard\": ");
            push_guard_expr(out, guard);
        }
        out.push_str(&format!(
            ", \"target_index\": {}, \"target_name\": ",
            tr.target_index
        ));
        push_json_string(out, &tr.target_name);
        out.push('}');
    }
}

fn push_guard_expr(out: &mut String, guard: &ThreadMapGuardExpr) {
    out.push('{');
    match guard {
        ThreadMapGuardExpr::Atom(name) => {
            out.push_str("\"kind\":\"atom\",\"name\":");
            push_json_string(out, name);
        }
        ThreadMapGuardExpr::True => {
            out.push_str("\"kind\":\"true\"");
        }
        ThreadMapGuardExpr::False => {
            out.push_str("\"kind\":\"false\"");
        }
        ThreadMapGuardExpr::Not(expr) => {
            out.push_str("\"kind\":\"not\",\"expr\":");
            push_guard_expr(out, expr);
        }
        ThreadMapGuardExpr::And(lhs, rhs) => {
            out.push_str("\"kind\":\"and\",\"lhs\":");
            push_guard_expr(out, lhs);
            out.push_str(",\"rhs\":");
            push_guard_expr(out, rhs);
        }
        ThreadMapGuardExpr::Or(lhs, rhs) => {
            out.push_str("\"kind\":\"or\",\"lhs\":");
            push_guard_expr(out, lhs);
            out.push_str(",\"rhs\":");
            push_guard_expr(out, rhs);
        }
        ThreadMapGuardExpr::Lt(lhs, rhs) => {
            out.push_str("\"kind\":\"lt\",\"lhs\":");
            push_nat_expr(out, lhs);
            out.push_str(",\"rhs\":");
            push_nat_expr(out, rhs);
        }
        ThreadMapGuardExpr::Ge(lhs, rhs) => {
            out.push_str("\"kind\":\"ge\",\"lhs\":");
            push_nat_expr(out, lhs);
            out.push_str(",\"rhs\":");
            push_nat_expr(out, rhs);
        }
        ThreadMapGuardExpr::Eq(lhs, rhs) => {
            out.push_str("\"kind\":\"eq\",\"lhs\":");
            push_nat_expr(out, lhs);
            out.push_str(",\"rhs\":");
            push_nat_expr(out, rhs);
        }
        ThreadMapGuardExpr::Ne(lhs, rhs) => {
            out.push_str("\"kind\":\"ne\",\"lhs\":");
            push_nat_expr(out, lhs);
            out.push_str(",\"rhs\":");
            push_nat_expr(out, rhs);
        }
    }
    out.push('}');
}

fn push_nat_expr(out: &mut String, expr: &ThreadMapNatExpr) {
    out.push('{');
    match expr {
        ThreadMapNatExpr::Var(name) => {
            out.push_str("\"kind\":\"var\",\"name\":");
            push_json_string(out, name);
        }
        ThreadMapNatExpr::Const(value) => {
            out.push_str(&format!("\"kind\":\"const\",\"value\":{value}"));
        }
    }
    out.push('}');
}

fn push_assignments(out: &mut String, assignments: &[ThreadMapAssignment]) {
    for (i, assignment) in assignments.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str("{\"target\": ");
        push_json_string(out, &assignment.target);
        out.push_str(", \"value\": ");
        push_json_string(out, &assignment.value);
        out.push('}');
    }
}

fn push_json_string_array(out: &mut String, values: &[String]) {
    for (i, value) in values.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        push_json_string(out, value);
    }
}

fn push_json_field(out: &mut String, indent: usize, name: &str, value: &str, comma: bool) {
    out.push_str(&" ".repeat(indent));
    push_json_string(out, name);
    out.push_str(": ");
    push_json_string(out, value);
    if comma {
        out.push(',');
    }
    out.push('\n');
}

fn push_json_string(out: &mut String, value: &str) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Span;
    use crate::thread_map::{
        ThreadMapGuardExpr, ThreadMapModule, ThreadMapState, ThreadMapThread, ThreadMapTransition,
    };

    fn map_with_states(states: Vec<ThreadMapState>) -> ThreadMap {
        ThreadMap {
            modules: vec![ThreadMapModule {
                module_name: "M".to_string(),
                generated_module_name: "_M_threads".to_string(),
                span: Span::new(0, 1),
                threads: vec![ThreadMapThread {
                    name: "Worker".to_string(),
                    index: 0,
                    once: false,
                    span: Span::new(0, 1),
                    hazards: Vec::new(),
                    states,
                }],
            }],
        }
    }

    fn emitted_action_state(index: usize, source_next_index: usize) -> ThreadMapState {
        ThreadMapState {
            index,
            state_name: format!("_t0_S{index}_action"),
            role: "action".to_string(),
            emitted: true,
            span: Span::new(0, 1),
            labels: Vec::new(),
            source_next_index,
            source_next_name: format!("_t0_S{source_next_index}_action"),
            wait_cycles_count: None,
            seq_updates: Vec::new(),
            seq_assignments: Vec::new(),
            folded_exit_updates: Vec::new(),
            folded_exit_assignments: Vec::new(),
            source_transitions: Vec::new(),
            source_transition_origin: "pre_fold_snapshot".to_string(),
            transitions: vec![ThreadMapTransition {
                condition: "true".to_string(),
                condition_guard: Some(ThreadMapGuardExpr::True),
                target_index: source_next_index,
                target_name: format!("_t0_S{source_next_index}_action"),
            }],
        }
    }

    #[test]
    fn render_includes_folded_state_marker_and_resolved_transition() {
        let map = ThreadMap {
            modules: vec![ThreadMapModule {
                module_name: "M".to_string(),
                generated_module_name: "_M_threads".to_string(),
                span: Span::new(0, 1),
                threads: vec![ThreadMapThread {
                    name: "Worker".to_string(),
                    index: 0,
                    once: false,
                    span: Span::new(0, 1),
                    hazards: Vec::new(),
                    states: vec![
                        ThreadMapState {
                            index: 1,
                            state_name: "_t0_S1_wait_until".to_string(),
                            role: "wait_until".to_string(),
                            emitted: true,
                            span: Span::new(0, 1),
                            labels: vec!["wait until req".to_string()],
                            source_next_index: 3,
                            source_next_name: "_t0_S3_wait_cycles".to_string(),
                            wait_cycles_count: None,
                            seq_updates: Vec::new(),
                            seq_assignments: Vec::new(),
                            folded_exit_updates: vec!["done_r <= true".to_string()],
                            folded_exit_assignments: vec![ThreadMapAssignment {
                                target: "done_r".to_string(),
                                value: "true".to_string(),
                            }],
                            source_transitions: vec![ThreadMapTransition {
                                condition: "req".to_string(),
                                condition_guard: Some(ThreadMapGuardExpr::Atom("req".to_string())),
                                target_index: 3,
                                target_name: "_t0_S3_wait_cycles".to_string(),
                            }],
                            source_transition_origin: "pre_fold_snapshot".to_string(),
                            transitions: vec![ThreadMapTransition {
                                condition: "req".to_string(),
                                condition_guard: Some(ThreadMapGuardExpr::Atom("req".to_string())),
                                target_index: 3,
                                target_name: "_t0_S3_wait_cycles".to_string(),
                            }],
                        },
                        ThreadMapState {
                            index: 2,
                            state_name: "_t0_S2_action".to_string(),
                            role: "action".to_string(),
                            emitted: false,
                            span: Span::new(0, 1),
                            labels: Vec::new(),
                            source_next_index: 0,
                            source_next_name: "_t0_S0_wait_until".to_string(),
                            wait_cycles_count: None,
                            seq_updates: Vec::new(),
                            seq_assignments: Vec::new(),
                            folded_exit_updates: Vec::new(),
                            folded_exit_assignments: Vec::new(),
                            source_transitions: Vec::new(),
                            source_transition_origin: "pre_fold_snapshot".to_string(),
                            transitions: Vec::new(),
                        },
                    ],
                }],
            }],
        };

        let json = render_json(&map);
        assert!(json.contains("\"schema\": \"arch.thread_lowering_proof.v5\""));
        assert!(json.contains("\"source_next_index\": 3"));
        assert!(json.contains("\"source_next_name\": \"_t0_S3_wait_cycles\""));
        assert!(json.contains("\"once\": false"));
        assert!(json.contains("\"folded_exit_updates\": [\"done_r <= true\"]"));
        assert!(json.contains(
            "\"folded_exit_assignments\": [{\"target\": \"done_r\", \"value\": \"true\"}]"
        ));
        assert!(json.contains("\"source_transitions\": [{\"condition\": \"req\""));
        assert!(json.contains("\"condition_guard\": {\"kind\":\"atom\",\"name\":\"req\"}"));
        assert!(json.contains("\"source_transition_origin\": \"pre_fold_snapshot\""));
        assert!(json.contains("\"condition\": \"req\""));
        assert!(json.contains("\"target_index\": 3"));
        assert!(json.contains("\"target_name\": \"_t0_S3_wait_cycles\""));
        assert!(json.contains("\"state_name\": \"_t0_S2_action\""));
        assert!(json.contains("\"emitted\": false"));
    }

    #[test]
    fn render_lean_checked_rejects_missing_wait_cycles_count() {
        let map = ThreadMap {
            modules: vec![ThreadMapModule {
                module_name: "M".to_string(),
                generated_module_name: "_M_threads".to_string(),
                span: Span::new(0, 1),
                threads: vec![ThreadMapThread {
                    name: "Worker".to_string(),
                    index: 0,
                    once: false,
                    span: Span::new(0, 1),
                    hazards: Vec::new(),
                    states: vec![ThreadMapState {
                        index: 0,
                        state_name: "_t0_S0_wait_cycles".to_string(),
                        role: "wait_cycles".to_string(),
                        emitted: true,
                        span: Span::new(0, 1),
                        labels: vec!["wait 2 cycle".to_string()],
                        source_next_index: 0,
                        source_next_name: "_t0_S0_wait_cycles".to_string(),
                        wait_cycles_count: None,
                        seq_updates: Vec::new(),
                        seq_assignments: Vec::new(),
                        folded_exit_updates: Vec::new(),
                        folded_exit_assignments: Vec::new(),
                        source_transitions: vec![ThreadMapTransition {
                            condition: "true".to_string(),
                            condition_guard: Some(ThreadMapGuardExpr::True),
                            target_index: 0,
                            target_name: "_t0_S0_wait_cycles".to_string(),
                        }],
                        source_transition_origin: "pre_fold_snapshot".to_string(),
                        transitions: vec![ThreadMapTransition {
                            condition: "true".to_string(),
                            condition_guard: Some(ThreadMapGuardExpr::True),
                            target_index: 0,
                            target_name: "_t0_S0_wait_cycles".to_string(),
                        }],
                    }],
                }],
            }],
        };

        let err = render_lean_checked(&map).expect_err("missing wait count should fail");
        assert!(
            err.contains("missing structured wait_cycles_count"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn render_lean_checked_rejects_missing_non_counted_condition_guard() {
        let map = ThreadMap {
            modules: vec![ThreadMapModule {
                module_name: "M".to_string(),
                generated_module_name: "_M_threads".to_string(),
                span: Span::new(0, 1),
                threads: vec![ThreadMapThread {
                    name: "Worker".to_string(),
                    index: 0,
                    once: false,
                    span: Span::new(0, 1),
                    hazards: Vec::new(),
                    states: vec![ThreadMapState {
                        index: 0,
                        state_name: "_t0_S0_wait_until".to_string(),
                        role: "wait_until".to_string(),
                        emitted: true,
                        span: Span::new(0, 1),
                        labels: vec!["wait until req".to_string()],
                        source_next_index: 0,
                        source_next_name: "_t0_S0_wait_until".to_string(),
                        wait_cycles_count: None,
                        seq_updates: Vec::new(),
                        seq_assignments: Vec::new(),
                        folded_exit_updates: Vec::new(),
                        folded_exit_assignments: Vec::new(),
                        source_transitions: vec![ThreadMapTransition {
                            condition: "req".to_string(),
                            condition_guard: None,
                            target_index: 0,
                            target_name: "_t0_S0_wait_until".to_string(),
                        }],
                        source_transition_origin: "pre_fold_snapshot".to_string(),
                        transitions: vec![ThreadMapTransition {
                            condition: "req".to_string(),
                            condition_guard: None,
                            target_index: 0,
                            target_name: "_t0_S0_wait_until".to_string(),
                        }],
                    }],
                }],
            }],
        };

        let err = render_lean_checked(&map).expect_err("missing guard should fail");
        assert!(
            err.contains("missing structured condition_guard"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn render_lean_checked_allows_wait_cycles_transition_without_guard() {
        let map = ThreadMap {
            modules: vec![ThreadMapModule {
                module_name: "M".to_string(),
                generated_module_name: "_M_threads".to_string(),
                span: Span::new(0, 1),
                threads: vec![ThreadMapThread {
                    name: "Worker".to_string(),
                    index: 0,
                    once: false,
                    span: Span::new(0, 1),
                    hazards: Vec::new(),
                    states: vec![ThreadMapState {
                        index: 0,
                        state_name: "_t0_S0_wait_cycles".to_string(),
                        role: "wait_cycles".to_string(),
                        emitted: true,
                        span: Span::new(0, 1),
                        labels: vec!["wait 2 cycle".to_string()],
                        source_next_index: 0,
                        source_next_name: "_t0_S0_wait_cycles".to_string(),
                        wait_cycles_count: Some("2".to_string()),
                        seq_updates: Vec::new(),
                        seq_assignments: Vec::new(),
                        folded_exit_updates: Vec::new(),
                        folded_exit_assignments: Vec::new(),
                        source_transitions: vec![ThreadMapTransition {
                            condition: "true".to_string(),
                            condition_guard: None,
                            target_index: 0,
                            target_name: "_t0_S0_wait_cycles".to_string(),
                        }],
                        source_transition_origin: "pre_fold_snapshot".to_string(),
                        transitions: vec![ThreadMapTransition {
                            condition: "true".to_string(),
                            condition_guard: None,
                            target_index: 0,
                            target_name: "_t0_S0_wait_cycles".to_string(),
                        }],
                    }],
                }],
            }],
        };

        let lean = render_lean_checked(&map).expect("wait_cycles guard should be optional");
        assert!(
            lean.contains("Control.waitCycles 2"),
            "expected counted-wait control in Lean:\n{lean}"
        );
    }

    #[test]
    fn render_lean_checked_rejects_unknown_transition_target() {
        let mut state = emitted_action_state(0, 0);
        state.transitions[0].target_index = 99;
        state.transitions[0].target_name = "_t0_S99_missing".to_string();
        let map = map_with_states(vec![state]);

        let err = render_lean_checked(&map).expect_err("unknown target should fail");
        assert!(
            err.contains("targets non-emitted or unknown state 99"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn render_lean_checked_rejects_source_next_drift() {
        let map = map_with_states(vec![emitted_action_state(0, 0), emitted_action_state(1, 0)]);

        let err = render_lean_checked(&map).expect_err("source-next drift should fail");
        assert!(
            err.contains("source_next compact state 0, expected natural compact next 1"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn render_lean_checked_rejects_non_contiguous_raw_state_table() {
        let state = emitted_action_state(1, 1);
        let map = map_with_states(vec![state]);

        let err = render_lean_checked(&map).expect_err("non-contiguous raw table should fail");
        assert!(
            err.contains("raw state table is not contiguous at position 0"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn render_lean_checked_rejects_unknown_source_transition_origin() {
        let mut state = emitted_action_state(0, 0);
        state.source_transition_origin = "post_fold_guess".to_string();
        let map = map_with_states(vec![state]);

        let err = render_lean_checked(&map).expect_err("bad provenance should fail");
        assert!(
            err.contains("unsupported source_transition_origin"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn render_lean_checked_rejects_single_branch_dispatch() {
        let mut state = emitted_action_state(0, 0);
        state.role = "dispatch".to_string();
        let map = map_with_states(vec![state]);

        let err = render_lean_checked(&map).expect_err("single-branch dispatch should fail");
        assert!(
            err.contains("dispatch state must have at least two source and lowered transitions"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn render_lean_checked_rejects_partial_folded_assignment_coverage() {
        let mut state = emitted_action_state(0, 0);
        state.folded_exit_updates = vec!["done_r <= true".to_string()];
        let map = map_with_states(vec![state]);

        let err = render_lean_checked(&map).expect_err("partial folded assignments should fail");
        assert!(
            err.contains("folded_exit_assignments only cover 0/1 folded_exit_updates"),
            "unexpected error: {err}"
        );
    }
}
