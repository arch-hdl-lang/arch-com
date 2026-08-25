//! `scaled_quantize<Fmt, pipelined, N>` staged datapath — throughput lockstep
//! (arch#955). Builds one design two ways — `--staged-ops` (the retimed staged
//! block) and default (the comb+cascade form) — and drives both with the SAME
//! back-to-back random stimulus in Verilator, comparing every cycle. A new
//! input enters every cycle (II=1); the staged output must equal the cascade
//! output bit-for-bit.
//!
//! Verilator, not iverilog: iverilog mis-simulates the staged multiply's
//! constant bit-selects (`sorry: ... all bits will be included`), so it cannot
//! validate these modules. Skips cleanly when Verilator is absent.

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

const SRC: &str = r#"
package QF
  type B4 = ScaledVec<FP4E2M1, 8, E8M0>;
end package QF
module PQ
  port clk: in Clock<Sys>;
  port rst: in Reset<Sync, High>;
  port v: in Vec<FP32, 8>;
  port y: out pipe_reg<B4, 5> reset rst => 0;
  seq on clk rising
    y@5 <= scaled_quantize<B4, pipelined, 5>(v);
  end seq
end module PQ
"#;

/// Combine the two builds into one Verilator-compilable file: the cascade
/// build supplies the shared file-scope function library (including the comb
/// quantize) and `PQ_ca`; the staged build supplies its two staged modules and
/// `PQ_st`. Empty `package` blocks are stripped.
fn combine(cascade: &str, staged: &str) -> String {
    let ci = cascade.find("\nmodule PQ").expect("cascade has module PQ");
    let lib = &cascade[..ci];
    let pq_ca = cascade[ci..]
        .replacen("module PQ ", "module PQ_ca ", 1)
        .replacen("module PQ(", "module PQ_ca(", 1);
    let si = staged
        .find("module ArchF32MulStaged4")
        .expect("staged has mul module");
    let pj = staged.find("\nmodule PQ").expect("staged has module PQ");
    let staged_mods = &staged[si..pj];
    let pq_st = staged[pj..]
        .replacen("module PQ ", "module PQ_st ", 1)
        .replacen("module PQ(", "module PQ_st(", 1);
    let wrap = "module Wrap(input logic clk, input logic rst, input logic [255:0] v, \
                output logic [39:0] ys, output logic [39:0] yc);\n\
                \x20 PQ_st st(.clk(clk),.rst(rst),.v(v),.y(ys));\n\
                \x20 PQ_ca ca(.clk(clk),.rst(rst),.v(v),.y(yc));\nendmodule\n";
    let combined = format!("{lib}\n{staged_mods}\n{pq_ca}\n{pq_st}\n{wrap}");
    // strip empty `package QF; ... endpackage`
    let mut out = String::new();
    let mut rest = combined.as_str();
    while let Some(p) = rest.find("package QF;") {
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
int main(int argc, char** argv){
  Verilated::commandArgs(argc, argv);
  VWrap* d = new VWrap;
  auto tick = [&](){ d->clk=0; d->eval(); d->clk=1; d->eval(); };
  auto setv=[&](){ for(int w=0;w<8;w++) ((uint32_t*)&d->v)[w]=(uint32_t)rand(); };
  d->rst=1; for(int i=0;i<8;i++){ setv(); tick(); }
  d->rst=0;
  for(int i=0;i<16;i++){ setv(); tick(); }   // fill both pipelines
  int mism=0; unsigned long long nonzero=0;
  for(int i=0;i<3000;i++){
    setv(); tick();
    if(d->yc!=0) nonzero++;
    if(d->ys != d->yc){ mism++; if(mism<6) printf("MISM i=%d ys=%010llx yc=%010llx\n", i,(unsigned long long)d->ys,(unsigned long long)d->yc); }
  }
  printf("VDONE mism=%d nonzero=%llu\n", mism, nonzero);
  delete d; return mism==0 ? 0 : 1;
}
"#;

#[test]
fn staged_quantize_throughput_lockstep_verilator() {
    if !verilator_available() {
        eprintln!("verilator not in PATH; skipping staged-quantize throughput lockstep");
        return;
    }
    let td = tempfile::tempdir().expect("tempdir");
    let ap = td.path().join("PQ.arch");
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
    assert!(run.status.success(), "throughput lockstep FAILED:\n{txt}");
    assert!(
        txt.contains("nonzero=") && !txt.contains("nonzero=0\n"),
        "sim produced no non-zero output (vacuous):\n{txt}"
    );
}
