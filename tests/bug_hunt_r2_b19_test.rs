//! Regression coverage for B19: parent-driven synchronizer models must
//! detect their own source and destination clock edges.

use std::process::Command;

const FIXTURE: &str = "tests/rdc/rdc_m5_cdc_distinct_sources_ok.arch";
const TOP_SOURCE: &str = r#"domain Src
  freq_mhz: 100
end domain Src
domain Dst
  freq_mhz: 200
end domain Dst
synchronizer FlagSync
  kind ff;
  param STAGES: const = 2;
  port src_clk: in Clock<Src>;
  port dst_clk: in Clock<Dst>;
  port data_in: in Bool;
  port data_out: out Bool;
end synchronizer FlagSync
"#;
const RESET_SOURCE: &str = r#"domain Src
  freq_mhz: 100
end domain Src
domain Dst
  freq_mhz: 200
end domain Dst
synchronizer FlagSync
  kind ff;
  param STAGES: const = 2;
  port src_clk: in Clock<Src>;
  port dst_clk: in Clock<Dst>;
  port rst: in Reset<Sync>;
  port data_in: in Bool;
  port data_out: out Bool;
end synchronizer FlagSync
"#;

fn arch() -> Command {
    Command::new(env!("CARGO_BIN_EXE_arch"))
}

fn compile_generated(out_dir: &std::path::Path, tb: &std::path::Path, exe: &std::path::Path) {
    let mut cpp_files = std::fs::read_dir(out_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "cpp"))
        .collect::<Vec<_>>();
    cpp_files.sort();
    let compile = Command::new("clang++")
        .arg("-std=c++20")
        .arg("-O0")
        .arg("-I")
        .arg(out_dir)
        .args(&cpp_files)
        .arg(tb)
        .arg("-o")
        .arg(exe)
        .output()
        .expect("compile generated model");
    assert!(
        compile.status.success(),
        "generated model should compile\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&compile.stdout),
        String::from_utf8_lossy(&compile.stderr)
    );
}

fn generate(source: &std::path::Path, out_dir: &std::path::Path) {
    let generated = arch()
        .arg("sim")
        .arg(source)
        .arg("--outdir")
        .arg(out_dir)
        .output()
        .expect("generate synchronizer model");
    assert!(
        generated.status.success(),
        "synchronizer codegen should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&generated.stdout),
        String::from_utf8_lossy(&generated.stderr)
    );
}

#[test]
fn parent_synchronizer_advances_only_on_destination_edges() {
    if Command::new("clang++").arg("--version").output().is_err() {
        eprintln!("skipping: clang++ not available on PATH");
        return;
    }
    let td = tempfile::tempdir().expect("create temporary output directory");
    let tb = td.path().join("tb.cpp");
    std::fs::write(
        &tb,
        r#"#include "VM.h"
static VM d;
static void edge(unsigned src, unsigned dst) {
  d.src_clk = src; d.dst_clk = dst; d.eval();
}
int main() {
  edge(0, 0); d.flag_a = 1; d.flag_b = 1;
  edge(1, 0); edge(0, 0);
  if (d.out_a || d.out_b) return 10;
  edge(0, 1); edge(0, 0);
  if (d.out_a || d.out_b) return 11;
  edge(0, 1); edge(0, 0);
  if (d.out_a != 1 || d.out_b != 1) return 12;
  d.flag_a = 0; d.flag_b = 0;
  edge(1, 0); edge(0, 0);
  if (d.out_a != 1 || d.out_b != 1) return 13;
  edge(0, 1); edge(0, 0);
  if (d.out_a != 1 || d.out_b != 1) return 14;
  edge(0, 1); edge(0, 0);
  if (d.out_a || d.out_b) return 15;
  return 0;
}
"#,
    )
    .unwrap();
    let out = arch()
        .arg("sim")
        .arg(FIXTURE)
        .arg("--tb")
        .arg(&tb)
        .arg("--outdir")
        .arg(td.path().join("build"))
        .output()
        .expect("run parent synchronizer simulation");
    assert!(
        out.status.success(),
        "parent synchronizer should propagate on destination edges only\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn top_level_synchronizer_eval_tracks_edges_once() {
    if Command::new("clang++").arg("--version").output().is_err() {
        eprintln!("skipping: clang++ not available on PATH");
        return;
    }
    let td = tempfile::tempdir().expect("create temporary output directory");
    let source = td.path().join("top.arch");
    let out_dir = td.path().join("build");
    std::fs::write(&source, TOP_SOURCE).unwrap();
    generate(&source, &out_dir);
    let tb = td.path().join("tb.cpp");
    std::fs::write(
        &tb,
        r#"#include "VFlagSync.h"
static void edge(VFlagSync& d, unsigned src, unsigned dst) {
  d.src_clk = src; d.dst_clk = dst; d.eval();
}
int main() {
  VFlagSync d; d.data_in = 0; edge(d, 0, 0); d.data_in = 1;
  edge(d, 1, 0); edge(d, 0, 0); edge(d, 0, 1);
  if (d.data_out) return 10;
  edge(d, 0, 0); edge(d, 0, 1);
  return d.data_out ? 0 : 11;
}
"#,
    )
    .unwrap();
    let exe = td.path().join("top_sim");
    compile_generated(&out_dir, &tb, &exe);
    let run = Command::new(&exe).output().expect("run top-level model");
    assert!(
        run.status.success(),
        "top-level synchronizer regression failed"
    );
}

#[test]
fn synchronous_reset_waits_for_an_edge() {
    if Command::new("clang++").arg("--version").output().is_err() {
        eprintln!("skipping: clang++ not available on PATH");
        return;
    }
    let td = tempfile::tempdir().expect("create temporary output directory");
    let source = td.path().join("reset.arch");
    let out_dir = td.path().join("build");
    std::fs::write(&source, RESET_SOURCE).unwrap();
    generate(&source, &out_dir);
    let tb = td.path().join("tb.cpp");
    std::fs::write(
        &tb,
        r#"#include "VFlagSync.h"
static void edge(VFlagSync& d, unsigned src, unsigned dst) {
  d.src_clk = src; d.dst_clk = dst; d.eval();
}
int main() {
  VFlagSync d; d.src_clk = 0; d.dst_clk = 0; d.rst = 1; d.data_in = 0; d.eval();
  d.rst = 0; d.data_in = 1;
  edge(d, 0, 1); edge(d, 0, 0); edge(d, 0, 1); edge(d, 0, 0);
  if (!d.data_out) return 10;
  d.rst = 1; d.eval();
  if (!d.data_out) return 11;
  edge(d, 0, 1);
  return d.data_out ? 12 : 0;
}
"#,
    )
    .unwrap();
    let exe = td.path().join("reset_sim");
    compile_generated(&out_dir, &tb, &exe);
    let run = Command::new(&exe).output().expect("run reset model");
    assert!(run.status.success(), "synchronous reset regression failed");
}
