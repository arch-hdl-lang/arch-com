#!/usr/bin/env python3
"""Check the emitted Lean FP model against the emitted SMT-LIB definitions.

This is an artifact-level QF_UFBV check, not a Lean-kernel theorem.  The small
Lean subset parser below translates each *actual* generated `let` RHS into
SMT-LIB; Z3 compares it with the corresponding RHS in `dump_fp smt`, treating
prior SSA values and called functions as arbitrary.  Unsat for every node,
matching signatures, and matching final references imply whole-function
correspondence by induction over SSA order and the function-call DAG.

Trusted here: this script's Lean-subset parser and semantic translation, the
SMT-LIB parser, Z3, and Lean's elaboration of Model.lean.  The freshness check
prevents a stale checked-in model, but does not prove the Rust printer correct.
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass
from pathlib import Path
import re
import subprocess
import sys


LEAN_ONLY = {
    "arch_decode_mant", "arch_decode_eunb", "arch_round48",
    "arch_msb_index48", "arch_round470", "arch_msb_index470",
    "arch_round98", "arch_msb_index98", "arch_fma_mag", "arch_fma_elo",
    "arch_fma_sign", "arch_fma_mag98", "arch_fma_elo98",
    "arch_fma_sign98", "arch_fma_f32_ref",
}


class CheckError(Exception):
    pass


def require(ok: bool, message: str) -> None:
    if not ok:
        raise CheckError(message)


def tokenize_sexpr(source: str) -> list[str]:
    source = "\n".join(line.split(";", 1)[0] for line in source.splitlines())
    return re.findall(r"\(|\)|[^\s()]+", source)


def parse_sexprs(source: str) -> list:
    tokens = tokenize_sexpr(source)
    pos = 0

    def one():
        nonlocal pos
        require(pos < len(tokens), "unexpected end of SMT-LIB")
        token = tokens[pos]
        pos += 1
        if token != "(":
            require(token != ")", "unexpected ')' in SMT-LIB")
            return token
        items = []
        while pos < len(tokens) and tokens[pos] != ")":
            items.append(one())
        require(pos < len(tokens), "unclosed '(' in SMT-LIB")
        pos += 1
        return items

    result = []
    while pos < len(tokens):
        result.append(one())
    return result


def print_sexpr(expr) -> str:
    if isinstance(expr, str):
        return expr
    return "(" + " ".join(map(print_sexpr, expr)) + ")"


def bv_sort_width(sort) -> int:
    require(isinstance(sort, list) and len(sort) == 3 and sort[:2] == ["_", "BitVec"],
            f"not a bit-vector sort: {sort}")
    width = int(sort[2])
    require(width > 0, f"invalid bit-vector width: {width}")
    return width


@dataclass
class Function:
    name: str
    params: list[tuple[str, int]]
    ret: int
    nodes: list[tuple[str, int | None, object]]
    result: object


def parse_smt(source: str) -> dict[str, Function]:
    functions = {}
    for form in parse_sexprs(source):
        require(isinstance(form, list) and len(form) == 5 and form[0] == "define-fun",
                f"expected only define-fun forms, got: {print_sexpr(form)[:100]}")
        _, name, raw_params, sort, body = form
        require(name not in functions, f"duplicate SMT function {name}")
        params = [(p[0], bv_sort_width(p[1])) for p in raw_params]
        nodes = []
        while isinstance(body, list) and len(body) == 3 and body[0] == "let":
            bindings = body[1]
            require(len(bindings) == 1 and len(bindings[0]) == 2,
                    f"non-SSA SMT let in {name}")
            node, rhs = bindings[0]
            nodes.append((node, None, rhs))
            body = body[2]
        functions[name] = Function(name, params, bv_sort_width(sort), nodes, body)
    return functions


DEF = re.compile(r"def (\w+) ((?:\(\w+ : BitVec \d+\) )*): BitVec (\d+) :=")
PARAM = re.compile(r"\((\w+) : BitVec (\d+)\)")
LET = re.compile(r"  let (_t\d+) : BitVec (\d+) := (.+)")


def parse_lean(source: str) -> dict[str, Function]:
    functions = {}
    current = None
    saw_namespace = False
    saw_end = False
    for line in source.splitlines():
        if not line or line.startswith("--"):
            continue
        if line == "namespace ArchFp":
            require(not saw_namespace, "duplicate ArchFp namespace")
            saw_namespace = True
            continue
        if line == "end ArchFp":
            require(current is not None, "empty Lean model")
            saw_end = True
            current = None
            continue
        match = DEF.fullmatch(line)
        if match:
            name, param_text, ret = match.groups()
            require(name not in functions, f"duplicate Lean function {name}")
            current = Function(name, [(p, int(w)) for p, w in PARAM.findall(param_text)],
                               int(ret), [], None)
            functions[name] = current
            continue
        require(current is not None and not saw_end, f"unexpected Lean line: {line}")
        match = LET.fullmatch(line)
        if match:
            node, width, rhs = match.groups()
            require(node == f"_t{len(current.nodes)}", f"noncontiguous SSA node {current.name}.{node}")
            current.nodes.append((node, int(width), rhs))
        else:
            require(line.startswith("  ") and current.result is None,
                    f"unexpected Lean body line: {line}")
            current.result = line.strip()
    require(saw_namespace and saw_end, "missing ArchFp namespace delimiters")
    require(all(f.result is not None for f in functions.values()), "Lean function missing result")
    return functions


@dataclass
class Term:
    smt: str
    width: int | None  # None is Bool; -1 is Nat obtained from BitVec.toNat.


class LeanExprParser:
    def __init__(self, source: str, variables: dict[str, int],
                 signatures: dict[str, Function]):
        self.tokens = re.findall(r"\(|\)|[^\s()]+", source)
        self.pos = 0
        self.variables = variables
        self.signatures = signatures

    def peek(self) -> str | None:
        return self.tokens[self.pos] if self.pos < len(self.tokens) else None

    def take(self, expected: str | None = None) -> str:
        require(self.pos < len(self.tokens), "unexpected end of Lean expression")
        token = self.tokens[self.pos]
        self.pos += 1
        if expected is not None:
            require(token == expected, f"expected {expected!r}, got {token!r}")
        return token

    def number(self) -> int:
        token = self.take()
        require(token.isdecimal(), f"expected decimal number, got {token!r}")
        return int(token)

    def parse(self) -> Term:
        result = self.expr()
        require(self.pos == len(self.tokens), f"trailing Lean tokens: {self.tokens[self.pos:]}")
        return result

    def expr(self) -> Term:
        token = self.take()
        if token != "(":
            if token.endswith(".toNat"):
                name = token[:-6]
                require(name in self.variables, f"unknown shift amount {name}")
                return Term(name, -1)
            require(token in self.variables, f"unknown Lean variable {token}")
            return Term(token, self.variables[token])

        head = self.peek()
        if head == "BitVec.ofNat":
            self.take()
            width, value = self.number(), self.number()
            require(width > 0, "zero-width BitVec.ofNat")
            out = Term(f"(_ bv{value} {width})", width)
        elif head == "BitVec.extractLsb":
            self.take()
            hi, lo = self.number(), self.number()
            value = self.expr()
            require(value.width is not None and value.width > 0 and 0 <= lo <= hi < value.width,
                    "invalid Lean extract width")
            out = Term(f"((_ extract {hi} {lo}) {value.smt})", hi - lo + 1)
        elif head == "BitVec.setWidth":
            self.take()
            width = self.number()
            value = self.expr()
            require(value.width is not None and value.width > 0 and width >= value.width,
                    "unexpected narrowing in generated Lean setWidth")
            out = Term(f"((_ zero_extend {width - value.width}) {value.smt})", width)
        elif head == "BitVec.ofBool":
            self.take()
            pred = self.expr()
            require(pred.width is None, "BitVec.ofBool argument is not Bool")
            out = Term(f"(ite {pred.smt} #b1 #b0)", 1)
        elif head in {"BitVec.ult", "BitVec.ule", "BitVec.slt", "BitVec.sle"}:
            op = self.take().split(".")[1]
            a, b = self.expr(), self.expr()
            require(a.width is not None and a.width > 0 and a.width == b.width,
                    f"mismatched Lean {op} operands")
            out = Term(f"(bv{op} {a.smt} {b.smt})", None)
        elif head == "if":
            self.take()
            cond = self.expr()
            op = self.take()
            require(op in {"==", "!="}, "unsupported Lean if condition")
            rhs = self.expr()
            require(cond.width is not None and cond.width > 0 and cond.width == rhs.width,
                    "mismatched Lean if condition")
            self.take("then")
            yes = self.expr()
            self.take("else")
            no = self.expr()
            require(yes.width is not None and yes.width > 0 and yes.width == no.width,
                    "mismatched Lean if branches")
            pred = f"(= {cond.smt} {rhs.smt})"
            if op == "!=":
                pred = f"(not {pred})"
            out = Term(f"(ite {pred} {yes.smt} {no.smt})", yes.width)
        elif head == "~~~":
            self.take()
            arg = self.expr()
            require(arg.width is not None and arg.width > 0, "bitwise not of non-BitVec")
            out = Term(f"(bvnot {arg.smt})", arg.width)
        elif head in self.signatures:
            name = self.take()
            sig = self.signatures[name]
            args = [self.expr() for _ in sig.params]
            require([a.width for a in args] == [w for _, w in sig.params],
                    f"wrong Lean call signature for {name}")
            out = Term(f"({name} {' '.join(a.smt for a in args)})", sig.ret)
        else:
            a = self.expr()
            op = self.take()
            b = self.expr()
            binary = {
                "+": "bvadd", "-": "bvsub", "*": "bvmul",
                "&&&": "bvand", "|||": "bvor", "^^^": "bvxor",
                "<<<": "bvshl", ">>>": "bvlshr", "++": "concat",
            }
            if op in binary:
                require(a.width is not None and a.width > 0, f"invalid {op} lhs")
                if op in {"<<<", ">>>"}:
                    require(b.width == -1 and b.smt in self.variables and
                            self.variables[b.smt] == a.width, "invalid Lean shift amount")
                else:
                    require(b.width is not None and b.width > 0, f"invalid {op} rhs")
                    if op != "++":
                        require(a.width == b.width, f"mismatched {op} widths")
                width = a.width + b.width if op == "++" else a.width
                out = Term(f"({binary[op]} {a.smt} {b.smt})", width)
            elif op in {"==", "!="}:
                require(a.width is not None and a.width > 0 and a.width == b.width,
                        f"mismatched Lean {op} operands")
                pred = f"(= {a.smt} {b.smt})"
                out = Term(pred if op == "==" else f"(not {pred})", None)
            else:
                raise CheckError(f"unsupported Lean operator {op!r}")
        self.take(")")
        if self.peek() == ".toNat":
            self.take()
            require(out.width is not None and out.width > 0,
                    ".toNat on non-BitVec")
            return Term(out.smt, -1)
        return out


def compare_function(lean: Function, smt: Function,
                     signatures: dict[str, Function], z3: str) -> int:
    require(lean.params == smt.params and lean.ret == smt.ret,
            f"signature mismatch for {lean.name}")
    require(len(lean.nodes) == len(smt.nodes),
            f"SSA node count mismatch for {lean.name}: {len(lean.nodes)} vs {len(smt.nodes)}")
    variables = dict(lean.params)
    clauses = ["(set-logic QF_UFBV)", "(set-option :timeout 10000)"]
    for sig in signatures.values():
        args = " ".join(f"(_ BitVec {w})" for _, w in sig.params)
        clauses.append(f"(declare-fun {sig.name} ({args}) (_ BitVec {sig.ret}))")
    for name, width in lean.params:
        clauses.append(f"(declare-const {name} (_ BitVec {width}))")
    for name, width, _ in lean.nodes:
        clauses.append(f"(declare-const {name} (_ BitVec {width}))")
    sites = []
    for (lname, width, lrhs), (sname, _, srhs) in zip(lean.nodes, smt.nodes):
        require(lname == sname, f"SSA name mismatch: {lean.name}.{lname} vs {sname}")
        translated = LeanExprParser(lrhs, variables, signatures).parse()
        require(translated.width == width, f"Lean width mismatch at {lean.name}.{lname}")
        sites.append(lname)
        clauses += ["(push 1)",
                    f"(assert (not (= {print_sexpr(srhs)} {translated.smt})))",
                    "(check-sat)", "(pop 1)"]
        variables[lname] = width
    final = LeanExprParser(lean.result, variables, signatures).parse()
    require(final.width == lean.ret, f"Lean return width mismatch in {lean.name}")
    sites.append("result")
    clauses += ["(push 1)",
                f"(assert (not (= {print_sexpr(smt.result)} {final.smt})))",
                "(check-sat)", "(pop 1)"]
    proc = subprocess.run([z3, "-in"], input="\n".join(clauses) + "\n",
                          text=True, capture_output=True, timeout=600, check=False)
    verdicts = proc.stdout.splitlines()
    require(proc.returncode == 0 and not proc.stderr.strip() and
            len(verdicts) == len(sites),
            f"Z3 error in {lean.name}: {proc.stderr[:400]} {proc.stdout[:400]}")
    for site, verdict in zip(sites, verdicts):
        require(verdict == "unsat", f"{lean.name}.{site}: expected unsat, got {verdict}")
    return len(sites)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dump-fp", type=Path, required=True,
                        help="built examples/dump_fp binary")
    parser.add_argument("--model", type=Path, required=True,
                        help="checked-in generated Model.lean")
    parser.add_argument("--z3", default="z3")
    parser.add_argument("--lean", default="lean",
                        help="Lean executable used to elaborate emitted model")
    parser.add_argument("--no-freshness-check", action="store_true",
                        help="only for mutation tests: compare supplied Model.lean as-is")
    args = parser.parse_args()

    model = args.model.read_text()
    snapshot_functions = parse_lean(model)
    lean_functions = snapshot_functions
    generated_only = 0
    if not args.no_freshness_check:
        generated = subprocess.run([str(args.dump_fp), "lean"], text=True,
                                   capture_output=True, check=True).stdout
        emitted_functions = parse_lean(generated)
        for name, definition in snapshot_functions.items():
            require(emitted_functions.get(name) == definition,
                    f"checked-in Lean definition {name} differs from freshly emitted dump_fp lean")
        generated_only = len(emitted_functions) - len(snapshot_functions)
        # Also check the functions added since the proof snapshot was taken.
        # They have no existing Lean theorem, but the renderer correspondence
        # should not silently exclude them.
        lean_functions = emitted_functions
    smt_source = subprocess.run([str(args.dump_fp), "smt"], text=True,
                                capture_output=True, check=True).stdout
    smt_elaboration = subprocess.run([args.z3, "-in"],
                                     input=smt_source + "\n(check-sat)\n",
                                     text=True, capture_output=True, timeout=120,
                                     check=False)
    require(smt_elaboration.returncode == 0 and
            smt_elaboration.stdout.strip() == "sat" and
            not smt_elaboration.stderr.strip(),
            f"emitted SMT-LIB does not parse/typecheck: "
            f"{smt_elaboration.stdout[:500]} {smt_elaboration.stderr[:500]}")
    smt_functions = parse_smt(smt_source)
    lean_only = set(lean_functions) - set(smt_functions)
    require(lean_only == LEAN_ONLY,
            f"unexpected Lean-only functions: {sorted(lean_only ^ LEAN_ONLY)}")
    smt_only = set(smt_functions) - set(lean_functions)
    require(not smt_only,
            f"unexpected SMT-only functions (missing Lean counterparts): {sorted(smt_only)}")
    lean_source = generated if not args.no_freshness_check else model
    elaboration = subprocess.run([args.lean, "--stdin"], input=lean_source,
                                 cwd=Path(__file__).resolve().parents[1],
                                 text=True, capture_output=True, timeout=120,
                                 check=False)
    require(elaboration.returncode == 0,
            f"emitted Lean does not elaborate: {elaboration.stdout[:500]} "
            f"{elaboration.stderr[:500]}")
    common = [name for name in lean_functions if name in smt_functions]
    require(common, "no common Lean/SMT functions")
    # Every call in the common Lean functions must target a common function;
    # the parser rejects unknown names, and SMT checks reject name/arity drift.
    signatures = {name: smt_functions[name] for name in common}
    checked = 0
    for name in common:
        n = compare_function(lean_functions[name], smt_functions[name], signatures, args.z3)
        checked += n
        print(f"{name}: {n} SSA/result obligations unsat", flush=True)
    print(f"PASS: {len(common)} common functions, {checked} local obligations; "
          f"{len(smt_only)} SMT-only and {len(lean_only)} "
          "Lean-only functions explicitly outside this correspondence check; "
          f"{generated_only} checked functions are not in the Lean proof snapshot")


if __name__ == "__main__":
    try:
        main()
    except (CheckError, subprocess.TimeoutExpired, subprocess.CalledProcessError) as exc:
        print(f"FAIL: {exc}", file=sys.stderr)
        sys.exit(1)
