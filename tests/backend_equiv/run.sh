#!/usr/bin/env bash
# Backend-equivalence torture-fixture runner.
#
# Each fixture deliberately STACKS 3–4 of the compiler's load-bearing
# composition axes (Vec<Bus>, thread+lock/mutex, generate, param widths,
# inst forwarding). Every one is checked (and the structural / sim cases
# built / simulated) so a regression at any feature *intersection* fails CI.
#
# Usage:  ARCH=path/to/arch  ./run.sh        (defaults to ../../target/release/arch)
#
# NOTE: this runs the arch-com side (check/build/sim soundness). The full
# sim<->SV trace-equivalence pass runs these same designs under
# `harc sim --check-backends` in harc-com CI.
set -u
ARCH="${ARCH:-$(cd "$(dirname "$0")/../.." && pwd)/target/release/arch}"
cd "$(dirname "$0")"
pass=0; fail=0
ck() { # ck <kind> <deps...> <top>
  local kind="$1"; shift
  local top="${@: -1}"
  local out rc
  case "$kind" in
    check) out=$("$ARCH" check "$@" 2>&1); rc=$?;;
    build) out=$("$ARCH" build "$@" -o /tmp/_be_$$.sv 2>&1); rc=$?; rm -f /tmp/_be_$$.sv;;
    sim)   echo 'int main(){return 0;}' >/tmp/_be_$$.cpp
           out=$("$ARCH" sim "$@" --tb /tmp/_be_$$.cpp --outdir /tmp/_be_$$ 2>&1); rc=$?
           rm -rf /tmp/_be_$$ /tmp/_be_$$.cpp;;
  esac
  if [ $rc -eq 0 ] && { [ "$kind" != check ] || echo "$out" | grep -q "OK: no errors"; }; then
    printf '  PASS %-6s %s\n' "$kind" "$top"; pass=$((pass+1))
  else
    printf '  FAIL %-6s %s\n%s\n' "$kind" "$top" "$out"; fail=$((fail+1))
  fi
}

# Parity-pass fixtures (clean interactions)
ck check BusVr.arch VrTap.arch        Fx1VecTapFabric.arch
ck check BusVr.arch                   Fx2GenThreadLock.arch
ck check BusVr.arch                   Fx3ThreadVecParamW.arch
ck check BusVr.arch                   Fx4MutexContendRR.arch
ck check BusVr.arch Fx5Sink.arch      Fx5WholeVecForward.arch
ck check BusVr.arch Fx7Latch.arch     Fx7ThreadFwdInst.arch
ck check BusVr.arch Fx8Inc.arch Fx8Dbl.arch Fx8GenIfSelect.arch
ck check BusVr.arch Fx8Inc.arch Fx8Dbl.arch Fx8GenIfSelectMode0.arch
ck check BusVr.arch                   Fx9MiniFabric.arch
ck check BusVr.arch                   Fx10MutexContendPrio.arch

# Regression fixtures — each caught a real Vec<Bus>-interaction bug
# (all now fixed; they must stay green):
ck check BusVr.arch VrTapScalar.arch  Fx1bMultiDriverBug.arch      # Bug 3 (#528): 1-D false multi-driver
ck check BusVr.arch Fx6Prod.arch Fx6Cons.arch Fx6Nested2D.arch     # Bug 4 (#533): 2-D nested false multi-driver
ck build BusVr.arch CrashSink.arch    Fx5bWholeVecForwardCrash.arch # Bug 1 (77bd55e): arch-build stack overflow
ck sim   BusVr.arch                   Fx3bVarIndexVecBusBug.arch     # Bug 2 (5fab9d6/f2c7e38): var-index Vec<Bus> sim
ck sim   BusVr.arch                   Fx3bVarIndexVecBusThread.arch
ck sim   BusVr.arch                   Fx3bVarIndexVecBusThreadWrite.arch

echo "=== backend_equiv: $pass passed, $fail failed ==="
[ $fail -eq 0 ]
