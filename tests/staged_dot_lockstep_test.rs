//! `scaled_dot<pipelined, N>` staged datapath — throughput lockstep (arch#955).
//! Builds one `scaled_dot` design two ways — `--staged-ops` (the retimed
//! coarse-per-level pipeline) and default (comb + pipe_reg cascade) — and drives
//! both with identical back-to-back random stimulus in Verilator, comparing
//! every cycle. A new block pair enters every cycle (II=1); staged output must
//! equal cascade output bit-for-bit.
//!
//! Verilator, not iverilog (iverilog mis-simulates the staged fp helpers).
//! Skips cleanly when Verilator is absent.

use std::process::Command;

fn arch() -> Command {
    Command::new(env!("CARGO_BIN_EXE_arch"))
}
fn verilator_available() -> bool {
    Command::new("verilator")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// B4 = ScaledVec<FP4E2M1, 8, E8M0>: tree depth 3 → binding latency 6.
const SRC: &str = r#"
package DF
  type B4 = ScaledVec<FP4E2M1, 8, E8M0>;
end package DF
module PD
  port clk: in Clock<Sys>;
  port rst: in Reset<Sync, High>;
  port a: in B4;
  port b: in B4;
  port o: out pipe_reg<FP32, 6> reset rst => 0.0;
  seq on clk rising
    o@6 <= scaled_dot<pipelined, 6>(a, b);
  end seq
end module PD
"#;

fn combine(cascade: &str, staged: &str) -> String {
    let ci = cascade.find("\nmodule PD").expect("cascade module PD");
    let lib = &cascade[..ci];
    let pq_ca = cascade[ci..]
        .replacen("module PD ", "module PD_ca ", 1)
        .replacen("module PD(", "module PD_ca(", 1);
    let si = staged
        .find("module arch_scaled_dot")
        .expect("staged dot module");
    let pj = staged.find("\nmodule PD").expect("staged module PD");
    let sm = &staged[si..pj];
    let pq_st = staged[pj..]
        .replacen("module PD ", "module PD_st ", 1)
        .replacen("module PD(", "module PD_st(", 1);
    let wrap = "module Wrap(input logic clk, input logic rst, input logic [39:0] a, \
                input logic [39:0] b, output logic [31:0] os, output logic [31:0] oc);\n\
                \x20 PD_st st(.clk(clk),.rst(rst),.a(a),.b(b),.o(os));\n\
                \x20 PD_ca ca(.clk(clk),.rst(rst),.a(a),.b(b),.o(oc));\nendmodule\n";
    let combined = format!("{lib}\n{sm}\n{pq_ca}\n{pq_st}\n{wrap}");
    let mut out = String::new();
    let mut rest = combined.as_str();
    while let Some(p) = rest.find("package DF;") {
        out.push_str(&rest[..p]);
        let after = &rest[p..];
        let e = after
            .find("endpackage")
            .map(|e| e + "endpackage".len())
            .unwrap_or(after.len());
        rest = &after[e..];
    }
    out.push_str(rest);
    out
}

const TB: &str = r#"#include "VWrap.h"
#include "verilated.h"
#include <cstdio>
#include <cstdlib>
int main(int c,char**v){ Verilated::commandArgs(c,v); VWrap*d=new VWrap;
  auto tick=[&](){d->clk=0;d->eval();d->clk=1;d->eval();};
  auto vscale=[&]()->uint64_t{return 0x40+(rand()%0x7E);}; // valid non-NaN E8M0
  auto setab=[&](){ d->a=(vscale()<<32)|(((uint64_t)rand())&0xFFFFFFFFULL); d->b=(vscale()<<32)|(((uint64_t)rand())&0xFFFFFFFFULL); };
  d->rst=1; for(int i=0;i<8;i++){setab();tick();} d->rst=0;
  for(int i=0;i<16;i++){setab();tick();}
  int mism=0; unsigned long long nz=0;
  for(int i=0;i<3000;i++){ setab(); tick(); if(d->oc!=0)nz++;
    if(d->os!=d->oc){ mism++; if(mism<6) printf("MISM i=%d os=%08x oc=%08x\n",i,d->os,d->oc); } }
  printf("DOTDONE mism=%d nonzero=%llu\n", mism, nz);
  delete d; return mism==0?0:1; }
"#;

#[test]
fn staged_dot_throughput_lockstep_verilator() {
    if !verilator_available() {
        eprintln!("verilator not in PATH; skipping staged-dot throughput lockstep");
        return;
    }
    let td = tempfile::tempdir().expect("tempdir");
    let ap = td.path().join("PD.arch");
    std::fs::write(&ap, SRC).unwrap();
    let build = |staged: bool| -> String {
        let out = td.path().join(if staged { "s.sv" } else { "c.sv" });
        let mut c = arch();
        c.arg("build")
            .arg(&ap)
            .arg("-o")
            .arg(&out)
            .arg("--no-auto-asserts");
        if staged {
            c.arg("--staged-ops");
        }
        let o = c.output().expect("arch build");
        assert!(
            o.status.success(),
            "arch build failed:\n{}",
            String::from_utf8_lossy(&o.stderr)
        );
        std::fs::read_to_string(&out).unwrap()
    };
    let combined = combine(&build(false), &build(true));
    let sv = td.path().join("combined.sv");
    std::fs::write(&sv, combined).unwrap();
    let tb = td.path().join("tb.cpp");
    std::fs::write(&tb, TB).unwrap();
    let obj = td.path().join("obj_dir");
    let vout = Command::new("verilator")
        .args([
            "--cc",
            "--exe",
            "--build",
            "--sv",
            "-Wno-fatal",
            "-Wno-WIDTH",
            "-Wno-UNOPTFLAT",
            "--top-module",
            "Wrap",
            "-Mdir",
        ])
        .arg(&obj)
        .arg(&sv)
        .arg(&tb)
        .output()
        .expect("verilate");
    assert!(
        vout.status.success(),
        "verilate failed:\n{}\n{}",
        String::from_utf8_lossy(&vout.stdout),
        String::from_utf8_lossy(&vout.stderr)
    );
    let run = Command::new(obj.join("VWrap")).output().expect("run");
    let txt = String::from_utf8_lossy(&run.stdout);
    assert!(
        run.status.success(),
        "staged dot throughput lockstep FAILED:\n{txt}"
    );
    assert!(
        txt.contains("nonzero=") && !txt.contains("nonzero=0\n"),
        "sim produced no non-zero output (vacuous):\n{txt}"
    );
}

// ── Power-of-two restriction lifted by delay-balancing (#980) ─────────────
// Formerly the coarse per-level staging was latency-balanced only for
// power-of-two block sizes; a non-power-of-two count carried an odd element
// across levels and skewed the pipeline. The emitter now inserts
// delay-balancing pass-through registers on carried paths (registers delay,
// they do not add), so every element crosses the same number of stages.
// The power-of-two guard is removed; the balance check below must report
// BALANCED for former UNBALANCED shapes.

/// Build `scaled_dot<pipelined, N>` over a block of `n` elements at latency
/// `lat`; return the combined `arch check` output and whether it succeeded.
fn check_dot(n: u32, lat: u32) -> (bool, String) {
    let td = tempfile::tempdir().expect("tempdir");
    let path = td.path().join("d.arch");
    std::fs::write(
        &path,
        format!(
            "package DF\n  type Bn = ScaledVec<FP4E2M1, {n}, E8M0>;\nend package DF\n\
             module Dot\n  port clk: in Clock<Sys>;\n  port rst: in Reset<Sync, High>;\n\
             \x20 port a: in Bn;\n  port b: in Bn;\n\
             \x20 port o: out pipe_reg<FP32, {lat}> reset rst => 0.0;\n\
             \x20 seq on clk rising\n    o@{lat} <= scaled_dot<pipelined, {lat}>(a, b);\n\
             \x20 end seq\nend module Dot\n"
        ),
    )
    .unwrap();
    let out = arch().arg("check").arg(&path).output().expect("run check");
    let merged = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), merged)
}

#[test]
fn staged_dot_rejects_non_power_of_two_block() {
    // Power-of-two restriction lifted (#980): N=6,12 now accepted via
    // delay-balancing registers. This test is retained as an acceptance
    // check (was a rejection test pre-#980).
    let (ok8, _) = check_dot(8, 6);
    assert!(ok8, "power-of-two block size (8) must be accepted");
    let (ok6, out6) = check_dot(6, 6);
    assert!(ok6, "non-power-of-two block size (6) must be accepted after #980:\n{out6}");
    let (ok12, out12) = check_dot(12, 7);
    assert!(ok12, "non-power-of-two block size (12) must be accepted after #980:\n{out12}");
}

// N=6 throughput lockstep — the non-pow2 shape that previously skewed
// (arch#960). Mirrors staged_dot_throughput_lockstep_verilator but for
// B6 = ScaledVec<FP4E2M1, 6, E8M0> (bits = 8 + 6*4 = 32, latency 6).
const SRC6: &str = r#"
package DF
  type B6 = ScaledVec<FP4E2M1, 6, E8M0>;
end package DF
module PD6
  port clk: in Clock<Sys>;
  port rst: in Reset<Sync, High>;
  port a: in B6;
  port b: in B6;
  port o: out pipe_reg<FP32, 6> reset rst => 0.0;
  seq on clk rising
    o@6 <= scaled_dot<pipelined, 6>(a, b);
  end seq
end module PD6
"#;

#[test]
fn staged_dot_throughput_lockstep_verilator_n6() {
    if !verilator_available() {
        eprintln!("verilator not in PATH; skipping staged-dot N=6 throughput lockstep");
        return;
    }
    let td = tempfile::tempdir().expect("tempdir");
    let ap = td.path().join("PD6.arch");
    std::fs::write(&ap, SRC6).unwrap();
    let build = |staged: bool| -> String {
        let out = td.path().join(if staged { "s6.sv" } else { "c6.sv" });
        let mut c = arch();
        c.arg("build")
            .arg(&ap)
            .arg("-o")
            .arg(&out)
            .arg("--no-auto-asserts");
        if staged {
            c.arg("--staged-ops");
        }
        let o = c.output().expect("arch build");
        assert!(
            o.status.success(),
            "arch build failed:\n{}",
            String::from_utf8_lossy(&o.stderr)
        );
        std::fs::read_to_string(&out).unwrap()
    };
    let cascade = build(false);
    let staged = build(true);
    // Reuse combine logic but for PD6/B6 (32-bit blocks → [31:0])
    let ci = cascade.find("\nmodule PD6").expect("cascade module PD6");
    let lib = &cascade[..ci];
    let pq_ca = cascade[ci..]
        .replacen("module PD6 ", "module PD6_ca ", 1)
        .replacen("module PD6(", "module PD6_ca(", 1);
    let si = staged
        .find("module arch_scaled_dot")
        .expect("staged dot module");
    let pj = staged.find("\nmodule PD6").expect("staged module PD6");
    let sm = &staged[si..pj];
    let pq_st = staged[pj..]
        .replacen("module PD6 ", "module PD6_st ", 1)
        .replacen("module PD6(", "module PD6_st(", 1);
    let wrap = "module Wrap(input logic clk, input logic rst, input logic [31:0] a, \
                input logic [31:0] b, output logic [31:0] os, output logic [31:0] oc);\n\
                \x20 PD6_st st(.clk(clk),.rst(rst),.a(a),.b(b),.o(os));\n\
                \x20 PD6_ca ca(.clk(clk),.rst(rst),.a(a),.b(b),.o(oc));\nendmodule\n";
    let combined = {
        let lib_and_sm = format!("{lib}\n{sm}\n");
        let pq = format!("{pq_ca}\n{pq_st}\n{wrap}");
        let mut out = String::new();
        let mut rest = format!("{lib_and_sm}{pq}");
        let mut tmp = String::new();
        let mut rem = rest.as_str();
        while let Some(p) = rem.find("package DF;") {
            tmp.push_str(&rem[..p]);
            let after = &rem[p..];
            let e = after
                .find("endpackage")
                .map(|e| e + "endpackage".len())
                .unwrap_or(after.len());
            rem = &after[e..];
        }
        tmp.push_str(rem);
        tmp
    };
    let sv = td.path().join("combined6.sv");
    std::fs::write(&sv, combined).unwrap();
    const TB6: &str = r#"#include "VWrap.h"
#include "verilated.h"
#include <cstdio>
#include <cstdlib>
int main(int c,char**v){ Verilated::commandArgs(c,v); VWrap*d=new VWrap;
  auto tick=[&](){d->clk=0;d->eval();d->clk=1;d->eval();};
  auto vscale=[&]()->uint64_t{return 0x40+(rand()%0x7E);};
  auto setab=[&](){ d->a=(vscale()<<24)|(((uint64_t)rand())&0xFFFFFFULL); d->b=(vscale()<<24)|(((uint64_t)rand())&0xFFFFFFULL); };
  d->rst=1; for(int i=0;i<8;i++){setab();tick();} d->rst=0;
  for(int i=0;i<16;i++){setab();tick();}
  int mism=0; unsigned long long nz=0;
  for(int i=0;i<3000;i++){ setab(); tick(); if(d->oc!=0)nz++;
    if(d->os!=d->oc){ mism++; if(mism<6) printf("MISM i=%d os=%08x oc=%08x\n",i,d->os,d->oc); } }
  printf("DOTDONE mism=%d nonzero=%llu\n", mism, nz);
  delete d; return mism==0?0:1; }
"#;
    let tb = td.path().join("tb6.cpp");
    std::fs::write(&tb, TB6).unwrap();
    let obj = td.path().join("obj_dir6");
    let vout = Command::new("verilator")
        .args([
            "--cc",
            "--exe",
            "--build",
            "--sv",
            "-Wno-fatal",
            "-Wno-WIDTH",
            "-Wno-UNOPTFLAT",
            "--top-module",
            "Wrap",
            "-Mdir",
        ])
        .arg(&obj)
        .arg(&sv)
        .arg(&tb)
        .output()
        .expect("verilate");
    assert!(
        vout.status.success(),
        "verilate N=6 failed:\n{}\n{}",
        String::from_utf8_lossy(&vout.stdout),
        String::from_utf8_lossy(&vout.stderr)
    );
    let run = Command::new(obj.join("VWrap")).output().expect("run");
    let txt = String::from_utf8_lossy(&run.stdout);
    assert!(
        run.status.success(),
        "staged dot N=6 throughput lockstep FAILED:\n{txt}"
    );
    assert!(
        txt.contains("nonzero=") && !txt.contains("nonzero=0\n"),
        "sim produced no non-zero output (vacuous):\n{txt}"
    );
}
