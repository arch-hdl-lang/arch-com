#!/usr/bin/env python3
"""Post-process sv2v output: lift `MacRes_b_16_b_16_34(...)[15:0]`
into intermediate wires so yosys 0.64's V2005 parser accepts the file.
"""
import re
import sys

FUNC = "MacRes_b_16_b_16_34"
src = open(sys.argv[1]).read()

def find_next_call_with_slice(s, start):
    """Find next FUNC call followed by [...] slice, starting at `start`.
       Return (call_start, after_slice, args, slice) or None.
    """
    i = start
    while True:
        p = s.find(FUNC, i)
        if p == -1:
            return None
        # Must be followed by '(' (call site, not decl or pure-name use).
        if p + len(FUNC) >= len(s) or s[p + len(FUNC)] != '(':
            i = p + 1
            continue
        # Walk paren-balance.
        depth = 1
        j = p + len(FUNC) + 1
        while j < len(s) and depth > 0:
            if s[j] == '(':
                depth += 1
            elif s[j] == ')':
                depth -= 1
            j += 1
        if depth != 0:
            return None
        # j now points just past the closing ')'.
        if j < len(s) and s[j] == '[':
            k = s.find(']', j)
            if k == -1:
                return None
            args = s[p + len(FUNC) + 1 : j - 1]
            slc  = s[j : k + 1]
            return (p, k + 1, args, slc)
        # call without slice — leave it alone.
        i = j
        continue

calls = []
pos = 0
while True:
    r = find_next_call_with_slice(src, pos)
    if r is None:
        break
    calls.append(r)
    pos = r[1]

m = re.search(r"module\s+_ibex_multdiv_fast_threads.*?;\n", src, re.DOTALL)
if not m:
    sys.stderr.write("ERROR: could not locate threads module header\n")
    sys.exit(1)
inject_at = m.end()

decls = []
out = []
last = 0
for idx, (cs, ae, args, slc) in enumerate(calls):
    wname = f"_macres_call_{idx}"
    decls.append(f"  wire [33:0] {wname} = {FUNC}({args});")
    out.append(src[last:cs])
    out.append(f"{wname}{slc}")
    last = ae
out.append(src[last:])
joined = "".join(out)
# inject_at is offset into src before mutation; since all calls occur AFTER
# the header (we verified by examining first call pos 3909 > header end),
# inject_at is still valid in `joined`.
final = joined[:inject_at] + "\n" + "\n".join(decls) + "\n" + joined[inject_at:]
sys.stdout.write(final)
sys.stderr.write(f"lifted {len(calls)} func-call-slice sites\n")
