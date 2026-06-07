#!/usr/bin/env python3
"""Focused regression tests for the certificate-to-Lean bridge."""

from __future__ import annotations

import importlib.util
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


SCRIPT = Path(__file__).with_name("cert_to_lean.py")
PROJECT_DIR = SCRIPT.parent.parent
SPEC = importlib.util.spec_from_file_location("cert_to_lean", SCRIPT)
assert SPEC is not None
cert_to_lean = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(cert_to_lean)


def guard_true() -> dict:
    return {"kind": "true"}


def guard_atom(name: str) -> dict:
    return {"kind": "atom", "name": name}


def guard_not(inner: dict) -> dict:
    return {"kind": "not", "expr": inner}


def nat_var(name: str) -> dict:
    return {"kind": "var", "name": name}


def nat_const(value: int) -> dict:
    return {"kind": "const", "value": value}


def transition(
    condition: str,
    target_index: int,
    target_name: str,
    condition_guard: dict | None = None,
) -> dict:
    if condition_guard is None:
        if condition in {"always", "true", "1'b1", "1"}:
            condition_guard = guard_true()
        elif condition.startswith("!") and condition[1:].replace("_", "").isalnum():
            condition_guard = guard_not(guard_atom(condition[1:]))
        else:
            condition_guard = guard_atom(condition)
    return {
        "condition": condition,
        "condition_guard": condition_guard,
        "target_index": target_index,
        "target_name": target_name,
    }


def strip_condition_guards(cert: dict) -> None:
    for module in cert.get("modules", []):
        for thread in module.get("threads", []):
            for state in thread.get("states", []):
                for field in ("source_transitions", "transitions"):
                    for tr in state.get(field, []):
                        tr.pop("condition_guard", None)


def base_cert(target_index: int = 1) -> dict:
    return {
        "schema": "arch.thread_lowering_proof.v5",
        "modules": [
            {
                "module_name": "M",
                "generated_module_name": "_M_threads",
                "threads": [
                    {
                        "name": "T",
                        "index": 0,
                        "once": False,
                        "states": [
                            {
                                "index": 0,
                                "state_name": "_t0_S0_entry",
                                "role": "entry",
                                "emitted": True,
                                "source_next_index": 1,
                                "source_next_name": "_t0_S1_action",
                                "labels": ["seq: 1 stmt"],
                                "wait_cycles_count": None,
                                "seq_updates": ["x <= true"],
                                "seq_assignments": [
                                    {"target": "x", "value": "true"},
                                ],
                                "folded_exit_updates": [],
                                "folded_exit_assignments": [],
                                "source_transitions": [
                                    transition("always", target_index, "_t0_S1_action"),
                                ],
                                "source_transition_origin": "pre_fold_snapshot",
                                "transitions": [
                                    transition("always", target_index, "_t0_S1_action"),
                                ],
                            },
                            {
                                "index": 1,
                                "state_name": "_t0_S1_action",
                                "role": "action",
                                "emitted": True,
                                "source_next_index": 0,
                                "source_next_name": "_t0_S0_entry",
                                "labels": [],
                                "wait_cycles_count": None,
                                "seq_updates": [],
                                "seq_assignments": [],
                                "folded_exit_updates": [],
                                "folded_exit_assignments": [],
                                "source_transitions": [
                                    transition("always", 0, "_t0_S0_entry"),
                                ],
                                "source_transition_origin": "pre_fold_snapshot",
                                "transitions": [
                                    transition("always", 0, "_t0_S0_entry"),
                                ],
                            },
                        ],
                    },
                ],
            },
        ],
    }


def cert_targeting_non_emitted_state() -> dict:
    cert = base_cert(target_index=2)
    cert["modules"][0]["threads"][0]["states"].append(
        {
            "index": 2,
            "state_name": "_t0_S2_folded_action",
            "role": "action",
            "emitted": False,
            "source_next_index": 0,
            "source_next_name": "_t0_S0_entry",
            "labels": [],
            "wait_cycles_count": None,
            "seq_updates": [],
            "seq_assignments": [],
            "folded_exit_updates": [],
            "folded_exit_assignments": [],
            "source_transitions": [],
            "source_transition_origin": "pre_fold_snapshot",
            "transitions": [],
        }
    )
    return cert


def dispatch_cert() -> dict:
    cert = base_cert(target_index=1)
    states = cert["modules"][0]["threads"][0]["states"]
    states[1]["source_next_index"] = 2
    states[1]["source_next_name"] = "_t0_S2_action"
    states[1]["source_transitions"] = [
        transition("always", 2, "_t0_S2_action"),
    ]
    states[1]["source_transition_origin"] = "pre_fold_snapshot"
    states[1]["transitions"] = [
        transition("always", 2, "_t0_S2_action"),
    ]
    states.append(
        {
            "index": 2,
            "state_name": "_t0_S2_action",
            "role": "action",
            "emitted": True,
            "source_next_index": 0,
            "source_next_name": "_t0_S0_entry",
            "labels": [],
            "wait_cycles_count": None,
            "seq_updates": [],
            "seq_assignments": [],
            "folded_exit_updates": [],
            "folded_exit_assignments": [],
            "source_transitions": [
                transition("always", 0, "_t0_S0_entry"),
            ],
            "source_transition_origin": "pre_fold_snapshot",
            "transitions": [
                transition("always", 0, "_t0_S0_entry"),
            ],
        }
    )
    state0 = states[0]
    state0["role"] = "dispatch"
    state0["source_transitions"] = [
        transition("sel", 2, "_t0_S2_action"),
        transition("!sel", 1, "_t0_S1_action"),
    ]
    state0["source_transition_origin"] = "pre_fold_snapshot"
    state0["transitions"] = [
        transition("sel", 1, "_t0_S1_action"),
        transition("!sel", 2, "_t0_S2_action"),
    ]
    return cert


def single_guard_cert() -> dict:
    cert = base_cert(target_index=1)
    states = cert["modules"][0]["threads"][0]["states"]
    states[1]["source_next_index"] = 2
    states[1]["source_next_name"] = "_t0_S2_action"
    states[1]["source_transitions"] = [
        transition("always", 2, "_t0_S2_action"),
    ]
    states[1]["transitions"] = [
        transition("always", 2, "_t0_S2_action"),
    ]
    states.append(
        {
            "index": 2,
            "state_name": "_t0_S2_action",
            "role": "action",
            "emitted": True,
            "source_next_index": 0,
            "source_next_name": "_t0_S0_entry",
            "labels": [],
            "wait_cycles_count": None,
            "seq_updates": [],
            "seq_assignments": [],
            "folded_exit_updates": [],
            "folded_exit_assignments": [],
            "source_transitions": [
                transition("always", 0, "_t0_S0_entry"),
            ],
            "source_transition_origin": "pre_fold_snapshot",
            "transitions": [
                transition("always", 0, "_t0_S0_entry"),
            ],
        }
    )
    guarded = states[1]
    guarded["source_transitions"] = [
        transition("ack", 0, "_t0_S0_entry"),
    ]
    guarded["transitions"] = [
        transition("ack", 0, "_t0_S0_entry"),
    ]
    return cert


def unconditional_jump_cert() -> dict:
    cert = single_guard_cert()
    guarded = cert["modules"][0]["threads"][0]["states"][1]
    guarded["source_transitions"] = [
        transition("true", 0, "_t0_S0_entry"),
    ]
    guarded["transitions"] = [
        transition("true", 0, "_t0_S0_entry"),
    ]
    return cert


def comparison_dispatch_cert() -> dict:
    cert = dispatch_cert()
    state0 = cert["modules"][0]["threads"][0]["states"][0]
    state0["source_transitions"] = [
        {
            "condition": "branch_lt",
            "condition_guard": {
                "kind": "and",
                "lhs": {"kind": "atom", "name": "grant"},
                "rhs": {
                    "kind": "lt",
                    "lhs": {"kind": "var", "name": "cnt"},
                    "rhs": {"kind": "const", "value": 3},
                },
            },
            "target_index": 2,
            "target_name": "_t0_S2_action",
        },
        {
            "condition": "branch_ge",
            "condition_guard": {
                "kind": "and",
                "lhs": {"kind": "atom", "name": "grant"},
                "rhs": {
                    "kind": "ge",
                    "lhs": {"kind": "var", "name": "cnt"},
                    "rhs": {"kind": "const", "value": 3},
                },
            },
            "target_index": 1,
            "target_name": "_t0_S1_action",
        },
    ]
    state0["transitions"] = [dict(tr) for tr in state0["source_transitions"]]
    return cert


def equality_dispatch_cert() -> dict:
    cert = dispatch_cert()
    state0 = cert["modules"][0]["threads"][0]["states"][0]
    state0["source_transitions"] = [
        transition(
            "idx == 3",
            2,
            "_t0_S2_action",
            {"kind": "eq", "lhs": nat_var("idx"), "rhs": nat_const(3)},
        ),
        transition(
            "idx != 3",
            1,
            "_t0_S1_action",
            {"kind": "ne", "lhs": nat_var("idx"), "rhs": nat_const(3)},
        ),
    ]
    state0["transitions"] = [dict(tr) for tr in state0["source_transitions"]]
    return cert


class CertToLeanTests(unittest.TestCase):
    def run_lean(self, lean_source: str) -> subprocess.CompletedProcess[str]:
        if shutil.which("lake") is None:
            self.skipTest("lake not found on PATH")
        with tempfile.NamedTemporaryFile("w", suffix=".lean", delete=False) as tmp:
            tmp.write(lean_source)
            tmp_path = Path(tmp.name)
        try:
            return subprocess.run(
                ["lake", "env", "lean", str(tmp_path)],
                cwd=PROJECT_DIR,
                env=os.environ.copy(),
                text=True,
                capture_output=True,
                check=False,
            )
        finally:
            tmp_path.unlink(missing_ok=True)

    def test_fsm_table_uses_certificate_target_not_source_next_alias(self) -> None:
        lean = cert_to_lean.render(base_cert())
        self.assertIn("def M_T_0Fsm : LoweredFsm :=", lean)
        self.assertIn("dispatch_branches_ok", lean)
        self.assertIn("{ actions := [", lean)
        self.assertIn("target := 1", lean)
        self.assertIn("target := 0", lean)
        self.assertIn("target := sourceNext M_T_0Source pc", lean)

    def test_non_dispatch_source_fsm_target_mismatch_fails_lean(self) -> None:
        cert = base_cert()
        state = cert["modules"][0]["threads"][0]["states"][0]
        state["source_transitions"][0]["target_index"] = 0
        state["source_transitions"][0]["target_name"] = "_t0_S0_entry"
        result = self.run_lean(cert_to_lean.render(cert))
        self.assertNotEqual(
            result.returncode,
            0,
            "mismatched non-dispatch source/FSM jump should not replay in Lean",
        )

    def test_unknown_target_is_rejected(self) -> None:
        with self.assertRaises(SystemExit) as caught:
            cert_to_lean.render(base_cert(target_index=99))
        self.assertIn("targets non-emitted or unknown state 99", str(caught.exception))

    def test_non_emitted_target_is_rejected(self) -> None:
        with self.assertRaises(SystemExit) as caught:
            cert_to_lean.render(cert_targeting_non_emitted_state())
        self.assertIn("targets non-emitted or unknown state 2", str(caught.exception))

    def test_non_contiguous_state_table_is_rejected(self) -> None:
        cert = base_cert()
        cert["modules"][0]["threads"][0]["states"][1]["index"] = 7
        with self.assertRaises(SystemExit) as caught:
            cert_to_lean.render(cert)
        self.assertIn("state table must be contiguous", str(caught.exception))

    def test_empty_emitted_state_table_is_rejected(self) -> None:
        cert = base_cert()
        for state in cert["modules"][0]["threads"][0]["states"]:
            state["emitted"] = False
        with self.assertRaises(SystemExit) as caught:
            cert_to_lean.render(cert)
        self.assertIn("certificate has no emitted states", str(caught.exception))

    def test_first_emitted_state_must_be_raw_zero(self) -> None:
        cert = base_cert()
        cert["modules"][0]["threads"][0]["states"][0]["emitted"] = False
        with self.assertRaises(SystemExit) as caught:
            cert_to_lean.render(cert)
        self.assertIn("first emitted state must be raw state 0", str(caught.exception))

    def test_v3_requires_source_next_index(self) -> None:
        cert = base_cert()
        del cert["modules"][0]["threads"][0]["states"][0]["source_next_index"]
        with self.assertRaises(SystemExit) as caught:
            cert_to_lean.render(cert)
        self.assertIn("missing source_next_index", str(caught.exception))

    def test_v3_requires_source_next_name(self) -> None:
        cert = base_cert()
        del cert["modules"][0]["threads"][0]["states"][0]["source_next_name"]
        with self.assertRaises(SystemExit) as caught:
            cert_to_lean.render(cert)
        self.assertIn("missing source_next_name", str(caught.exception))

    def test_source_next_unknown_target_is_rejected(self) -> None:
        cert = base_cert()
        state = cert["modules"][0]["threads"][0]["states"][0]
        state["source_next_index"] = 99
        state["source_next_name"] = "_t0_S99_missing"
        with self.assertRaises(SystemExit) as caught:
            cert_to_lean.render(cert)
        self.assertIn("source_next targets non-emitted or unknown state 99", str(caught.exception))

    def test_source_next_must_match_compact_natural_next(self) -> None:
        cert = base_cert()
        state = cert["modules"][0]["threads"][0]["states"][0]
        state["source_next_index"] = 0
        state["source_next_name"] = "_t0_S0_entry"
        with self.assertRaises(SystemExit) as caught:
            cert_to_lean.render(cert)
        self.assertIn("expected natural compact next 1", str(caught.exception))

    def test_v5_requires_source_transitions(self) -> None:
        cert = base_cert()
        del cert["modules"][0]["threads"][0]["states"][0]["source_transitions"]
        with self.assertRaises(SystemExit) as caught:
            cert_to_lean.render(cert)
        self.assertIn("missing source_transitions", str(caught.exception))

    def test_v5_requires_supported_source_transition_origin(self) -> None:
        cert = base_cert()
        del cert["modules"][0]["threads"][0]["states"][0]["source_transition_origin"]
        with self.assertRaises(SystemExit) as caught:
            cert_to_lean.render(cert)
        self.assertIn("unsupported source_transition_origin", str(caught.exception))

    def test_v5_rejects_unknown_source_transition_origin(self) -> None:
        cert = base_cert()
        cert["modules"][0]["threads"][0]["states"][0]["source_transition_origin"] = "post_fold_clone"
        with self.assertRaises(SystemExit) as caught:
            cert_to_lean.render(cert)
        self.assertIn("unsupported source_transition_origin", str(caught.exception))

    def test_source_transition_requires_required_fields(self) -> None:
        cert = base_cert()
        del cert["modules"][0]["threads"][0]["states"][0]["source_transitions"][0]["target_name"]
        with self.assertRaises(SystemExit) as caught:
            cert_to_lean.render(cert)
        self.assertIn("source transition 0 missing target_name", str(caught.exception))

    def test_v5_requires_lowered_transition_condition_guard(self) -> None:
        cert = base_cert()
        del cert["modules"][0]["threads"][0]["states"][0]["transitions"][0][
            "condition_guard"
        ]
        with self.assertRaises(SystemExit) as caught:
            cert_to_lean.render(cert)
        self.assertIn("transition 0 missing condition_guard", str(caught.exception))

    def test_v5_requires_source_transition_condition_guard(self) -> None:
        cert = base_cert()
        del cert["modules"][0]["threads"][0]["states"][0]["source_transitions"][0][
            "condition_guard"
        ]
        with self.assertRaises(SystemExit) as caught:
            cert_to_lean.render(cert)
        self.assertIn("source transition 0 missing condition_guard", str(caught.exception))

    def test_v5_rejects_malformed_boolean_condition_guard(self) -> None:
        cert = base_cert()
        cert["modules"][0]["threads"][0]["states"][0]["transitions"][0][
            "condition_guard"
        ] = {
            "kind": "and",
            "lhs": guard_atom("req"),
        }
        with self.assertRaises(SystemExit) as caught:
            cert_to_lean.render(cert)
        message = str(caught.exception)
        self.assertIn("transition 0 invalid condition_guard", message)
        self.assertIn("and condition_guard missing lhs/rhs", message)

    def test_v5_rejects_malformed_nat_condition_guard(self) -> None:
        cert = base_cert()
        cert["modules"][0]["threads"][0]["states"][0]["source_transitions"][0][
            "condition_guard"
        ] = {
            "kind": "lt",
            "lhs": {"kind": "var"},
            "rhs": {"kind": "const", "value": 3},
        }
        with self.assertRaises(SystemExit) as caught:
            cert_to_lean.render(cert)
        message = str(caught.exception)
        self.assertIn("source transition 0 invalid condition_guard", message)
        self.assertIn("var Nat expression missing name", message)

    def test_v5_rejects_non_string_condition_guard_atom_name(self) -> None:
        cert = base_cert()
        cert["modules"][0]["threads"][0]["states"][0]["transitions"][0][
            "condition_guard"
        ] = {
            "kind": "atom",
            "name": ["req"],
        }
        with self.assertRaises(SystemExit) as caught:
            cert_to_lean.render(cert)
        message = str(caught.exception)
        self.assertIn("transition 0 invalid condition_guard", message)
        self.assertIn("atom condition_guard name must be a string", message)

    def test_v5_rejects_non_integer_condition_guard_nat_const(self) -> None:
        cert = base_cert()
        cert["modules"][0]["threads"][0]["states"][0]["source_transitions"][0][
            "condition_guard"
        ] = {
            "kind": "lt",
            "lhs": {"kind": "var", "name": "idx"},
            "rhs": {"kind": "const", "value": "3"},
        }
        with self.assertRaises(SystemExit) as caught:
            cert_to_lean.render(cert)
        message = str(caught.exception)
        self.assertIn("source transition 0 invalid condition_guard", message)
        self.assertIn("const Nat expression value must be an integer", message)

    def test_v5_rejects_negative_condition_guard_nat_const(self) -> None:
        cert = base_cert()
        cert["modules"][0]["threads"][0]["states"][0]["source_transitions"][0][
            "condition_guard"
        ] = {
            "kind": "lt",
            "lhs": {"kind": "var", "name": "idx"},
            "rhs": {"kind": "const", "value": -1},
        }
        with self.assertRaises(SystemExit) as caught:
            cert_to_lean.render(cert)
        message = str(caught.exception)
        self.assertIn("source transition 0 invalid condition_guard", message)
        self.assertIn("const Nat expression value must be non-negative", message)

    def test_v5_accepts_structured_equality_condition_guard(self) -> None:
        cert = base_cert()
        guard = {"kind": "eq", "lhs": nat_var("idx"), "rhs": nat_const(3)}
        state = cert["modules"][0]["threads"][0]["states"][0]
        state["source_transitions"][0]["condition_guard"] = guard
        state["transitions"][0]["condition_guard"] = guard

        lean = cert_to_lean.render(cert)
        self.assertIn("GuardExpr.eq", lean)
        self.assertIn("NatExpr.const 3", lean)

    def test_v4_allows_label_only_guards_for_compatibility(self) -> None:
        cert = base_cert()
        cert["schema"] = "arch.thread_lowering_proof.v4"
        strip_condition_guards(cert)
        lean = cert_to_lean.render(cert)
        self.assertIn("LoweringCertifies", lean)

    def test_unconditional_non_dispatch_jump_replays_in_lean(self) -> None:
        lean = cert_to_lean.render(unconditional_jump_cert())
        self.assertIn("Control.jump 0", lean)
        result = self.run_lean(lean)
        self.assertEqual(
            result.returncode,
            0,
            f"expected unconditional jump replay to pass\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}",
        )

    def test_single_guarded_natural_transition_is_not_unconditional_advance(self) -> None:
        cert = single_guard_cert()
        guarded = cert["modules"][0]["threads"][0]["states"][1]
        guarded["source_transitions"][0]["target_index"] = 2
        guarded["source_transitions"][0]["target_name"] = "_t0_S2_action"
        guarded["transitions"][0]["target_index"] = 2
        guarded["transitions"][0]["target_name"] = "_t0_S2_action"
        lean = cert_to_lean.render(cert)
        self.assertIn("Control.guarded (GuardExpr.atom 0) 2", lean)
        result = self.run_lean(lean)
        self.assertEqual(
            result.returncode,
            0,
            f"expected natural guarded transition replay to pass\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}",
        )

    def test_mismatched_unconditional_jump_fails_lean(self) -> None:
        cert = base_cert()
        state = cert["modules"][0]["threads"][0]["states"][0]
        state["source_transitions"][0]["target_index"] = 0
        state["source_transitions"][0]["target_name"] = "_t0_S0_entry"
        state["transitions"][0]["target_index"] = 1
        state["transitions"][0]["target_name"] = "_t0_S1_action"
        result = self.run_lean(cert_to_lean.render(cert))
        self.assertNotEqual(
            result.returncode,
            0,
            "mismatched unconditional source/FSM targets should not replay in Lean",
        )

    def test_source_transition_unknown_target_is_rejected(self) -> None:
        cert = base_cert()
        state = cert["modules"][0]["threads"][0]["states"][0]
        state["source_transitions"][0]["target_index"] = 99
        state["source_transitions"][0]["target_name"] = "_t0_S99_missing"
        with self.assertRaises(SystemExit) as caught:
            cert_to_lean.render(cert)
        self.assertIn("targets non-emitted or unknown state 99", str(caught.exception))

    def test_source_dispatch_control_uses_source_transitions(self) -> None:
        lean = cert_to_lean.render(dispatch_cert())
        self.assertIn(
            "control := Control.dispatch [{ guard := (GuardExpr.atom 0), target := 2 }, { guard := (GuardExpr.neg (GuardExpr.atom 0)), target := 1 }]",
            lean,
        )
        self.assertIn(
            "control := Control.dispatch [{ guard := (GuardExpr.atom 0), target := 1 }, { guard := (GuardExpr.neg (GuardExpr.atom 0)), target := 2 }]",
            lean,
        )

    def test_parseable_dispatch_guards_emit_exclusivity_proof(self) -> None:
        lean = cert_to_lean.render(dispatch_cert())
        self.assertIn("GuardExpr.eval", lean)
        self.assertIn("mutually exclusive", lean)
        self.assertIn("by_cases", lean)

    def test_comparison_dispatch_guards_emit_exclusivity_proof(self) -> None:
        lean = cert_to_lean.render(comparison_dispatch_cert())
        self.assertIn("GuardExpr.lt", lean)
        self.assertIn("GuardExpr.ge", lean)
        self.assertIn("omega", lean)
        result = self.run_lean(lean)
        self.assertEqual(
            result.returncode,
            0,
            f"expected comparison exclusivity replay to pass\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}",
        )

    def test_equality_dispatch_guards_emit_exclusivity_proof(self) -> None:
        lean = cert_to_lean.render(equality_dispatch_cert())
        self.assertIn("GuardExpr.eq", lean)
        self.assertIn("GuardExpr.ne", lean)
        self.assertIn("mutually exclusive", lean)
        self.assertIn("omega", lean)
        result = self.run_lean(lean)
        self.assertEqual(
            result.returncode,
            0,
            f"expected equality exclusivity replay to pass\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}",
        )

    def test_structured_guards_not_display_labels_drive_lean_control_terms(self) -> None:
        cert = dispatch_cert()
        state0 = cert["modules"][0]["threads"][0]["states"][0]
        state0["source_transitions"][1]["condition"] = "sel_alias"
        state0["transitions"] = [dict(tr) for tr in state0["source_transitions"]]
        lean = cert_to_lean.render(cert)
        self.assertIn("{ guard := (GuardExpr.atom 0), target := 2 }", lean)
        self.assertIn("{ guard := (GuardExpr.neg (GuardExpr.atom 0)), target := 1 }", lean)
        self.assertNotIn("{ guard := (GuardExpr.atom 0), target := 1 }", lean)

    def test_same_guard_label_with_different_structured_guard_fails_lean(self) -> None:
        cert = single_guard_cert()
        guarded = cert["modules"][0]["threads"][0]["states"][1]
        guarded["source_transitions"][0]["condition_guard"] = {
            "kind": "atom",
            "name": "ack",
        }
        guarded["transitions"][0]["condition_guard"] = {
            "kind": "atom",
            "name": "ack_shadow",
        }
        lean = cert_to_lean.render(cert)
        self.assertIn("Control.guarded (GuardExpr.atom 0) 0", lean)
        self.assertIn("Control.guarded (GuardExpr.atom 1) 0", lean)
        result = self.run_lean(lean)
        self.assertNotEqual(
            result.returncode,
            0,
            "same label with different structured guards should not replay in Lean",
        )

    def test_single_guarded_non_dispatch_transition_replays_in_lean(self) -> None:
        lean = cert_to_lean.render(single_guard_cert())
        self.assertIn("Control.guarded (GuardExpr.atom 0) 0", lean)
        result = self.run_lean(lean)
        self.assertEqual(
            result.returncode,
            0,
            f"expected single guarded jump replay to pass\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}",
        )

    def test_mismatched_single_guarded_transition_fails_lean(self) -> None:
        cert = single_guard_cert()
        guarded = cert["modules"][0]["threads"][0]["states"][1]
        guarded["transitions"][0]["target_index"] = 0
        guarded["transitions"][0]["target_name"] = "_t0_S0_entry"
        guarded["source_transitions"][0]["target_index"] = 2
        guarded["source_transitions"][0]["target_name"] = "_t0_S2_action"
        result = self.run_lean(cert_to_lean.render(cert))
        self.assertNotEqual(
            result.returncode,
            0,
            "mismatched single guarded source/FSM targets should not replay in Lean",
        )

    def test_matching_certificate_replays_in_lean(self) -> None:
        result = self.run_lean(cert_to_lean.render(base_cert()))
        self.assertEqual(
            result.returncode,
            0,
            f"expected Lean replay to pass\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}",
        )

    def test_mismatched_dispatch_certificate_fails_lean(self) -> None:
        result = self.run_lean(cert_to_lean.render(dispatch_cert()))
        self.assertNotEqual(
            result.returncode,
            0,
            "mismatched source/FSM dispatch table should not replay in Lean",
        )
        self.assertIn(
            "error",
            (result.stdout + result.stderr).lower(),
            f"expected Lean failure to report an error\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}",
        )

    def test_unknown_role_is_rejected(self) -> None:
        cert = base_cert()
        cert["modules"][0]["threads"][0]["states"][0]["role"] = "mystery"
        with self.assertRaises(SystemExit) as caught:
            cert_to_lean.render(cert)
        self.assertIn("unknown state role", str(caught.exception))

    def test_non_dispatch_state_requires_one_transition(self) -> None:
        cert = base_cert()
        cert["modules"][0]["threads"][0]["states"][0]["transitions"] = []
        with self.assertRaises(SystemExit) as caught:
            cert_to_lean.render(cert)
        self.assertIn("entry state requires exactly one transition", str(caught.exception))

    def test_dispatch_requires_at_least_two_transitions(self) -> None:
        cert = base_cert()
        cert["modules"][0]["threads"][0]["states"][0]["role"] = "dispatch"
        with self.assertRaises(SystemExit) as caught:
            cert_to_lean.render(cert)
        self.assertIn("dispatch state requires at least two transitions", str(caught.exception))

    def test_transition_requires_required_fields(self) -> None:
        cert = base_cert()
        del cert["modules"][0]["threads"][0]["states"][0]["transitions"][0]["target_name"]
        with self.assertRaises(SystemExit) as caught:
            cert_to_lean.render(cert)
        self.assertIn("transition 0 missing target_name", str(caught.exception))

    def test_wait_count_only_allowed_on_wait_cycles(self) -> None:
        cert = base_cert()
        cert["modules"][0]["threads"][0]["states"][0]["wait_cycles_count"] = "2"
        with self.assertRaises(SystemExit) as caught:
            cert_to_lean.render(cert)
        self.assertIn("wait_cycles_count only allowed", str(caught.exception))

    def test_wait_cycles_requires_structured_count(self) -> None:
        cert = base_cert()
        state = cert["modules"][0]["threads"][0]["states"][0]
        state["role"] = "wait_cycles"
        state["labels"] = ["wait 2 cycle"]
        state["wait_cycles_count"] = None
        with self.assertRaises(SystemExit) as caught:
            cert_to_lean.render(cert)
        self.assertIn("missing wait_cycles_count", str(caught.exception))

    def test_wait_cycles_uses_structured_count(self) -> None:
        cert = base_cert()
        state = cert["modules"][0]["threads"][0]["states"][0]
        state["role"] = "wait_cycles"
        state["labels"] = ["wait 999 cycle"]
        state["wait_cycles_count"] = "2"
        lean = cert_to_lean.render(cert)
        self.assertIn("Control.waitCycles 2", lean)
        self.assertNotIn("Control.waitCycles 999", lean)

    def test_partial_seq_assignment_coverage_is_rejected(self) -> None:
        cert = base_cert()
        state = cert["modules"][0]["threads"][0]["states"][0]
        state["seq_updates"].append("if y then z <= true")
        with self.assertRaises(SystemExit) as caught:
            cert_to_lean.render(cert)
        self.assertIn("seq_assignments partially cover seq_updates", str(caught.exception))

    def test_seq_assignment_store_effects_are_emitted(self) -> None:
        cert = base_cert()
        state = cert["modules"][0]["threads"][0]["states"][0]
        state["seq_updates"] = ["x <= true", "y <= false"]
        state["seq_assignments"] = [
            {"target": "x", "value": "true"},
            {"target": "y", "value": "false"},
        ]
        lean = cert_to_lean.render(cert)
        self.assertIn(
            "def M_T_0__t0_S0_entry_seq_0Updates : List "
            "Arch.ThreadLoweringProof.FoldedExit.Update :=",
            lean,
        )
        self.assertIn(
            "[Arch.ThreadLoweringProof.FoldedExit.setVar 0 0, "
            "Arch.ThreadLoweringProof.FoldedExit.setVar 1 1]",
            lean,
        )
        self.assertIn(
            "Arch.ThreadLoweringProof.FoldedExit.applyUpdates "
            "M_T_0__t0_S0_entry_seq_0Updates store 0 = 0",
            lean,
        )
        self.assertIn(
            "Arch.ThreadLoweringProof.FoldedExit.applyUpdates "
            "M_T_0__t0_S0_entry_seq_0Updates store 1 = 1",
            lean,
        )

    def test_repeated_seq_assignment_proves_final_write(self) -> None:
        cert = base_cert()
        state = cert["modules"][0]["threads"][0]["states"][0]
        state["seq_updates"] = ["x <= true", "x <= false"]
        state["seq_assignments"] = [
            {"target": "x", "value": "true"},
            {"target": "x", "value": "false"},
        ]
        lean = cert_to_lean.render(cert)
        self.assertIn(
            "[Arch.ThreadLoweringProof.FoldedExit.setVar 0 0, "
            "Arch.ThreadLoweringProof.FoldedExit.setVar 0 1]",
            lean,
        )
        self.assertIn(
            "Arch.ThreadLoweringProof.FoldedExit.applyUpdates "
            "M_T_0__t0_S0_entry_seq_0Updates store 0 = 1",
            lean,
        )
        self.assertNotIn(
            "Arch.ThreadLoweringProof.FoldedExit.applyUpdates "
            "M_T_0__t0_S0_entry_seq_0Updates store 0 = 0",
            lean,
        )

    def test_once_terminal_non_emitted_target_maps_to_hold(self) -> None:
        cert = base_cert(target_index=1)
        thread = cert["modules"][0]["threads"][0]
        thread["once"] = True
        terminal = thread["states"][1]
        terminal["emitted"] = False
        terminal["source_next_index"] = 1
        terminal["source_next_name"] = "_t0_S1_action"
        terminal["source_transitions"] = [
            transition("always", 1, "_t0_S1_action"),
        ]
        terminal["transitions"] = [
            transition("always", 1, "_t0_S1_action"),
        ]

        lean = cert_to_lean.render(cert)
        self.assertIn("once := true", lean)
        self.assertIn(
            "{ actions := [0], control := Control.advance, target := 0 }",
            lean,
        )
        self.assertIn(
            "forall t, sourceTraceObs M_T_0Source inputs natInputs cfg0 t = "
            "fsmTraceObs M_T_0Fsm inputs natInputs cfg0 t",
            lean,
        )

    def test_folded_exit_multi_assignment_store_effects_are_emitted(self) -> None:
        cert = base_cert()
        state = cert["modules"][0]["threads"][0]["states"][0]
        state["seq_updates"] = []
        state["seq_assignments"] = []
        state["folded_exit_updates"] = ["x <= true", "y <= false"]
        state["folded_exit_assignments"] = [
            {"target": "x", "value": "true"},
            {"target": "y", "value": "false"},
        ]
        lean = cert_to_lean.render(cert)
        self.assertIn(
            "[Arch.ThreadLoweringProof.FoldedExit.setVar 0 0, "
            "Arch.ThreadLoweringProof.FoldedExit.setVar 1 1]",
            lean,
        )
        self.assertIn(
            "((Arch.ThreadLoweringProof.FoldedExit.sourceStep "
            "M_T_0__t0_S0_entry_folded_0Source env natEnv cfg).store 0 = 0)",
            lean,
        )
        self.assertIn(
            "((Arch.ThreadLoweringProof.FoldedExit.fsmStep "
            "M_T_0__t0_S0_entry_folded_0Fsm env natEnv cfg).store 1 = 1)",
            lean,
        )

    def test_repeated_folded_exit_assignment_proves_final_write(self) -> None:
        cert = base_cert()
        state = cert["modules"][0]["threads"][0]["states"][0]
        state["seq_updates"] = []
        state["seq_assignments"] = []
        state["folded_exit_updates"] = ["x <= true", "x <= false"]
        state["folded_exit_assignments"] = [
            {"target": "x", "value": "true"},
            {"target": "x", "value": "false"},
        ]
        lean = cert_to_lean.render(cert)
        self.assertIn(
            "[Arch.ThreadLoweringProof.FoldedExit.setVar 0 0, "
            "Arch.ThreadLoweringProof.FoldedExit.setVar 0 1]",
            lean,
        )
        self.assertIn(
            "((Arch.ThreadLoweringProof.FoldedExit.sourceStep "
            "M_T_0__t0_S0_entry_folded_0Source env natEnv cfg).store 0 = 1)",
            lean,
        )
        self.assertNotIn(
            "((Arch.ThreadLoweringProof.FoldedExit.sourceStep "
            "M_T_0__t0_S0_entry_folded_0Source env natEnv cfg).store 0 = 0)",
            lean,
        )

    def test_unsupported_folded_exit_update_is_rejected(self) -> None:
        cert = base_cert()
        state = cert["modules"][0]["threads"][0]["states"][0]
        state["folded_exit_updates"] = ["if req then done_r <= true"]
        state["folded_exit_assignments"] = []
        with self.assertRaises(SystemExit) as caught:
            cert_to_lean.render(cert)
        self.assertIn("folded_exit_updates require structured", str(caught.exception))

    def test_partial_folded_exit_assignment_coverage_is_rejected(self) -> None:
        cert = base_cert()
        state = cert["modules"][0]["threads"][0]["states"][0]
        state["folded_exit_updates"] = ["done_r <= true", "ack_r <= false"]
        state["folded_exit_assignments"] = [
            {"target": "done_r", "value": "true"},
        ]
        with self.assertRaises(SystemExit) as caught:
            cert_to_lean.render(cert)
        self.assertIn("folded_exit_updates require structured", str(caught.exception))


if __name__ == "__main__":
    unittest.main()
