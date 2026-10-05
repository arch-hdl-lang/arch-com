#!/usr/bin/env python3
"""Positive run and semantic mutation test for the Lean↔SMT renderer check."""

from pathlib import Path
import re
import subprocess
import sys
import tempfile


REPO = Path(__file__).resolve().parents[3]
CHECK = REPO / "proofs/lean_fp_equiv/scripts/check_renderer_correspondence.py"
MODEL = REPO / "proofs/lean_fp_equiv/ArchFpEquiv/Model.lean"


def run(binary: Path, model: Path, *, mutated: bool = False) -> subprocess.CompletedProcess:
    command = [sys.executable, str(CHECK), "--dump-fp", str(binary),
               "--model", str(model)]
    if mutated:
        command.append("--no-freshness-check")
    return subprocess.run(command, cwd=REPO, text=True, capture_output=True,
                          timeout=180, check=False)


def main() -> None:
    binary = (Path(sys.argv[1]) if len(sys.argv) > 1
              else REPO / "target/release/examples/dump_fp").resolve()
    good = run(binary, MODEL)
    if good.returncode != 0 or not re.search(
            r"PASS: \d+ common functions, \d+ local obligations", good.stdout):
        raise SystemExit(f"positive correspondence check failed:\n{good.stdout}\n{good.stderr}")

    original = subprocess.run([str(binary), "lean"], text=True, capture_output=True,
                              timeout=120, check=True).stdout
    if not original.startswith("namespace ArchFp\n"):
        raise SystemExit("dump_fp did not emit the expected Lean namespace")
    mutations = [
        ("let _t4 : BitVec 1 := (_t1 &&& _t3)",
         "let _t4 : BitVec 1 := (_t1 ||| _t3)", "arch_f32_canon._t4"),
        ("let _t44 : BitVec 48 := (_t38 <<< _t43.toNat)",
         "let _t44 : BitVec 48 := (_t38 >>> _t43.toNat)", "arch_f32_mul._t44"),
        ("let _t90 : BitVec 1 := (BitVec.ofBool (BitVec.sle _t89 (BitVec.ofNat 16 0)))",
         "let _t90 : BitVec 1 := (BitVec.ofBool (BitVec.ule _t89 (BitVec.ofNat 16 0)))",
         "arch_f32_mul._t90"),
    ]
    with tempfile.TemporaryDirectory(prefix="arch-lean-smt-mutation-") as tmp:
        model = Path(tmp) / "MutatedModel.lean"
        for before, after, site in mutations:
            if before not in original:
                raise SystemExit(f"mutation target {site} has drifted; update the test")
            model.write_text(original.replace(before, after, 1))
            bad = run(binary, model, mutated=True)
            if bad.returncode == 0 or site not in bad.stderr or "got sat" not in bad.stderr:
                raise SystemExit(f"mutation {site} was not rejected:\n"
                                 f"{bad.stdout}\n{bad.stderr}")
        marker = "def arch_f32_canon "
        start = original.find(marker)
        if start < 0:
            raise SystemExit("missing function-deletion mutation target")
        end = original.find("\ndef ", start)
        if end < 0:
            raise SystemExit("function-deletion mutation target has no successor")
        model.write_text(original[:start] + original[end + 1:])
        bad = run(binary, model, mutated=True)
        if bad.returncode == 0 or "unexpected SMT-only functions" not in bad.stderr:
            raise SystemExit(f"missing Lean function was not rejected:\n"
                             f"{bad.stdout}\n{bad.stderr}")
    print("PASS: emitted Lean↔SMT artifacts agree; AND, shift, signed-compare, and function-deletion mutations are rejected")


if __name__ == "__main__":
    main()
