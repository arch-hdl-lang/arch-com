#!/usr/bin/env python3
"""Convert an ARCH thread-proof JSON sidecar into Lean proof replay files.

This is a prototype bridge between `arch build --emit-thread-proof` and the
Lean thread-lowering proof models. It checks the abstract control table through
the `CountedWait` model and, for certificates that include folded wait-exit
updates, emits `FoldedExit` store-effect replay proofs as well.
"""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path
from typing import Any


GuardExprNode = tuple[str, Any]
NatExprNode = tuple[str, Any]


class SymbolTable:
    """Injective string-to-Nat assignment for one generated Lean file."""

    def __init__(self) -> None:
        self._ids: dict[str, int] = {}

    def id(self, value: str) -> int:
        if value not in self._ids:
            self._ids[value] = len(self._ids)
        return self._ids[value]


def lean_ident(name: str) -> str:
    cleaned = re.sub(r"[^A-Za-z0-9_]", "_", name)
    if not cleaned or cleaned[0].isdigit():
        cleaned = f"cert_{cleaned}"
    return cleaned


def strip_outer_parens(value: str) -> str:
    s = value.strip()
    while s.startswith("(") and s.endswith(")"):
        depth = 0
        wraps = True
        for idx, ch in enumerate(s):
            if ch == "(":
                depth += 1
            elif ch == ")":
                depth -= 1
                if depth == 0 and idx != len(s) - 1:
                    wraps = False
                    break
            if depth < 0:
                wraps = False
                break
        if not wraps or depth != 0:
            break
        s = s[1:-1].strip()
    return s


def split_top_level(value: str, op: str) -> list[str] | None:
    parts: list[str] = []
    depth = 0
    start = 0
    idx = 0
    while idx < len(value):
        ch = value[idx]
        if ch == "(":
            depth += 1
            idx += 1
            continue
        if ch == ")":
            depth -= 1
            idx += 1
            continue
        if depth == 0 and value.startswith(op, idx):
            parts.append(value[start:idx].strip())
            idx += len(op)
            start = idx
            continue
        idx += 1
    if not parts:
        return None
    parts.append(value[start:].strip())
    return parts


def split_top_level_comparison(value: str) -> tuple[str, str, str] | None:
    depth = 0
    idx = 0
    ops = (">=", "<=", "==", "!=", "<", ">")
    while idx < len(value):
        ch = value[idx]
        if ch == "(":
            depth += 1
            idx += 1
            continue
        if ch == ")":
            depth -= 1
            idx += 1
            continue
        if depth == 0:
            for op in ops:
                if value.startswith(op, idx):
                    return value[:idx].strip(), op, value[idx + len(op):].strip()
        idx += 1
    return None


def parse_nat_expr(value: str, nat_ids: SymbolTable) -> NatExprNode:
    s = strip_outer_parens(value)
    sized_dec = re.fullmatch(r"[0-9]+'d([0-9]+)(?:\.(?:resize|trunc)\([0-9]+\))?", s)
    if sized_dec:
        return ("const", int(sized_dec.group(1)))
    plain = re.fullmatch(r"([0-9]+)(?:\.(?:resize|trunc)\([0-9]+\))?", s)
    if plain:
        return ("const", int(plain.group(1)))
    return ("var", nat_ids.id(s))


def nat_expr_from_json(value: Any, nat_ids: SymbolTable) -> NatExprNode:
    if not isinstance(value, dict):
        raise SystemExit(f"condition_guard Nat expression must be an object, got {value!r}")
    kind = value.get("kind")
    if kind == "const":
        if "value" not in value:
            raise SystemExit(f"const Nat expression missing value: {value!r}")
        const_value = value["value"]
        if not isinstance(const_value, int) or isinstance(const_value, bool):
            raise SystemExit(f"const Nat expression value must be an integer: {value!r}")
        if const_value < 0:
            raise SystemExit(f"const Nat expression value must be non-negative: {value!r}")
        return ("const", const_value)
    if kind == "var":
        if "name" not in value:
            raise SystemExit(f"var Nat expression missing name: {value!r}")
        if not isinstance(value["name"], str):
            raise SystemExit(f"var Nat expression name must be a string: {value!r}")
        return ("var", nat_ids.id(value["name"]))
    raise SystemExit(f"unsupported condition_guard Nat expression kind: {kind!r}")


def render_nat_expr_in_namespace(expr: NatExprNode, namespace: str = "") -> str:
    nat_expr = f"{namespace}.NatExpr" if namespace else "NatExpr"
    tag = expr[0]
    if tag == "const":
        return f"({nat_expr}.const {expr[1]})"
    if tag == "var":
        return f"({nat_expr}.var {expr[1]})"
    raise ValueError(f"unknown Nat expression tag: {tag}")


def render_nat_expr(expr: NatExprNode) -> str:
    return render_nat_expr_in_namespace(expr)


def parse_guard_expr(
    condition: str,
    guard_ids: SymbolTable,
    nat_ids: SymbolTable,
) -> GuardExprNode:
    s = strip_outer_parens(condition)
    if s in {"always", "true", "1'b1", "1"}:
        return ("true",)
    if s in {"false", "1'b0", "0"}:
        return ("false",)
    or_parts = split_top_level(s, "||")
    if or_parts is not None:
        expr = parse_guard_expr(or_parts[0], guard_ids, nat_ids)
        for part in or_parts[1:]:
            expr = ("or", expr, parse_guard_expr(part, guard_ids, nat_ids))
        return expr
    and_parts = split_top_level(s, "&&")
    if and_parts is not None:
        expr = parse_guard_expr(and_parts[0], guard_ids, nat_ids)
        for part in and_parts[1:]:
            expr = ("and", expr, parse_guard_expr(part, guard_ids, nat_ids))
        return expr
    if s.startswith("!"):
        return ("not", parse_guard_expr(s[1:], guard_ids, nat_ids))
    comparison = split_top_level_comparison(s)
    if comparison is not None:
        lhs, op, rhs = comparison
        lhs_expr = parse_nat_expr(lhs, nat_ids)
        rhs_expr = parse_nat_expr(rhs, nat_ids)
        if op == "<":
            return ("lt", lhs_expr, rhs_expr)
        if op == ">=":
            return ("ge", lhs_expr, rhs_expr)
        if op == ">":
            return ("lt", rhs_expr, lhs_expr)
        if op == "<=":
            return ("ge", rhs_expr, lhs_expr)
        if op == "==":
            return ("eq", lhs_expr, rhs_expr)
        if op == "!=":
            return ("ne", lhs_expr, rhs_expr)
    return ("atom", guard_ids.id(s))


def guard_expr_from_json(
    value: Any,
    guard_ids: SymbolTable,
    nat_ids: SymbolTable,
) -> GuardExprNode:
    if not isinstance(value, dict):
        raise SystemExit(f"condition_guard must be an object, got {value!r}")
    kind = value.get("kind")
    if kind == "atom":
        if "name" not in value:
            raise SystemExit(f"atom condition_guard missing name: {value!r}")
        if not isinstance(value["name"], str):
            raise SystemExit(f"atom condition_guard name must be a string: {value!r}")
        return ("atom", guard_ids.id(value["name"]))
    if kind == "true":
        return ("true",)
    if kind == "false":
        return ("false",)
    if kind == "not":
        if "expr" not in value:
            raise SystemExit(f"not condition_guard missing expr: {value!r}")
        return ("not", guard_expr_from_json(value["expr"], guard_ids, nat_ids))
    if kind in {"and", "or"}:
        if "lhs" not in value or "rhs" not in value:
            raise SystemExit(f"{kind} condition_guard missing lhs/rhs: {value!r}")
        return (
            kind,
            guard_expr_from_json(value["lhs"], guard_ids, nat_ids),
            guard_expr_from_json(value["rhs"], guard_ids, nat_ids),
        )
    if kind in {"lt", "ge", "eq", "ne"}:
        if "lhs" not in value or "rhs" not in value:
            raise SystemExit(f"{kind} condition_guard missing lhs/rhs: {value!r}")
        return (
            kind,
            nat_expr_from_json(value["lhs"], nat_ids),
            nat_expr_from_json(value["rhs"], nat_ids),
        )
    raise SystemExit(f"unsupported condition_guard kind: {kind!r}")


def transition_guard_expr(
    transition: dict[str, Any],
    guard_atom_ids: SymbolTable,
    nat_ids: SymbolTable,
) -> GuardExprNode:
    structured = transition.get("condition_guard")
    if isinstance(structured, dict):
        return guard_expr_from_json(structured, guard_atom_ids, nat_ids)
    return parse_guard_expr(str(transition["condition"]), guard_atom_ids, nat_ids)


def transition_guard_key(
    transition: dict[str, Any],
    guard_atom_ids: SymbolTable,
    nat_ids: SymbolTable,
) -> str:
    return render_guard_expr(transition_guard_expr(transition, guard_atom_ids, nat_ids))


def transition_is_unconditional(
    transition: dict[str, Any],
    guard_atom_ids: SymbolTable,
    nat_ids: SymbolTable,
) -> bool:
    return transition_guard_expr(transition, guard_atom_ids, nat_ids)[0] == "true"


def render_guard_expr_in_namespace(expr: GuardExprNode, namespace: str = "") -> str:
    guard_expr = f"{namespace}.GuardExpr" if namespace else "GuardExpr"
    tag = expr[0]
    if tag == "true":
        return f"{guard_expr}.trueLit"
    if tag == "false":
        return f"{guard_expr}.falseLit"
    if tag == "atom":
        return f"({guard_expr}.atom {expr[1]})"
    if tag == "not":
        return f"({guard_expr}.neg {render_guard_expr_in_namespace(expr[1], namespace)})"
    if tag == "and":
        return (
            f"({guard_expr}.and {render_guard_expr_in_namespace(expr[1], namespace)} "
            f"{render_guard_expr_in_namespace(expr[2], namespace)})"
        )
    if tag == "or":
        return (
            f"({guard_expr}.or {render_guard_expr_in_namespace(expr[1], namespace)} "
            f"{render_guard_expr_in_namespace(expr[2], namespace)})"
        )
    if tag == "lt":
        return (
            f"({guard_expr}.lt {render_nat_expr_in_namespace(expr[1], namespace)} "
            f"{render_nat_expr_in_namespace(expr[2], namespace)})"
        )
    if tag == "ge":
        return (
            f"({guard_expr}.ge {render_nat_expr_in_namespace(expr[1], namespace)} "
            f"{render_nat_expr_in_namespace(expr[2], namespace)})"
        )
    if tag == "eq":
        return (
            f"({guard_expr}.eq {render_nat_expr_in_namespace(expr[1], namespace)} "
            f"{render_nat_expr_in_namespace(expr[2], namespace)})"
        )
    if tag == "ne":
        return (
            f"({guard_expr}.ne {render_nat_expr_in_namespace(expr[1], namespace)} "
            f"{render_nat_expr_in_namespace(expr[2], namespace)})"
        )
    raise ValueError(f"unknown guard expression tag: {tag}")


def render_guard_expr(expr: GuardExprNode) -> str:
    return render_guard_expr_in_namespace(expr)


def guard_expr_atoms(expr: GuardExprNode) -> set[int]:
    tag = expr[0]
    if tag == "atom":
        return {int(expr[1])}
    if tag in {"true", "false", "lt", "ge", "eq", "ne"}:
        return set()
    if tag == "not":
        return guard_expr_atoms(expr[1])
    if tag in {"and", "or"}:
        return guard_expr_atoms(expr[1]) | guard_expr_atoms(expr[2])
    return set()


def guard_requirements(expr: GuardExprNode) -> dict[int, bool] | None:
    tag = expr[0]
    if tag == "true":
        return {}
    if tag == "false":
        return {}
    if tag == "atom":
        return {int(expr[1]): True}
    if tag == "not":
        inner = expr[1]
        if inner[0] != "atom":
            return None
        return {int(inner[1]): False}
    if tag in {"lt", "ge", "eq", "ne"}:
        return {}
    if tag == "and":
        lhs = guard_requirements(expr[1])
        rhs = guard_requirements(expr[2])
        if lhs is None or rhs is None:
            return None
        merged = dict(lhs)
        for atom, value in rhs.items():
            if atom in merged and merged[atom] != value:
                return None
            merged[atom] = value
        return merged
    return None


def guard_comparisons(expr: GuardExprNode) -> set[tuple[str, NatExprNode, NatExprNode]] | None:
    tag = expr[0]
    if tag in {"true", "false", "atom"}:
        return set()
    if tag in {"lt", "ge", "eq", "ne"}:
        return {(tag, expr[1], expr[2])}
    if tag == "and":
        lhs = guard_comparisons(expr[1])
        rhs = guard_comparisons(expr[2])
        if lhs is None or rhs is None:
            return None
        return lhs | rhs
    return None


def guards_syntactically_exclusive(lhs: GuardExprNode, rhs: GuardExprNode) -> bool:
    lhs_req = guard_requirements(lhs)
    rhs_req = guard_requirements(rhs)
    if lhs_req is None or rhs_req is None:
        bool_exclusive = False
    else:
        bool_exclusive = any(
            atom in rhs_req and rhs_req[atom] != value for atom, value in lhs_req.items()
        )
    lhs_cmp = guard_comparisons(lhs)
    rhs_cmp = guard_comparisons(rhs)
    cmp_exclusive = False
    if lhs_cmp is not None and rhs_cmp is not None:
        cmp_exclusive = any(
            (("ge" if op == "lt" else "lt"), lhs_expr, rhs_expr) in rhs_cmp
            for op, lhs_expr, rhs_expr in lhs_cmp
            if op in {"lt", "ge"}
        ) or any(
            (("ne" if op == "eq" else "eq"), lhs_expr, rhs_expr) in rhs_cmp
            for op, lhs_expr, rhs_expr in lhs_cmp
            if op in {"eq", "ne"}
        )
    return bool_exclusive or cmp_exclusive


def guard_has_comparison(expr: GuardExprNode) -> bool:
    tag = expr[0]
    if tag in {"lt", "ge", "eq", "ne"}:
        return True
    if tag == "not":
        return guard_has_comparison(expr[1])
    if tag in {"and", "or"}:
        return guard_has_comparison(expr[1]) or guard_has_comparison(expr[2])
    return False


def control_for_state(
    state: dict[str, Any],
    index_map: dict[int, int],
    guard_atom_ids: SymbolTable,
    nat_ids: SymbolTable,
    source_next: int | None,
    num_states: int,
    once: bool,
    raw_states: list[dict[str, Any]],
    transitions_field: str = "transitions",
) -> str:
    role = state.get("role", "")
    transitions = state.get(transitions_field, [])
    if role == "wait_until":
        guard = (
            transition_guard_key(transitions[0], guard_atom_ids, nat_ids)
            if transitions
            else "GuardExpr.trueLit"
        )
        return f"Control.waitUntil {guard}"
    if role == "wait_cycles":
        raw_count = state.get("wait_cycles_count")
        if raw_count is None:
            raise SystemExit(f"{state.get('state_name', '<unnamed>')}: missing wait_cycles_count")
        if not re.fullmatch(r"[0-9]+", str(raw_count)):
            raise SystemExit(
                f"{state.get('state_name', '<unnamed>')}: non-literal wait_cycles_count {raw_count!r}"
            )
        count = int(raw_count)
        return f"Control.waitCycles {count}"
    if role == "dispatch":
        branches = []
        for tr in transitions:
            guard = transition_guard_key(tr, guard_atom_ids, nat_ids)
            target = resolve_target(state, tr, index_map, num_states, once, raw_states)
            branches.append(f"{{ guard := {guard}, target := {target} }}")
        return "Control.dispatch [" + ", ".join(branches) + "]"
    if len(transitions) == 1 and source_next is not None:
        target = resolve_target(
            state, transitions[0], index_map, num_states, once, raw_states
        )
        if transition_is_unconditional(transitions[0], guard_atom_ids, nat_ids):
            if target != source_next:
                return f"Control.jump {target}"
        else:
            guard = transition_guard_key(transitions[0], guard_atom_ids, nat_ids)
            return f"Control.guarded {guard} {target}"
    return "Control.advance"


def is_guarded_non_dispatch_transition(
    state: dict[str, Any],
    transition: dict[str, Any],
    index_map: dict[int, int],
    source_next: int,
    guard_atom_ids: SymbolTable,
    nat_ids: SymbolTable,
) -> bool:
    if state.get("role") == "dispatch":
        return False
    return not transition_is_unconditional(transition, guard_atom_ids, nat_ids)


def is_jump_non_dispatch_transition(
    state: dict[str, Any],
    transition: dict[str, Any],
    index_map: dict[int, int],
    source_next: int,
    guard_atom_ids: SymbolTable,
    nat_ids: SymbolTable,
    num_states: int,
    once: bool,
    raw_states: list[dict[str, Any]],
) -> bool:
    if state.get("role") == "dispatch":
        return False
    target = resolve_target(state, transition, index_map, num_states, once, raw_states)
    return target != source_next and transition_is_unconditional(
        transition, guard_atom_ids, nat_ids
    )


def resolve_target(
    state: dict[str, Any],
    transition: dict[str, Any],
    index_map: dict[int, int],
    num_states: int,
    once: bool,
    raw_states: list[dict[str, Any]],
) -> int:
    raw_target = int(transition["target_index"])
    if raw_target not in index_map:
        if is_once_terminal_raw_target(raw_target, raw_states, once):
            compact_idx = index_map.get(int(state["index"]))
            if compact_idx is not None:
                return compact_next(compact_idx, num_states, once)
        state_name = state.get("state_name", "<unnamed>")
        target_name = transition.get("target_name", f"S{raw_target}")
        raise SystemExit(
            f"{state_name}: transition targets non-emitted or unknown state "
            f"{raw_target} ({target_name})"
        )
    return index_map[raw_target]


def target_for_state(
    state: dict[str, Any],
    num_states: int,
    index_map: dict[int, int],
    once: bool,
    raw_states: list[dict[str, Any]],
) -> int:
    transitions = state.get("transitions", [])
    if transitions:
        return resolve_target(state, transitions[0], index_map, num_states, once, raw_states)
    compact_idx = index_map.get(int(state["index"]), 0)
    return compact_next(compact_idx, num_states, once)


def source_next_for_state(
    state: dict[str, Any],
    num_states: int,
    index_map: dict[int, int],
    once: bool,
    require_explicit: bool,
    raw_states: list[dict[str, Any]],
) -> int:
    compact_idx = index_map.get(int(state["index"]), 0)
    inferred = compact_next(compact_idx, num_states, once)
    if "source_next_index" not in state:
        if require_explicit:
            raise SystemExit(
                f"{state.get('state_name', '<unnamed>')}: missing source_next_index"
            )
        return inferred
    if require_explicit and "source_next_name" not in state:
        raise SystemExit(
            f"{state.get('state_name', '<unnamed>')}: missing source_next_name"
        )

    raw_target = int(state["source_next_index"])
    if raw_target not in index_map:
        if is_once_terminal_raw_target(raw_target, raw_states, once):
            return inferred
        target_name = state.get("source_next_name", f"S{raw_target}")
        raise SystemExit(
            f"{state.get('state_name', '<unnamed>')}: source_next targets "
            f"non-emitted or unknown state {raw_target} ({target_name})"
        )
    explicit = index_map[raw_target]
    if explicit != inferred:
        raise SystemExit(
            f"{state.get('state_name', '<unnamed>')}: source_next compact state "
            f"{explicit}, expected natural compact next {inferred}"
        )
    return explicit


def compact_next(idx: int, num_states: int, once: bool) -> int:
    if idx + 1 < num_states:
        return idx + 1
    return idx if once else 0


def is_once_terminal_raw_target(
    raw_target: int,
    raw_states: list[dict[str, Any]],
    once: bool,
) -> bool:
    if not once or raw_target < 0 or raw_target >= len(raw_states):
        return False
    state = raw_states[raw_target]
    if state.get("emitted", True):
        return False
    if int(state.get("source_next_index", -1)) != raw_target:
        return False
    return all(
        int(transition.get("target_index", -1)) == raw_target
        for transition in state.get("source_transitions", [])
    )


def lean_nat_list(values: list[int]) -> str:
    if not values:
        return "[]"
    return "[" + ", ".join(str(value) for value in values) + "]"


def action_list_for_state(state: dict[str, Any], action_ids: SymbolTable) -> str:
    assignments = state.get("seq_assignments", [])
    if assignments:
        return lean_nat_list(
            [
                action_ids.id(f"{assignment.get('target', '')} := {assignment.get('value', '')}")
                for assignment in assignments
            ]
        )
    return lean_nat_list([action_ids.id(str(label)) for label in state.get("seq_updates", [])])


def validate_assignment_coverage(base: str, state: dict[str, Any]) -> None:
    state_name = state.get("state_name", f"S{state.get('index', '?')}")
    seq_updates = state.get("seq_updates", [])
    seq_assignments = state.get("seq_assignments", [])
    if seq_assignments and len(seq_assignments) != len(seq_updates):
        raise SystemExit(
            f"{base}: {state_name}: seq_assignments partially cover seq_updates "
            f"({len(seq_assignments)} of {len(seq_updates)})"
        )

    folded_updates = state.get("folded_exit_updates", [])
    folded_assignments = state.get("folded_exit_assignments", [])
    if folded_updates and len(folded_assignments) != len(folded_updates):
        raise SystemExit(
            f"{base}: {state_name}: folded_exit_updates require structured "
            f"folded_exit_assignments for every update "
            f"({len(folded_assignments)} of {len(folded_updates)})"
        )


def validate_thread_table(
    base: str,
    thread: dict[str, Any],
    require_structured_guards: bool,
) -> list[dict[str, Any]]:
    raw_states = thread.get("states", [])
    if not isinstance(raw_states, list) or not raw_states:
        raise SystemExit(f"{base}: certificate has no states")

    for expected_idx, state in enumerate(raw_states):
        try:
            raw_idx = int(state["index"])
        except (KeyError, TypeError, ValueError) as err:
            raise SystemExit(f"{base}: state at table slot {expected_idx} has invalid index") from err
        if raw_idx != expected_idx:
            raise SystemExit(
                f"{base}: state table must be contiguous; slot {expected_idx} has index {raw_idx}"
            )
        validate_assignment_coverage(base, state)

    emitted = [state for state in raw_states if state.get("emitted", True)]
    if not emitted:
        raise SystemExit(f"{base}: certificate has no emitted states")
    if int(emitted[0]["index"]) != 0:
        raise SystemExit(
            f"{base}: first emitted state must be raw state 0, got {emitted[0]['index']}"
        )
    for state in emitted:
        validate_emitted_state_shape(base, state, require_structured_guards)
    return emitted


def validate_emitted_state_shape(
    base: str,
    state: dict[str, Any],
    require_structured_guards: bool,
) -> None:
    role = state.get("role")
    state_name = state.get("state_name", f"S{state.get('index', '?')}")
    transitions = state.get("transitions", [])
    if not isinstance(transitions, list):
        raise SystemExit(f"{base}: {state_name}: transitions must be a list")

    expected_roles = {"entry", "action", "wait_until", "wait_cycles", "dispatch"}
    if role not in expected_roles:
        raise SystemExit(f"{base}: {state_name}: unknown state role {role!r}")

    if role == "dispatch":
        if len(transitions) < 2:
            raise SystemExit(
                f"{base}: {state_name}: dispatch state requires at least two transitions"
            )
    elif len(transitions) != 1:
        raise SystemExit(
            f"{base}: {state_name}: {role} state requires exactly one transition"
        )

    if role != "wait_cycles" and state.get("wait_cycles_count") is not None:
        raise SystemExit(
            f"{base}: {state_name}: wait_cycles_count only allowed on wait_cycles states"
        )

    for ti, transition in enumerate(transitions):
        for field in ("condition", "target_index", "target_name"):
            if field not in transition:
                raise SystemExit(
                    f"{base}: {state_name}: transition {ti} missing {field}"
                )
        validate_transition_guard_shape(
            base,
            state_name,
            role,
            "transition",
            ti,
            transition,
            require_structured_guards,
        )


def validate_transition_guard_shape(
    base: str,
    state_name: str,
    role: str,
    transition_kind: str,
    transition_index: int,
    transition: dict[str, Any],
    require_structured_guards: bool,
) -> None:
    if not require_structured_guards or role == "wait_cycles":
        return
    if "condition_guard" not in transition:
        raise SystemExit(
            f"{base}: {state_name}: {transition_kind} {transition_index} "
            "missing condition_guard"
        )
    if not isinstance(transition["condition_guard"], dict):
        raise SystemExit(
            f"{base}: {state_name}: {transition_kind} {transition_index} "
            "condition_guard must be an object"
        )
    try:
        guard_expr_from_json(
            transition["condition_guard"],
            SymbolTable(),
            SymbolTable(),
        )
    except SystemExit as err:
        raise SystemExit(
            f"{base}: {state_name}: {transition_kind} {transition_index} "
            f"invalid condition_guard: {err}"
        ) from err


def validate_source_transitions(
    base: str,
    state: dict[str, Any],
    require_source_transitions: bool,
    require_structured_guards: bool,
) -> None:
    role = state.get("role")
    state_name = state.get("state_name", f"S{state.get('index', '?')}")
    if "source_transitions" not in state:
        if require_source_transitions:
            raise SystemExit(f"{base}: {state_name}: missing source_transitions")
        return
    origin = state.get("source_transition_origin")
    if require_source_transitions and origin != "pre_fold_snapshot":
        raise SystemExit(
            f"{base}: {state_name}: unsupported source_transition_origin {origin!r}"
        )

    source_transitions = state.get("source_transitions")
    if not isinstance(source_transitions, list):
        raise SystemExit(f"{base}: {state_name}: source_transitions must be a list")

    transitions = state.get("transitions", [])
    if role == "dispatch" and len(source_transitions) != len(transitions):
        raise SystemExit(
            f"{base}: {state_name}: source_transitions length "
            f"{len(source_transitions)} must match transitions length {len(transitions)}"
        )
    if role != "dispatch" and len(source_transitions) != 1:
        raise SystemExit(
            f"{base}: {state_name}: non-dispatch source_transitions requires exactly one transition"
        )

    for ti, transition in enumerate(source_transitions):
        for field in ("condition", "target_index", "target_name"):
            if field not in transition:
                raise SystemExit(
                    f"{base}: {state_name}: source transition {ti} missing {field}"
                )
        validate_transition_guard_shape(
            base,
            state_name,
            role,
            "source transition",
            ti,
            transition,
            require_structured_guards,
        )


def render_thread(
    module_name: str,
    thread: dict[str, Any],
    require_source_next: bool,
    require_source_transitions: bool,
    require_structured_guards: bool,
    guard_atom_ids: SymbolTable,
    nat_ids: SymbolTable,
    action_ids: SymbolTable,
) -> str:
    base = lean_ident(f"{module_name}_{thread['name']}_{thread['index']}")
    raw_states = thread.get("states", [])
    states = validate_thread_table(base, thread, require_structured_guards)
    once = bool(thread.get("once", False))
    index_map = {int(s["index"]): new_idx for new_idx, s in enumerate(states)}
    num_states = len(states)
    by_index = {idx: state for idx, state in enumerate(states)}
    has_dispatch = any(state.get("role") == "dispatch" for state in states)

    arms = []
    for idx in range(num_states):
        state = by_index.get(idx)
        if state is None:
            actions = "[]"
            source_control = "Control.advance"
            fsm_control = "Control.advance"
            target = compact_next(idx, num_states, once)
        else:
            validate_source_transitions(
                base, state, require_source_transitions, require_structured_guards
            )
            source_next = source_next_for_state(
                state, num_states, index_map, once, require_source_next, raw_states
            )
            source_control = control_for_state(
                state,
                index_map,
                guard_atom_ids,
                nat_ids,
                source_next,
                num_states,
                once,
                raw_states,
                "source_transitions" if "source_transitions" in state else "transitions",
            )
            fsm_control = control_for_state(
                state,
                index_map,
                guard_atom_ids,
                nat_ids,
                source_next,
                num_states,
                once,
                raw_states,
                "transitions",
            )
            if state.get("role") != "dispatch" and "source_transitions" in state:
                source_transition = state["source_transitions"][0]
                source_target = resolve_target(
                    state,
                    source_transition,
                    index_map,
                    num_states,
                    once,
                    raw_states,
                )
                if source_target != source_next and not is_guarded_non_dispatch_transition(
                    state,
                    source_transition,
                    index_map,
                    source_next,
                    guard_atom_ids,
                    nat_ids,
                ) and not is_jump_non_dispatch_transition(
                    state,
                    source_transition,
                    index_map,
                    source_next,
                    guard_atom_ids,
                    nat_ids,
                    num_states,
                    once,
                    raw_states,
                ):
                    raise SystemExit(
                        f"{base}: state {state['state_name']} source transition targets "
                        f"compact state {source_target}, expected source_next {source_next}"
                    )
            target = target_for_state(state, num_states, index_map, once, raw_states)
            lowered_transition = state.get("transitions", [{}])[0]
            has_guarded_lowered = (
                state.get("role") != "dispatch"
                and state.get("transitions")
                and is_guarded_non_dispatch_transition(
                    state,
                    lowered_transition,
                    index_map,
                    source_next,
                    guard_atom_ids,
                    nat_ids,
                )
            )
            has_jump_lowered = (
                state.get("role") != "dispatch"
                and state.get("transitions")
                and is_jump_non_dispatch_transition(
                    state,
                    lowered_transition,
                    index_map,
                    source_next,
                    guard_atom_ids,
                    nat_ids,
                    num_states,
                    once,
                    raw_states,
                )
            )
            if (
                state.get("role") != "dispatch"
                and target != source_next
                and not has_guarded_lowered
                and not has_jump_lowered
            ):
                raise SystemExit(
                    f"{base}: state {state['state_name']} targets compact state {target}, "
                    f"expected source_next {source_next}"
                )
            if state.get("role") == "dispatch" or has_guarded_lowered or has_jump_lowered:
                target = source_next
            actions = action_list_for_state(state, action_ids)
        arms.append((idx, actions, source_control, fsm_control, target))

    source_lines = [
        f"def {base}Source : SourceThread :=",
        f"  {{ numStates := {num_states}",
        f"    once := {'true' if once else 'false'}",
        "    state := fun pc =>",
    ]
    for idx, actions, source_control, fsm_control, target in arms:
        prefix = "      if" if idx == 0 else "      else if"
        source_lines.append(f"{prefix} pc = {idx} then")
        source_lines.append(f"        {{ actions := {actions}, control := {source_control} }}")
    source_lines.append("      else")
    source_lines.append("        { actions := [], control := Control.advance } }")
    fsm_lines = [
        f"def {base}Fsm : LoweredFsm :=",
        "  { state := fun pc =>",
    ]
    for idx, actions, source_control, fsm_control, target in arms:
        prefix = "      if" if idx == 0 else "      else if"
        fsm_lines.append(f"{prefix} pc = {idx} then")
        fsm_lines.append(
            f"        {{ actions := {actions}, control := {fsm_control}, target := {target} }}"
        )
    fsm_lines.append("      else")
    fsm_lines.append(
        f"        {{ actions := [], control := Control.advance, target := sourceNext {base}Source pc }} }}"
    )

    cert_tactic = cert_field_tactic(base, num_states)
    dispatch_cert_tactic = cert_field_tactic(
        base, num_states, include_dispatch_branches=has_dispatch
    )

    proof = [
        f"example : LoweringCertifies {base}Source {base}Fsm := by",
        "  refine",
        "    { actions_ok := ?_",
        "      control_ok := ?_",
        "      dispatch_branches_ok := ?_",
        "      target_ok := ?_ }",
        "  · intro pc",
        indent_block(cert_tactic, 4),
        "  · intro pc",
        indent_block(cert_tactic, 4),
        "  · intro pc",
        indent_block(dispatch_cert_tactic, 4),
        "  · intro pc",
        indent_block(cert_tactic, 4),
        "",
        f"example : StepEffectFaithful {base}Source {base}Fsm :=",
        "  step_effect_faithful (by",
        "    refine",
        "      { actions_ok := ?_",
        "        control_ok := ?_",
        "        dispatch_branches_ok := ?_",
        "        target_ok := ?_ }",
        "    · intro pc",
        indent_block(cert_tactic, 6),
        "    · intro pc",
        indent_block(cert_tactic, 6),
        "    · intro pc",
        indent_block(dispatch_cert_tactic, 6),
        "    · intro pc",
        indent_block(cert_tactic, 6),
        "  )",
        "",
        f"example (inputs : Nat -> Env) (natInputs : Nat -> NatEnv) (cfg0 : Config) :",
        f"    forall t, sourceTraceObs {base}Source inputs natInputs cfg0 t = fsmTraceObs {base}Fsm inputs natInputs cfg0 t :=",
        f"  trace_equiv (by",
        "    refine",
        "      { actions_ok := ?_",
        "        control_ok := ?_",
        "        dispatch_branches_ok := ?_",
        "        target_ok := ?_ }",
        "    · intro pc",
        indent_block(cert_tactic, 6),
        "    · intro pc",
        indent_block(cert_tactic, 6),
        "    · intro pc",
        indent_block(dispatch_cert_tactic, 6),
        "    · intro pc",
        indent_block(cert_tactic, 6),
        "  ) inputs natInputs cfg0",
    ]
    exclusivity = render_dispatch_exclusivity_proofs(base, states, guard_atom_ids, nat_ids)
    return "\n".join(source_lines + [""] + fsm_lines + [""] + proof + exclusivity)


def render_dispatch_exclusivity_proofs(
    base: str,
    states: list[dict[str, Any]],
    guard_ids: SymbolTable,
    nat_ids: SymbolTable,
) -> list[str]:
    chunks: list[str] = []
    for state in states:
        if state.get("role") != "dispatch":
            continue
        transitions = state.get("source_transitions", state.get("transitions", []))
        parsed = [
            transition_guard_expr(transition, guard_ids, nat_ids)
            for transition in transitions
        ]
        state_name = lean_ident(str(state.get("state_name", f"S{state.get('index', '?')}")))
        for i, lhs in enumerate(parsed):
            for j in range(i + 1, len(parsed)):
                rhs = parsed[j]
                if not guards_syntactically_exclusive(lhs, rhs):
                    continue
                atoms = sorted(guard_expr_atoms(lhs) | guard_expr_atoms(rhs))
                split = " <;> ".join(f"by_cases h{atom} : env {atom}" for atom in atoms)
                has_comparison = guard_has_comparison(lhs) or guard_has_comparison(rhs)
                if split:
                    simp_args = "GuardExpr.eval, NatExpr.eval, " + ", ".join(
                        f"h{atom}" for atom in atoms
                    )
                    tactic = f"{split} <;> simp [{simp_args}]"
                else:
                    tactic = "simp [GuardExpr.eval, NatExpr.eval]"
                if has_comparison:
                    tactic += " <;> omega"
                    example_binder = "(env : Env) (natEnv : NatEnv)"
                else:
                    example_binder = "(env : Env)"
                lhs_eval = f"GuardExpr.eval env {render_guard_expr(lhs)}"
                rhs_eval = f"GuardExpr.eval env {render_guard_expr(rhs)}"
                if has_comparison:
                    lhs_eval += " natEnv"
                    rhs_eval += " natEnv"
                chunks.extend(
                    [
                        "",
                        f"-- Parseable dispatch guards {i} and {j} in {state_name} are mutually exclusive.",
                        f"example {example_binder} :",
                        f"    {lhs_eval} = true ->",
                        f"    {rhs_eval} = true ->",
                        f"    False := by",
                        f"  {tactic}",
                    ]
                )
    return chunks


def indent_block(text: str, spaces: int) -> str:
    prefix = " " * spaces
    return "\n".join(prefix + line if line else line for line in text.splitlines())


def cert_field_tactic(
    base: str,
    num_states: int,
    include_dispatch_branches: bool = False,
) -> str:
    extra = ", dispatchBranches" if include_dispatch_branches else ""
    if num_states == 0:
        return f"simp [{base}Source, {base}Fsm, sourceNext{extra}]"

    def simp_line(hyps: list[str]) -> str:
        hyp_args = "".join(f", {hyp}" for hyp in hyps)
        return f"simp [{base}Source, {base}Fsm, sourceNext{extra}{hyp_args}]"

    def go(idx: int, hyps: list[str], depth: int) -> list[str]:
        if idx >= num_states:
            return ["  " * depth + simp_line(hyps)]
        hyp = f"h{idx}"
        lines = ["  " * depth + f"by_cases {hyp} : pc = {idx}"]
        lines.append("  " * depth + "· " + simp_line(hyps + [hyp]))
        lines.append("  " * depth + "·")
        lines.extend(go(idx + 1, hyps + [hyp], depth + 1))
        return lines

    return "\n".join(go(0, [], 0))


def lean_update_list(
    labels: list[str],
    assignments: list[dict[str, Any]],
    update_ids: SymbolTable,
    var_ids: SymbolTable,
    value_ids: SymbolTable,
) -> str:
    if assignments:
        updates = []
        fe = "Arch.ThreadLoweringProof.FoldedExit"
        for assignment in assignments:
            target = var_ids.id(str(assignment.get("target", "")))
            value = value_ids.id(str(assignment.get("value", "")))
            updates.append(f"{fe}.setVar {target} {value}")
        return "[" + ", ".join(updates) + "]"
    if not labels:
        return "[]"
    return "[" + ", ".join(f"updateById {update_ids.id(str(label))}" for label in labels) + "]"


def render_folded_exit_proof(
    module_name: str,
    thread: dict[str, Any],
    source_state: dict[str, Any],
    folded_ordinal: int,
    guard_atom_ids: SymbolTable,
    nat_ids: SymbolTable,
    update_ids: SymbolTable,
    var_ids: SymbolTable,
    value_ids: SymbolTable,
) -> str:
    base = lean_ident(
        f"{module_name}_{thread['name']}_{thread['index']}_{source_state['state_name']}_folded_{folded_ordinal}"
    )
    transitions = source_state.get("transitions", [])
    fe = "Arch.ThreadLoweringProof.FoldedExit"
    guard = (
        render_guard_expr_in_namespace(
            transition_guard_expr(transitions[0], guard_atom_ids, nat_ids), fe
        )
        if transitions
        else f"{fe}.GuardExpr.trueLit"
    )
    updates = lean_update_list(
        source_state.get("folded_exit_updates", []),
        source_state.get("folded_exit_assignments", []),
        update_ids,
        var_ids,
        value_ids,
    )
    assignments = source_state.get("folded_exit_assignments", [])
    lines = [
            f"def {base}Source : {fe}.SourceThread :=",
            "  { numStates := 2",
            "    state := fun pc =>",
            "      if pc = 0 then",
            f"        {{ updates := []",
            f"          exitUpdates := {updates}",
            f"          control := {fe}.Control.waitUntil {guard} }}",
            "      else",
            "        { updates := []",
            "          exitUpdates := []",
            f"          control := {fe}.Control.advance }} }}",
            "",
            f"def {base}Fsm : {fe}.LoweredFsm :=",
            "  { state := fun pc =>",
            "      if pc = 0 then",
            "        { updates := []",
            f"          foldedExitUpdates := {updates}",
            f"          control := {fe}.Control.waitUntil {guard}",
            "          target := 1",
            "          foldedTarget := some 1 }",
            "      else",
            "        { updates := []",
            "          foldedExitUpdates := []",
            f"          control := {fe}.Control.advance",
            f"          target := {fe}.sourceNext {base}Source pc",
            "          foldedTarget := none } }",
            "",
            f"example : {fe}.LoweringCertifies {base}Source {base}Fsm := by",
            "  refine",
            "    { updates_ok := ?_",
            "      control_ok := ?_",
            "      target_ok := ?_",
            "      folded_updates_ok := ?_",
            "      folded_target_ok := ?_",
            "      folded_target_some_ok := ?_ }",
            "  · intro pc",
            f"    by_cases h : pc = 0 <;> simp [{base}Source, {base}Fsm, h]",
            "  · intro pc",
            f"    by_cases h : pc = 0 <;> simp [{base}Source, {base}Fsm, h]",
            "  · intro pc",
            f"    by_cases h : pc = 0 <;> simp [{base}Source, {base}Fsm, {fe}.sourceNext, h]",
            "  · intro pc",
            f"    by_cases h : pc = 0 <;> simp [{base}Source, {base}Fsm, h]",
            "  · intro pc hupdates",
            f"    by_cases h : pc = 0 <;> simp [{base}Source, {base}Fsm, {fe}.sourceNext, h] at *",
            "  · intro pc target hfold",
            f"    by_cases h : pc = 0 <;> simp [{base}Source, {base}Fsm, {fe}.sourceNext, h] at *",
            "    exact hfold.symm",
            "",
            f"example (env : {fe}.Env) (natEnv : {fe}.NatEnv) (cfg : {fe}.Config) :",
            f"    {fe}.sourceStep {base}Source env natEnv cfg = {fe}.fsmStep {base}Fsm env natEnv cfg :=",
            f"  {fe}.one_step_equiv (by",
            "    refine",
            "      { updates_ok := ?_",
            "        control_ok := ?_",
            "        target_ok := ?_",
            "        folded_updates_ok := ?_",
            "        folded_target_ok := ?_",
            "        folded_target_some_ok := ?_ }",
            f"    · intro pc; by_cases h : pc = 0 <;> simp [{base}Source, {base}Fsm, h]",
            f"    · intro pc; by_cases h : pc = 0 <;> simp [{base}Source, {base}Fsm, h]",
            f"    · intro pc; by_cases h : pc = 0 <;> simp [{base}Source, {base}Fsm, {fe}.sourceNext, h]",
            f"    · intro pc; by_cases h : pc = 0 <;> simp [{base}Source, {base}Fsm, h]",
            f"    · intro pc hupdates; by_cases h : pc = 0 <;> simp [{base}Source, {base}Fsm, {fe}.sourceNext, h] at *",
            f"    · intro pc target hfold; by_cases h : pc = 0 <;> simp [{base}Source, {base}Fsm, {fe}.sourceNext, h] at *; exact hfold.symm",
            "  ) env natEnv cfg",
    ]
    seen_targets = set()
    final_writes = []
    for assignment in reversed(assignments):
        target_name = str(assignment.get("target", ""))
        if target_name in seen_targets:
            continue
        seen_targets.add(target_name)
        final_writes.append(assignment)
    final_writes.reverse()

    for assignment in final_writes:
        target = var_ids.id(str(assignment.get("target", "")))
        value = value_ids.id(str(assignment.get("value", "")))
        lines.extend(
            [
                "",
                f"example (env : {fe}.Env) (natEnv : {fe}.NatEnv) (store : {fe}.Store)",
                f"    (hguard : {fe}.GuardExpr.eval env {guard} natEnv = true) :",
                f"    let cfg : {fe}.Config := {{ pc := 0, store := store }}",
                f"    (({fe}.sourceStep {base}Source env natEnv cfg).store {target} = {value}) /\\",
                f"      (({fe}.fsmStep {base}Fsm env natEnv cfg).store {target} = {value}) := by",
                f"  simp [{fe}.sourceStep, {fe}.fsmStep, {base}Source, {base}Fsm,",
                f"    {fe}.sourceAdvanceTo, {fe}.fsmAdvanceTo, {fe}.applyUpdates, {fe}.setVar, hguard]",
            ]
        )
    return "\n".join(lines)


def render_seq_assignment_store_proof(
    module_name: str,
    thread: dict[str, Any],
    state: dict[str, Any],
    ordinal: int,
    var_ids: SymbolTable,
    value_ids: SymbolTable,
) -> str:
    base = lean_ident(
        f"{module_name}_{thread['name']}_{thread['index']}_{state['state_name']}_seq_{ordinal}"
    )
    fe = "Arch.ThreadLoweringProof.FoldedExit"
    assignments = state.get("seq_assignments", [])
    updates = []
    for assignment in assignments:
        target = var_ids.id(str(assignment.get("target", "")))
        value = value_ids.id(str(assignment.get("value", "")))
        updates.append(f"{fe}.setVar {target} {value}")

    lines = [
        f"def {base}Updates : List {fe}.Update :=",
        "  [" + ", ".join(updates) + "]",
    ]

    seen_targets = set()
    final_writes = []
    for assignment in reversed(assignments):
        target_name = str(assignment.get("target", ""))
        if target_name in seen_targets:
            continue
        seen_targets.add(target_name)
        final_writes.append(assignment)
    final_writes.reverse()

    for assignment in final_writes:
        target = var_ids.id(str(assignment.get("target", "")))
        value = value_ids.id(str(assignment.get("value", "")))
        lines.extend(
            [
                "",
                f"example (store : {fe}.Store) :",
                f"    {fe}.applyUpdates {base}Updates store {target} = {value} := by",
                f"  simp [{base}Updates, {fe}.applyUpdates, {fe}.setVar]",
            ]
        )
    return "\n".join(lines)


def render_seq_assignment_store_proofs(
    cert: dict[str, Any],
    var_ids: SymbolTable,
    value_ids: SymbolTable,
) -> list[str]:
    chunks: list[str] = []
    ordinal = 0
    for module in cert.get("modules", []):
        for thread in module.get("threads", []):
            for state in thread.get("states", []):
                if not state.get("emitted", True) or not state.get("seq_assignments"):
                    continue
                chunks.append(
                    render_seq_assignment_store_proof(
                        module["module_name"],
                        thread,
                        state,
                        ordinal,
                        var_ids,
                        value_ids,
                    )
                )
                ordinal += 1
    return chunks


def render_folded_exit_proofs(
    cert: dict[str, Any],
    guard_atom_ids: SymbolTable,
    nat_ids: SymbolTable,
    update_ids: SymbolTable,
    var_ids: SymbolTable,
    value_ids: SymbolTable,
) -> list[str]:
    chunks: list[str] = []
    ordinal = 0
    for module in cert.get("modules", []):
        for thread in module.get("threads", []):
            for state in thread.get("states", []):
                if state.get("folded_exit_updates"):
                    chunks.append(
                        render_folded_exit_proof(
                            module["module_name"],
                            thread,
                            state,
                            ordinal,
                            guard_atom_ids,
                            nat_ids,
                            update_ids,
                            var_ids,
                            value_ids,
                        )
                    )
                    ordinal += 1
    return chunks


def render(cert: dict[str, Any]) -> str:
    schema = cert.get("schema")
    require_source_next = schema == "arch.thread_lowering_proof.v3"
    require_source_transitions = schema in {
        "arch.thread_lowering_proof.v4",
        "arch.thread_lowering_proof.v5",
    }
    require_structured_guards = schema == "arch.thread_lowering_proof.v5"
    require_source_next = require_source_next or require_source_transitions
    guard_atom_ids = SymbolTable()
    nat_ids = SymbolTable()
    action_ids = SymbolTable()
    update_ids = SymbolTable()
    var_ids = SymbolTable()
    value_ids = SymbolTable()
    chunks = [
        "import ArchThreadLoweringProof.CountedWait",
        "import ArchThreadLoweringProof.FoldedExit",
        "",
        "set_option linter.unusedSimpArgs false",
        "",
        "namespace Arch.ThreadLoweringProof.Generated",
        "open Arch.ThreadLoweringProof.CountedWait",
        "",
        "def updateById (id : Nat) : Arch.ThreadLoweringProof.FoldedExit.Update :=",
        "  fun store var => if var = id then id else store var",
        "",
    ]
    for module in cert.get("modules", []):
        for thread in module.get("threads", []):
            chunks.append(
                render_thread(
                    module["module_name"],
                    thread,
                    require_source_next,
                    require_source_transitions,
                    require_structured_guards,
                    guard_atom_ids,
                    nat_ids,
                    action_ids,
                )
            )
            chunks.append("")
    chunks.extend(render_seq_assignment_store_proofs(cert, var_ids, value_ids))
    if chunks and chunks[-1] != "":
        chunks.append("")
    chunks.extend(
        render_folded_exit_proofs(
            cert,
            guard_atom_ids,
            nat_ids,
            update_ids,
            var_ids,
            value_ids,
        )
    )
    chunks.append("end Arch.ThreadLoweringProof.Generated")
    chunks.append("")
    return "\n".join(chunks)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("certificate", type=Path)
    parser.add_argument("-o", "--output", type=Path, required=True)
    args = parser.parse_args()

    cert = json.loads(args.certificate.read_text())
    if cert.get("schema") not in {
        "arch.thread_lowering_proof.v0",
        "arch.thread_lowering_proof.v1",
        "arch.thread_lowering_proof.v2",
        "arch.thread_lowering_proof.v3",
        "arch.thread_lowering_proof.v4",
        "arch.thread_lowering_proof.v5",
    }:
        raise SystemExit(f"unsupported schema: {cert.get('schema')!r}")
    args.output.write_text(render(cert))


if __name__ == "__main__":
    main()
