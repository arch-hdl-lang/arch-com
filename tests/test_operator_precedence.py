#!/usr/bin/env python3
"""
Verification script for ARCH operator precedence codegen.

Runs `arch build` on operator_precedence_test.arch, reads the generated .sv,
and checks each `assign` line against expected patterns.

Key invariants tested:
  - Same-operator chains emit NO extra parens
  - Mixed comparison + bitwise ALWAYS get parens around comparisons
  - Mixed bitwise ops (& vs | vs ^) get parens around the tighter group
  - Arithmetic vs comparison follows natural SV precedence
  - Unary ops bind tighter than binary
  - Ternary expressions are correctly parenthesized in context
  - Wrapping operators produce width casts
"""

import os
import re
import subprocess
import sys

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
ARCH_FILE = os.path.join(SCRIPT_DIR, "operator_precedence_test.arch")
SV_FILE = os.path.join(SCRIPT_DIR, "operator_precedence_test.sv")

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


def build():
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


def extract_assigns(sv_text):
    """Extract a dict of signal_name -> RHS expression from assign statements."""
    assigns = {}
    for m in re.finditer(r"assign\s+(\w+)\s*=\s*(.+);", sv_text):
        assigns[m.group(1)] = m.group(2).strip()
    return assigns


# ─── Test case definitions ───────────────────────────────────────────────
# Each entry: (signal_name, description, check_function)
#
# check_function(rhs: str) -> (bool, str)
#   Returns (passed, reason).

def has_no_parens(rhs):
    """The RHS should have no parentheses at all (except width casts)."""
    # Strip width casts like W'(...) which are fine
    stripped = re.sub(r"\w+'\(.*?\)", "", rhs)
    if "(" in stripped or ")" in stripped:
        return False, f"unexpected parens in '{rhs}'"
    return True, ""


def exact_match(expected):
    """RHS must match expected string exactly."""
    def check(rhs):
        if rhs == expected:
            return True, ""
        return False, f"expected '{expected}', got '{rhs}'"
    return check


def contains(pattern, desc=None):
    """RHS must contain the given regex pattern."""
    def check(rhs):
        if re.search(pattern, rhs):
            return True, ""
        return False, f"expected pattern '{desc or pattern}' not found in '{rhs}'"
    return check


def not_contains(pattern, desc=None):
    """RHS must NOT contain the given regex pattern."""
    def check(rhs):
        if re.search(pattern, rhs):
            return False, f"unexpected pattern '{desc or pattern}' found in '{rhs}'"
        return True, ""
    return check


def all_of(*checks):
    """All check functions must pass."""
    def check(rhs):
        for c in checks:
            ok, reason = c(rhs)
            if not ok:
                return False, reason
        return True, ""
    return check


def parens_around_cmp_in_bitwise():
    """Comparison sub-expressions inside bitwise ops must be parenthesized."""
    def check(rhs):
        # Find bitwise operators that have comparison operands
        # The key pattern: (x == y) & (z == w) — parens around comparisons
        # BAD pattern: x == y & z == w — no parens, SV would parse as x == (y & z) == w
        cmp_ops = ["==", "!=", "<", ">", ">="]
        bit_ops = ["&", "|", "^"]

        # Check that every comparison operator has its operands grouped in parens
        # Simple heuristic: if we see `== ... &` or `& ... ==` outside parens,
        # the comparisons are not protected.
        # More reliable: check that each cmp op is inside a paren group
        # when a bitwise op is also present.

        has_bitwise = any(f" {op} " in rhs for op in bit_ops)
        has_cmp = any(f" {op} " in rhs for op in cmp_ops)

        if not (has_bitwise and has_cmp):
            return True, ""  # not applicable

        # Each comparison should be wrapped: (... cmp_op ...)
        # Strategy: remove all parenthesized groups and check no cmp ops remain
        # next to bitwise ops
        simplified = rhs
        # Iteratively remove innermost parens groups
        while True:
            new = re.sub(r"\([^()]*\)", "GRP", simplified)
            if new == simplified:
                break
            simplified = new

        # In the simplified (parens removed) form, there should be no
        # comparison operators adjacent to bitwise operators
        for cmp in cmp_ops:
            for bit in bit_ops:
                # Pattern: cmp_op ... bit_op or bit_op ... cmp_op
                if f" {cmp} " in simplified and f" {bit} " in simplified:
                    return False, (
                        f"comparison '{cmp}' not parenthesized when mixed with "
                        f"bitwise '{bit}': simplified='{simplified}'"
                    )

        return True, ""
    return check


def parens_around_different_bitwise():
    """When different bitwise ops are mixed, inner groups must be parenthesized."""
    def check(rhs):
        bit_ops = set()
        for op in ["&", "|", "^"]:
            if f" {op} " in rhs:
                bit_ops.add(op)

        if len(bit_ops) < 2:
            return True, ""  # only one bitwise op, no mixing

        # Remove paren groups and check only one bitwise op remains
        simplified = rhs
        while True:
            new = re.sub(r"\([^()]*\)", "GRP", simplified)
            if new == simplified:
                break
            simplified = new

        remaining_ops = set()
        for op in ["&", "|", "^"]:
            if f" {op} " in simplified:
                remaining_ops.add(op)

        if len(remaining_ops) > 1:
            return False, (
                f"mixed bitwise ops not properly parenthesized: "
                f"simplified='{simplified}'"
            )
        return True, ""
    return check


# ─── Test cases ──────────────────────────────────────────────────────────

TESTS = [
    # 1. Same-operator chains
    ("chain_and", "& chain: no parens needed",
     exact_match("a & b & c")),
    ("chain_or", "| chain: no parens needed",
     exact_match("a | b | c")),
    ("chain_xor", "^ chain: no parens needed",
     exact_match("a ^ b ^ c")),
    ("chain_add", "+% chain: wrapping width cast",
     contains(r"W'\(")),

    # 2. Mixed comparison + bitwise (parens REQUIRED)
    ("cmp_and_eq", "(a == b) & (c == d): parens around comparisons",
     all_of(
         contains(r"\(a == b\)"),
         contains(r"\(c == d\)"),
         parens_around_cmp_in_bitwise(),
     )),
    ("cmp_or_neq", "(a != b) | (c != d): parens around comparisons",
     all_of(
         contains(r"\(a != b\)"),
         contains(r"\(c != d\)"),
         parens_around_cmp_in_bitwise(),
     )),
    ("cmp_and_lt_gt", "(a < b) & (c > d): parens around comparisons",
     all_of(
         contains(r"\(a < b\)"),
         contains(r"\(c > d\)"),
         parens_around_cmp_in_bitwise(),
     )),
    ("cmp_or_gte", "(a >= b) | (d >= c): parens around comparisons",
     all_of(
         contains(r"\(a >= b\)"),
         contains(r"\(d >= c\)"),
         parens_around_cmp_in_bitwise(),
     )),
    ("cmp_and_three", "triple (==) & (==) & (==): all comparisons parenthesized",
     all_of(
         contains(r"\(a == b\)"),
         contains(r"\(c == d\)"),
         contains(r"\(e == f\)"),
         parens_around_cmp_in_bitwise(),
     )),

    # 3. Mixed bitwise operators
    ("mix_and_or", "(a & b) | (c & d): & groups parenthesized inside |",
     all_of(
         contains(r"\(a & b\)"),
         contains(r"\(c & d\)"),
         parens_around_different_bitwise(),
     )),
    ("mix_or_xor", "(a | b) ^ (c | d): | groups parenthesized inside ^",
     all_of(
         contains(r"\(a \| b\)"),
         contains(r"\(c \| d\)"),
         parens_around_different_bitwise(),
     )),
    ("mix_xor_and", "(a ^ b) & (c ^ d): ^ groups parenthesized inside &",
     all_of(
         contains(r"\(a \^ b\)"),
         contains(r"\(c \^ d\)"),
         parens_around_different_bitwise(),
     )),
    ("mix_or_and", "(a | b) & (c | d): | groups parenthesized inside &",
     all_of(
         contains(r"\(a \| b\)"),
         contains(r"\(c \| d\)"),
         parens_around_different_bitwise(),
     )),
    ("mix_xor_or", "(a ^ b) | (c ^ d): ^ groups parenthesized inside |",
     all_of(
         contains(r"\(a \^ b\)"),
         contains(r"\(c \^ d\)"),
         parens_around_different_bitwise(),
     )),
    ("mix_and_xor", "(a & b) ^ (c & d): & groups parenthesized inside ^",
     all_of(
         contains(r"\(a & b\)"),
         contains(r"\(c & d\)"),
         parens_around_different_bitwise(),
     )),

    # 4. Arithmetic vs comparison (natural SV precedence)
    #    The zext calls produce width-cast parens like (W + 1)'($unsigned(a)),
    #    but the + and == are NOT additionally parenthesized.
    ("arith_cmp_add", "a+b == c: addition binds tighter than == in SV",
     all_of(
         contains(r"\+.*=="),  # + before ==
         # The outer expression has `... + ... == ...` without extra grouping
         # parens around the addition.  Width-cast parens (W+1)'(...) are fine.
         not_contains(r"\)\s*\+\s*\S+\)\s*==", "no manual grouping parens around addition"),
     )),
    ("arith_cmp_mul", "a*b < c+d: mul/add bind tighter than < in SV",
     all_of(
         contains(r"\*.*<"),   # * before <
         contains(r"<.*\+"),   # + after <
     )),

    # 5. Shift operators
    ("shift_left", "a << b: simple shift",
     exact_match("a << b")),
    ("shift_right", "a >> b: simple shift",
     exact_match("a >> b")),
    ("shift_plus", "(a << b) + c: shift in addition context",
     contains(r"<<.*\+")),

    # 6. Logical operators
    ("logic_and_or", "a==b && c==d || e==f: logical ops",
     all_of(
         contains(r"==.*&&.*==.*\|\|.*=="),
     )),
    ("logic_or_and", "a==b || c==d && e==f: logical ops",
     all_of(
         contains(r"==.*\|\|.*==.*&&.*=="),
     )),

    # 7. Unary operators
    ("unary_not_and", "~a & b: bitwise NOT has higher precedence",
     exact_match("~a & b")),
    ("unary_not_group", "~(a & b): NOT on grouped expression",
     exact_match("~(a & b)")),
    ("unary_reduct_and", "&a: reduction AND",
     exact_match("&a")),
    ("unary_reduct_or", "|a: reduction OR",
     exact_match("|a")),
    ("unary_reduct_xor", "^a: reduction XOR",
     exact_match("^a")),

    # 8. Ternary
    ("tern_simple", "sel ? a : b: simple ternary",
     exact_match("sel ? a : b")),
    ("tern_in_add", "ternary in addition: wrapping op context",
     contains(r"\?\s*a\s*:\s*b")),
    ("tern_cmp_cond", "(a == b) ? c : d: comparison as ternary condition",
     all_of(
         contains(r"=="),
         contains(r"\?\s*c\s*:\s*d"),
     )),

    # 9. Method calls
    ("meth_trunc_eq", "trunc result == c",
     all_of(
         contains(r"W'\("),  # width cast from trunc
         contains(r"==\s*c"),
     )),
    ("meth_zext_add", "wrapping add",
     contains(r"W'\(a \+ b\)")),

    # 10. Wrapping operators
    ("wrap_add_eq", "(a +% b) == c: wrapping add compared",
     all_of(
         contains(r"W'\(a \+ b\)"),
         contains(r"==\s*c"),
     )),
    ("wrap_chain", "a +% b +% c: chained wrapping add",
     all_of(
         contains(r"W'\("),
         contains(r"\+.*\+"),
     )),
    ("wrap_sub_eq", "(a -% b) == (c -% d): wrapping sub compared",
     all_of(
         contains(r"W'\(a - b\)"),
         contains(r"W'\(c - d\)"),
         contains(r"=="),
     )),

    # 11. Complex nested
    ("nested_cmp_bitwise", "((a==b)&(c==d)) | ((e==f)&(a!=c)): nested cmp+bitwise",
     all_of(
         contains(r"\(a == b\)"),
         contains(r"\(c == d\)"),
         contains(r"\(e == f\)"),
         contains(r"\(a != c\)"),
         parens_around_cmp_in_bitwise(),
     )),
    ("nested_multi_level", "((a&b)|(c&d)) ^ ((e&f)|(a&c)): nested mixed bitwise",
     all_of(
         parens_around_different_bitwise(),
         contains(r"\(a & b\)"),
         contains(r"\(c & d\)"),
         contains(r"\(e & f\)"),
         contains(r"\(a & c\)"),
     )),

    # 12. More comparison + bitwise combos
    ("cmp_neq_and", "(a != b) & (c != d): != with &",
     all_of(
         contains(r"\(a != b\)"),
         contains(r"\(c != d\)"),
         parens_around_cmp_in_bitwise(),
     )),
    ("cmp_lt_or", "(a < b) | (c < d): < with |",
     all_of(
         contains(r"\(a < b\)"),
         contains(r"\(c < d\)"),
         parens_around_cmp_in_bitwise(),
     )),
    ("cmp_gt_xor", "(a > b) ^ (c > d): > with ^",
     all_of(
         contains(r"\(a > b\)"),
         contains(r"\(c > d\)"),
         parens_around_cmp_in_bitwise(),
     )),

    # 13. Arithmetic in shift amount
    ("shift_arith_amt", "a << (b+c).trunc: shift with complex amount",
     all_of(
         contains(r"<<"),
         contains(r"W'\(b \+ c\)"),
     )),

    # 14. Kitchen sink
    ("kitchen_sink", "mixed arith+cmp+bitwise+logical",
     all_of(
         parens_around_cmp_in_bitwise(),
         contains(r"=="),
         contains(r"[>]"),
         contains(r"!="),
     )),
]


def main():
    print(f"Building {ARCH_FILE}...")
    sv_text = build()
    assigns = extract_assigns(sv_text)

    passed = 0
    failed = 0
    missing = 0

    for signal, desc, check_fn in TESTS:
        if signal not in assigns:
            print(f"  MISS  {signal}: {desc}")
            print(f"         signal not found in generated SV")
            missing += 1
            continue

        rhs = assigns[signal]
        ok, reason = check_fn(rhs)
        if ok:
            print(f"  PASS  {signal}: {desc}")
            passed += 1
        else:
            print(f"  FAIL  {signal}: {desc}")
            print(f"         {reason}")
            print(f"         actual: assign {signal} = {rhs};")
            failed += 1

    print()
    total = passed + failed + missing
    print(f"Results: {passed}/{total} PASS, {failed} FAIL, {missing} MISSING")

    if failed > 0 or missing > 0:
        sys.exit(1)
    else:
        print("All operator precedence tests passed.")
        sys.exit(0)


if __name__ == "__main__":
    main()
