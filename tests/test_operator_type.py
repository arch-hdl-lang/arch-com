#!/usr/bin/env python3
"""
Verification script for ARCH operator x data-type correctness.

Tests two things:
1. POSITIVE: arch build on operator_type_test.arch succeeds and the generated
   SV has correct wire widths, signedness, and cast expressions.
2. NEGATIVE: small inline .arch snippets that SHOULD fail type checking
   actually produce errors.
"""

import os
import re
import subprocess
import sys
import tempfile

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
ARCH_FILE = os.path.join(SCRIPT_DIR, "operator_type_test.arch")
SV_FILE = os.path.join(SCRIPT_DIR, "operator_type_test.sv")

# Find arch binary: prefer target/release, then target/debug, then PATH
REPO_ROOT = os.path.dirname(SCRIPT_DIR)
ARCH_BIN = None
for candidate in [
    os.path.join(REPO_ROOT, "target", "release", "arch"),
    os.path.join(REPO_ROOT, "target", "debug", "arch"),
]:
    if os.path.isfile(candidate) and os.access(candidate, os.X_OK):
        ARCH_BIN = candidate
        break
if ARCH_BIN is None:
    ARCH_BIN = "arch"  # fall back to PATH


# ─── Helpers ────────────────────────────────────────────────────────────

def run_arch_build():
    """Run arch build and return the .sv content."""
    result = subprocess.run(
        [ARCH_BIN, "build", ARCH_FILE],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        print(f"FAIL: arch build failed:\n{result.stderr}", file=sys.stderr)
        sys.exit(1)
    with open(SV_FILE) as f:
        return f.read()


def run_arch_check(arch_source):
    """Run arch check on inline source. Returns (returncode, stderr)."""
    with tempfile.NamedTemporaryFile(
        mode="w", suffix=".arch", delete=False
    ) as f:
        f.write(arch_source)
        f.flush()
        tmp = f.name
    try:
        result = subprocess.run(
            [ARCH_BIN, "check", tmp],
            capture_output=True,
            text=True,
        )
        return result.returncode, result.stderr + result.stdout
    finally:
        os.unlink(tmp)


def extract_declarations(sv_text):
    """Extract signal declarations: name -> (width_spec, is_signed).

    Width spec is one of:
      None      -> 1-bit (no [N:0])
      '[N:0]'   -> N+1 bits
    """
    decls = {}
    for m in re.finditer(
        r"logic\s+(signed\s+)?\[(\d+):0\]\s+(\w+)\s*;", sv_text
    ):
        signed = m.group(1) is not None
        width = int(m.group(2)) + 1
        name = m.group(3)
        decls[name] = {"width": width, "signed": signed}
    # 1-bit declarations: logic name; or logic signed name;
    for m in re.finditer(r"logic\s+(signed\s+)?(\w+)\s*;", sv_text):
        signed = m.group(1) is not None
        name = m.group(2)
        if name not in decls:  # don't override wider ones
            decls[name] = {"width": 1, "signed": signed}
    return decls


def extract_assigns(sv_text):
    """Extract assign statements: name -> RHS expression."""
    assigns = {}
    for m in re.finditer(r"assign\s+(\w+)\s*=\s*(.+);", sv_text):
        assigns[m.group(1)] = m.group(2).strip()
    return assigns


# ─── Positive test definitions ──────────────────────────────────────────
# Each: (signal_name, description, expected_width, expected_signed, rhs_checks)
# rhs_checks is a list of (pattern, description) that must match the RHS.

POSITIVE_TESTS = [
    # 1. Arithmetic widening
    ("arith_add_same", "UInt<8> + UInt<8> -> UInt<9>",
     9, False, []),
    ("arith_add_mixed", "UInt<8> + UInt<4> -> UInt<9>",
     9, False, []),
    ("arith_sub_same", "UInt<8> - UInt<8> -> UInt<9>",
     9, False, []),
    ("arith_mul_same", "UInt<8> * UInt<8> -> UInt<16>",
     16, False, []),
    ("arith_mul_mixed", "UInt<8> * UInt<4> -> UInt<12>",
     12, False, []),

    # 2. Wrapping operators
    ("wrap_add_same", "UInt<8> +% UInt<8> -> UInt<8>",
     8, False, [(r"8'\(a8 \+ b8\)", "width cast 8'(...)")]),
    ("wrap_sub_same", "UInt<8> -% UInt<8> -> UInt<8>",
     8, False, [(r"8'\(a8 - b8\)", "width cast 8'(...)")]),
    ("wrap_mul_same", "UInt<8> *% UInt<8> -> UInt<8>",
     8, False, [(r"8'\(a8 \* b8\)", "width cast 8'(...)")]),
    ("wrap_add_mixed", "UInt<8> +% UInt<4> -> UInt<8>",
     8, False, [(r"\(8 > 4 \? 8 : 4\)'\(a8 \+ c4\)", "max-width cast")]),
    ("wrap_add_comm", "UInt<4> +% UInt<8> -> UInt<8> (commutative)",
     8, False, [(r"\(4 > 8 \? 4 : 8\)'\(c4 \+ a8\)", "max-width cast")]),
    ("wrap_sub_mixed", "UInt<8> -% UInt<4> -> UInt<8>",
     8, False, [(r"a8 - c4", "subtraction present")]),
    ("wrap_mul_mixed", "UInt<8> *% UInt<4> -> UInt<8>",
     8, False, [(r"a8 \* c4", "multiplication present")]),

    # 3. Signed arithmetic
    ("sint_add", "SInt<8> + SInt<8> -> SInt<9>",
     9, True, []),
    ("sint_mul", "SInt<8> * SInt<8> -> SInt<16>",
     16, True, []),
    ("sint_cast_add", "signed(UInt<8>) + signed(UInt<8>) -> SInt<9>",
     9, True, [(r"\$signed\(a8\)", "$signed cast on a8"),
               (r"\$signed\(b8\)", "$signed cast on b8")]),

    # 4. Comparison results
    ("cmp_eq", "UInt<8> == UInt<8> -> Bool",
     1, False, [(r"a8 == b8", "equality comparison")]),
    ("cmp_lt", "UInt<8> < UInt<8> -> Bool",
     1, False, [(r"a8 < b8", "less-than comparison")]),
    ("cmp_ne", "UInt<8> != UInt<8> -> Bool",
     1, False, [(r"a8 != b8", "not-equal comparison")]),
    ("cmp_ge", "UInt<8> >= UInt<8> -> Bool",
     1, False, [(r"a8 >= b8", "greater-equal comparison")]),
    ("cmp_gt", "UInt<8> > UInt<8> -> Bool",
     1, False, [(r"a8 > b8", "greater-than comparison")]),

    # 5. Bitwise operators
    ("bit_and", "UInt<8> & UInt<8> -> UInt<8>",
     8, False, [(r"a8 & b8", "bitwise AND")]),
    ("bit_or", "UInt<8> | UInt<8> -> UInt<8>",
     8, False, [(r"a8 \| b8", "bitwise OR")]),
    ("bit_xor", "UInt<8> ^ UInt<8> -> UInt<8>",
     8, False, [(r"a8 \^ b8", "bitwise XOR")]),
    ("bit_not", "~UInt<8> -> UInt<8>",
     8, False, [(r"~a8", "bitwise NOT")]),

    # 6. Shift operators
    ("shift_left_var", "UInt<8> << UInt<3> -> UInt<8>",
     8, False, [(r"a8 << d3", "left shift")]),
    ("shift_right_var", "UInt<8> >> UInt<3> -> UInt<8>",
     8, False, [(r"a8 >> d3", "right shift")]),
    ("shift_left_lit", "UInt<8> << 4 -> UInt<8>",
     8, False, [(r"a8 << 4", "literal shift")]),

    # 7. Bool operations
    ("bool_and", "Bool & Bool -> Bool",
     1, False, [(r"bool_a & bool_b", "bool AND")]),
    ("bool_or", "Bool | Bool -> Bool",
     1, False, [(r"bool_a \| bool_b", "bool OR")]),
    ("bool_not", "~Bool -> Bool",
     1, False, [(r"~bool_a", "bool NOT")]),
    ("bool_xor", "Bool ^ Bool -> Bool",
     1, False, [(r"bool_a \^ bool_b", "bool XOR")]),

    # 8. Reduction operators
    ("red_and", "&UInt<8> -> Bool",
     1, False, [(r"&a8", "reduction AND")]),
    ("red_or", "|UInt<8> -> Bool",
     1, False, [(r"\|a8", "reduction OR")]),
    ("red_xor", "^UInt<8> -> Bool",
     1, False, [(r"\^a8", "reduction XOR")]),

    # 9. Width casts
    ("trunc_add", "(UInt<8>+UInt<8>).trunc<8>() -> UInt<8>",
     8, False, [(r"8'\(a8 \+ b8\)", "trunc width cast")]),
    ("zext_small", "UInt<4>.zext<8>() -> UInt<8>",
     8, False, [(r"\$unsigned\(c4\)", "zext uses $unsigned")]),
    ("zext_add", "zext<16>+zext<16> -> UInt<17>",
     17, False, [(r"16'\(\$unsigned\(a8\)\)", "zext cast on a8"),
                 (r"16'\(\$unsigned\(b8\)\)", "zext cast on b8")]),
    ("sext_test", "SInt<8>.sext<16>() -> SInt<16>",
     16, True, [(r"sa", "source signal present")]),

    # 10. Concatenation
    ("concat_same", "{UInt<8>, UInt<8>} -> UInt<16>",
     16, False, [(r"\{a8, b8\}", "concat expression")]),
    ("concat_diff", "{UInt<4>, UInt<4>} -> UInt<8>",
     8, False, [(r"\{c4, c4\}", "concat expression")]),
    ("concat_bool", "{Bool, UInt<7>} -> UInt<8>",
     8, False, [(r"\{bool_a,", "bool in concat")]),

    # 11. onehot
    ("onehot_test", "onehot(UInt<3>) -> UInt<8>",
     8, False, [(r"1 << d3", "shift-based onehot")]),
]


# ─── Negative test definitions ──────────────────────────────────────────
# Each: (description, arch_source, expected_error_substring)

NEGATIVE_TESTS = [
    (
        "Width mismatch: UInt<9> assigned to UInt<8>",
        """\
module NegWidth
  port a: in UInt<8>;
  port b: in UInt<8>;
  let bad: UInt<8> = a + b;
end module NegWidth
""",
        "type mismatch",
    ),
    (
        "Signedness mismatch: SInt<8> assigned to UInt<8>",
        """\
module NegSign
  port a: in SInt<8>;
  let bad: UInt<8> = a;
end module NegSign
""",
        "type mismatch",
    ),
    (
        ".trunc<N>() where N > source width (widens)",
        """\
module NegTruncWiden
  port a: in UInt<8>;
  let bad: UInt<16> = a.trunc<16>();
end module NegTruncWiden
""",
        "widens",
    ),
    (
        ".zext<N>() where N < source width (narrows)",
        """\
module NegZextNarrow
  port a: in UInt<8>;
  let bad: UInt<4> = a.zext<4>();
end module NegZextNarrow
""",
        "narrows",
    ),
    (
        ".sext<N>() where N < source width (narrows)",
        """\
module NegSextNarrow
  port a: in SInt<8>;
  let bad: SInt<4> = a.sext<4>();
end module NegSextNarrow
""",
        "narrows",
    ),
    (
        "Assigning wider to narrower without .trunc (mul result)",
        """\
module NegMulWidth
  port a: in UInt<8>;
  port b: in UInt<8>;
  let bad: UInt<8> = a * b;
end module NegMulWidth
""",
        "type mismatch",
    ),
]


# ─── Main ───────────────────────────────────────────────────────────────

def run_positive_tests(sv_text):
    """Run all positive tests. Returns (passed, failed, missing) counts."""
    decls = extract_declarations(sv_text)
    assigns = extract_assigns(sv_text)

    passed = 0
    failed = 0
    missing = 0

    for signal, desc, exp_width, exp_signed, rhs_checks in POSITIVE_TESTS:
        # Check declaration exists
        if signal not in decls:
            print(f"  MISS  {signal}: {desc}")
            print(f"         signal not found in generated SV declarations")
            missing += 1
            continue

        errors = []

        # Check width
        actual_width = decls[signal]["width"]
        if actual_width != exp_width:
            errors.append(
                f"width: expected {exp_width}, got {actual_width}"
            )

        # Check signedness
        actual_signed = decls[signal]["signed"]
        if actual_signed != exp_signed:
            errors.append(
                f"signed: expected {exp_signed}, got {actual_signed}"
            )

        # Check RHS patterns
        if signal in assigns:
            rhs = assigns[signal]
            for pattern, pat_desc in rhs_checks:
                if not re.search(pattern, rhs):
                    errors.append(
                        f"RHS pattern '{pat_desc}' not found in: {rhs}"
                    )
        elif rhs_checks:
            errors.append("no assign statement found for RHS checks")

        if errors:
            print(f"  FAIL  {signal}: {desc}")
            for e in errors:
                print(f"         {e}")
            failed += 1
        else:
            print(f"  PASS  {signal}: {desc}")
            passed += 1

    return passed, failed, missing


def run_negative_tests():
    """Run all negative tests. Returns (passed, failed) counts."""
    passed = 0
    failed = 0

    for desc, source, expected_err in NEGATIVE_TESTS:
        rc, output = run_arch_check(source)

        if rc != 0:
            # Compiler rejected it — good
            if expected_err and expected_err not in output:
                print(f"  FAIL  [neg] {desc}")
                print(f"         expected error containing '{expected_err}'")
                print(f"         got: {output.strip()[:200]}")
                failed += 1
            else:
                print(f"  PASS  [neg] {desc}")
                passed += 1
        else:
            print(f"  FAIL  [neg] {desc}")
            print(f"         expected compile error, but arch check succeeded")
            failed += 1

    return passed, failed


def main():
    print("=" * 70)
    print("ARCH Operator x Type Correctness Tests")
    print("=" * 70)

    # ── Phase 1: arch check on positive test ──
    print("\n--- Phase 1: Type-check positive test file ---")
    rc, output = run_arch_check(open(ARCH_FILE).read())
    if rc != 0:
        print(f"FAIL: arch check failed on {ARCH_FILE}:")
        print(output)
        sys.exit(1)
    print("  PASS  arch check succeeded (no type errors)")

    # ── Phase 2: arch build and verify SV output ──
    print("\n--- Phase 2: Build and verify SV output ---")
    sv_text = run_arch_build()
    pos_pass, pos_fail, pos_miss = run_positive_tests(sv_text)

    # ── Phase 3: Negative tests ──
    print("\n--- Phase 3: Negative type-checking tests ---")
    neg_pass, neg_fail = run_negative_tests()

    # ── Summary ──
    print("\n" + "=" * 70)
    total_pass = pos_pass + neg_pass + 1  # +1 for arch check
    total_fail = pos_fail + neg_fail
    total_miss = pos_miss
    total = total_pass + total_fail + total_miss
    print(
        f"Results: {total_pass}/{total} PASS, "
        f"{total_fail} FAIL, {total_miss} MISSING"
    )

    if total_fail > 0 or total_miss > 0:
        sys.exit(1)
    else:
        print("All operator x type tests passed.")
        sys.exit(0)


if __name__ == "__main__":
    main()
