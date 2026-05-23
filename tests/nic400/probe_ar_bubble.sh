#!/usr/bin/env bash
# Generate a rising-edge sample table of the AR handshake window from the
# latency-test VCD. Pinpoints the 1-cycle bubble between
#   - rising K   : `M.ar_valid` first observed high
#   - rising K+1 : `S.ar_valid` finally also high → handshake fires
#
# Usage: from the repo root,
#   arch sim --wave tests/nic400/fab_latency.vcd \
#     tests/nic400/Nic400Fabric.arch \
#     tests/nic400/Nic400MasterPort.arch \
#     tests/nic400/Nic400SlavePort.arch \
#     tests/nic400/BusAxi4.arch \
#     --tb tests/nic400/tb_nic400_fabric_latency.cpp \
#     -o /tmp/fab_wave
#   tests/nic400/probe_ar_bubble.sh tests/nic400/fab_latency.vcd
#
# Or open the VCD directly in a viewer (gtkwave / surfer).

set -e
VCD=${1:-tests/nic400/fab_latency.vcd}
[ -r "$VCD" ] || { echo "VCD file '$VCD' not found"; exit 1; }

awk '
BEGIN {
  tmin = 13; tmax = 22
  w["s0"]="clk"
  w["s2"]="M.ar_valid"; w["s3"]="M.ar_ready"; w["s5"]="M.ar_id"
  w["s38"]="S.ar_valid"; w["s39"]="S.ar_ready"; w["s41"]="S.ar_id"
}
/^#/ {
  if (t!="" && t>=tmin && t<=tmax && val["s0"]=="1") {
    printf "rising t=%2d  M.ar_v=%s  M.ar_r=%s  M.ar_id=%s    S.ar_v=%s  S.ar_r=%s  S.ar_id=%s\n",
      t, val["s2"], val["s3"], val["s5"], val["s38"], val["s39"], val["s41"]
  }
  t = substr($0,2)+0; next
}
/^[01][a-z]/ { v=substr($0,1,1); id=substr($0,2); val[id]=v }
/^b/ { split($0,p," "); v=p[1]; sub(/^b/,"",v); val[p[2]]=v }
END {
  if (t!="" && t>=tmin && t<=tmax && val["s0"]=="1") {
    printf "rising t=%2d  M.ar_v=%s  M.ar_r=%s  M.ar_id=%s    S.ar_v=%s  S.ar_r=%s  S.ar_id=%s\n",
      t, val["s2"], val["s3"], val["s5"], val["s38"], val["s39"], val["s41"]
  }
}
' "$VCD"
