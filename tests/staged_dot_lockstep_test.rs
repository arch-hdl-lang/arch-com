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
