#!/usr/bin/env python3
"""Translate a straight-line ARCH FP datapath (comb `function` body OR a
combinationalized staged module body) to SMT2 over UNINTERPRETED fp primitives.
The primitives (arch_f32_mul/add/sub/fma, arch_*_to_f32) become (declare-fun ...),
so equality of two datapaths reduces to congruence over their wiring — no
arithmetic bit-blasting. Import `build_side`; the miter script ties two together.
"""
import re

_TOK = re.compile(r'\s*(\+:|[():,\[\]]|[A-Za-z_][A-Za-z0-9_]*|\d+)')

def _toks(s):
    out, i = [], 0
    while i < len(s):
        m = _TOK.match(s, i)
        if not m:
            raise SyntaxError(f"bad token near {s[i:i+20]!r}")
        out.append(m.group(1)); i = m.end()
    return out

class _P:
    def __init__(self, ts): self.ts, self.i = ts, 0
    def peek(self): return self.ts[self.i] if self.i < len(self.ts) else None
    def eat(self, x=None):
        t = self.ts[self.i]; self.i += 1
        if x is not None and t != x:
            raise SyntaxError(f"want {x!r} got {t!r}")
        return t

def _parse(p, width, ufs):
    """Return (smt_expr, bitwidth). `width` maps input/wire name -> bits."""
    t = p.eat()
    if p.peek() == '(':                         # CALL: fp primitive
        p.eat('(')
        args, argw = [], []
        while True:
            e, w = _parse(p, width, ufs)
            args.append(e); argw.append(w)
            if p.peek() == ',':
                p.eat(','); continue
            break
        p.eat(')')
        ufs[t] = (tuple(argw), 32)              # every primitive returns FP32
        return f"({t} {' '.join(args)})", 32
    if p.peek() == '[':                         # SLICE of an input
        p.eat('[')
        a = int(p.eat()); sep = p.eat(); bnum = int(p.eat()); p.eat(']')
        if sep == '+:':
            lo, w = a, bnum; top = lo + w - 1
        else:
            top, lo = a, bnum; w = top - lo + 1
        return f"((_ extract {top} {lo}) {t})", w
    # bare identifier: an input (raw) or an earlier wire (w_-prefixed)
    if t in width and width[t] == 'input':
        return t, None
    return f"w_{t}", width.get(t)

def build_side(body, inputs, output_lhs, prefix):
    """body: SV text with `LHS <= RHS;` / `LHS = RHS;` statements (<= == treated
    the same — the staged registers are shorted). inputs: {name: bits}.
    output_lhs: wire holding the datapath output. prefix: unique tag for wire defs.
    Returns (smt_lines, ufs, out_symbol)."""
    width = {}
    for n, b in inputs.items():
        width[n] = 'input'
    decl = {}
    for m in re.finditer(r'logic\s*\[(\d+):0\]\s*([A-Za-z_][A-Za-z0-9_]*)\s*;', body):
        decl[m.group(2)] = int(m.group(1)) + 1
    ufs, lines, seen = {}, [], set()
    for lhs, rhs in re.findall(r'([A-Za-z_][A-Za-z0-9_]*)\s*(?:<=|=)\s*([^;]+);', body):
        if lhs in inputs or lhs in seen:
            continue
        seen.add(lhs)
        # a plain copy `x <= y` (reg short) with y a bare ident/slice is handled by _parse
        expr, w = _parse(_P(_toks(rhs.strip())), width, ufs)
        rw = decl.get(lhs, w or 32)
        width[lhs] = rw
        lines.append(f"(define-fun {prefix}_w_{lhs} () (_ BitVec {rw}) {_pref(expr, prefix)})")
    return lines, ufs, f"{prefix}_w_{output_lhs}"

def _pref(expr, prefix):
    # prefix wire references `w_foo` -> `PREFIX_w_foo` (keep inputs a/b and (_ extract) raw)
    return re.sub(r'\bw_([A-Za-z0-9_]+)', rf'{prefix}_w_\1', expr)


def build_miter(comb_sv, comb_fn, staged_sv, staged_mod, output_port='y',
                inputs=None, mutate=None):
    """Emit a QF_UFBV miter proving the combinationalized staged datapath
    (`staged_mod` in `staged_sv`, registers shorted) computes the same
    composition of uninterpreted fp primitives as the combinational operator
    (`comb_fn` function in `comb_sv`). `unsat` = identical wiring for all inputs.
    `mutate(text)->text` optionally corrupts the staged SV first (non-vacuity
    check: a real wiring change must flip the verdict to `sat`)."""
    import re as _re
    inputs = inputs or {'a': 40, 'b': 40}
    csrc = open(comb_sv).read()
    fn = _re.search(r'function automatic logic \[31:0\] ' + _re.escape(comb_fn)
                    + r'\(([^)]*)\);(.*?)endfunction', csrc, _re.DOTALL)
    if not fn:
        raise SystemExit(f"comb function {comb_fn} not found in {comb_sv}")
    cdefs, cufs, cout = build_side(fn.group(2), inputs, comb_fn, 'C')

    ssrc = open(staged_sv).read()
    if mutate:
        ssrc = mutate(ssrc)
    mod = _re.search(r'module ' + _re.escape(staged_mod) + r'\b.*?endmodule',
                     ssrc, _re.DOTALL)
    if not mod:
        raise SystemExit(f"staged module {staged_mod} not found in {staged_sv}")
    sdefs, sufs, sout = build_side(mod.group(0), inputs, output_port, 'S')

    ufs = {**cufs, **sufs}
    out = ["(set-logic QF_UFBV)"]
    for name, (argw, rw) in sorted(ufs.items()):
        dom = ' '.join(f"(_ BitVec {w})" for w in argw)
        out.append(f"(declare-fun {name} ({dom}) (_ BitVec {rw}))")
    for n, b in inputs.items():
        out.append(f"(declare-const {n} (_ BitVec {b}))")
    out += cdefs + sdefs
    out.append(f"(assert (not (= {sout} {cout})))")
    out.append("(check-sat)")
    return '\n'.join(out) + '\n'


# ── mutations for the non-vacuity self-check ────────────────────────────────
def _mut_add_operand(text):
    # corrupt one reduction-tree add operand (a wiring change that must flip the
    # verdict to `sat`). Target a staged tree add by its register-named operands
    # `arch_f32_add(rX, rY)` — helper-function bodies never use those names — and
    # rewire the second input to the first (self-add), changing the value.
    import re as _re
    return _re.sub(r'(arch_f32_add\((r\w+), )(r\w+)(\))',
                   lambda m: f"{m.group(1)}{m.group(2)}{m.group(4)}"
                   if m.group(2) != m.group(3) else m.group(0),
                   text, count=1)


MUTATIONS = {'add-operand': _mut_add_operand}


def _main(argv):
    import subprocess as sp
    import sys as _sys
    ap = argv[1:]
    if len(ap) < 4:
        print("usage: uf_datapath.py COMB_SV COMB_FN STAGED_SV STAGED_MOD "
              "[OUTPUT_PORT] [--mutate KIND]", file=_sys.stderr)
        return 2
    comb_sv, comb_fn, staged_sv, staged_mod = ap[:4]
    port = 'y'
    mutate = None
    rest = ap[4:]
    i = 0
    while i < len(rest):
        if rest[i] == '--mutate':
            mutate = MUTATIONS[rest[i + 1]]; i += 2
        else:
            port = rest[i]; i += 1
    smt = build_miter(comb_sv, comb_fn, staged_sv, staged_mod, port, mutate=mutate)
    r = sp.run(['z3', '-in', '-T:120'], input=smt, capture_output=True, text=True)
    print((r.stdout + r.stderr).strip().splitlines()[-1] if (r.stdout or r.stderr) else "error")
    return 0


if __name__ == '__main__':
    import sys
    sys.exit(_main(sys.argv))
