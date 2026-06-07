#!/usr/bin/env bash
# Rising-edge sample table of the AR + R handshake windows from the
# latency-test VCD.
#
# Historically this script was the diagnostic for a 1-cycle thread-FSM
# bubble between
#   - rising K   : `M.ar_valid` first observed high
#   - rising K+1 : `S.ar_valid` finally also high → handshake fires
# i.e. the master drove valid, but the slave-side thread's entry-wait
# state ate one cycle before propagating.
#
# With `wait 0+ cycle until X; do .. until Y;` Mealy fusion in the v2
# MasterPort / SlavePort, that bubble is gone: the master and slave
# valid both rise on the same posedge and the handshake fires in the
# same cycle. The script now serves as a regression probe — if a future
# change re-introduces a bubble, the two handshakes below split across
# rising edges again and the asymmetry is visible at a glance.
#
# Usage: from the repo root,
#   arch sim --wave examples/nic400/fab_latency.vcd \
#     examples/nic400/Nic400Fabric.arch \
#     examples/nic400/Nic400MasterPort.arch \
#     examples/nic400/Nic400SlavePort.arch \
#     examples/nic400/BusAxi4.arch \
#     --tb examples/nic400/tb_nic400_fabric_latency.cpp \
#     -o /tmp/fab_wave
#   examples/nic400/probe_ar_bubble.sh examples/nic400/fab_latency.vcd
#
# Or open the VCD directly in a viewer (gtkwave / surfer).

set -e
VCD=${1:-examples/nic400/fab_latency.vcd}
[ -r "$VCD" ] || { echo "VCD file '$VCD' not found"; exit 1; }

awk '
BEGIN {
  # AR-forward window: TB drives m_0_ar_valid + s_0_ar_ready around t=15.
  # R-return  window: TB drives s_0_r_valid + m_0_r_ready around t=21.
  ar_tmin = 13; ar_tmax = 17
  r_tmin  = 19; r_tmax  = 23
  w["s0"]="clk"
  # AR (master side / slave side)
  w["s2"]="m_0_ar_valid";  w["s3"]="m_0_ar_ready";  w["s5"]="m_0_ar_id"
  w["s38"]="s_0_ar_valid"; w["s39"]="s_0_ar_ready"; w["s41"]="s_0_ar_id"
  # R (slave side / master side)
  w["s50"]="s_0_r_valid";  w["s51"]="s_0_r_ready";  w["s53"]="s_0_r_id"
  w["s14"]="m_0_r_valid";  w["s15"]="m_0_r_ready";  w["s17"]="m_0_r_id"
  printf "── AR forward (M → S) ─────────────────────────────────────────\n"
}
function dump_ar(    msg) {
  msg = ""
  if (val["s2"]=="1" && val["s38"]=="1") msg = " <- AR handshake (same cycle)"
  printf "  rising t=%2d  M.ar_v=%s ar_r=%s ar_id=%s    S.ar_v=%s ar_r=%s ar_id=%s%s\n",
    t, val["s2"], val["s3"], val["s5"], val["s38"], val["s39"], val["s41"], msg
}
function dump_r(    msg) {
  msg = ""
  if (val["s50"]=="1" && val["s14"]=="1") msg = " <- R handshake (same cycle)"
  printf "  rising t=%2d  S.r_v=%s  r_r=%s  r_id=%s    M.r_v=%s  r_r=%s  r_id=%s%s\n",
    t, val["s50"], val["s51"], val["s53"], val["s14"], val["s15"], val["s17"], msg
}
/^#/ {
  if (t!="" && val["s0"]=="1") {
    if (t>=ar_tmin && t<=ar_tmax) dump_ar()
    if (t==r_tmin) printf "── R return (S → M) ──────────────────────────────────────────\n"
    if (t>=r_tmin && t<=r_tmax) dump_r()
  }
  t = substr($0,2)+0; next
}
/^[01][a-z]/ { v=substr($0,1,1); id=substr($0,2); val[id]=v }
/^b/ { split($0,p," "); v=p[1]; sub(/^b/,"",v); val[p[2]]=v }
END {
  if (t!="" && val["s0"]=="1") {
    if (t>=ar_tmin && t<=ar_tmax) dump_ar()
    if (t>=r_tmin && t<=r_tmax) dump_r()
  }
}
' "$VCD"
