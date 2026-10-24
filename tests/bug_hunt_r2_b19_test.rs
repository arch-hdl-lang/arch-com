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

fn arch() -> Command {
    Command::new(env!("CARGO_BIN_EXE_arch"))
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
  d.src_clk = src;
  d.dst_clk = dst;
  d.eval();
}
int main() {
  edge(0, 0);
  d.flag_a = 1;
  d.flag_b = 1;

  edge(1, 0);
  edge(0, 0);
  if (d.out_a || d.out_b) return 10; // source-only edges do not advance.

  edge(0, 1);
  edge(0, 0);
  if (d.out_a || d.out_b) return 11; // first destination edge enters stage 0.

  edge(0, 1);
  edge(0, 0);
  if (d.out_a != 1 || d.out_b != 1) return 12;

  d.flag_a = 0;
  d.flag_b = 0;
  edge(1, 0);
  edge(0, 0);
  if (d.out_a != 1 || d.out_b != 1) return 13;

  edge(0, 1);
  edge(0, 0);
  if (d.out_a != 1 || d.out_b != 1) return 14;

  edge(0, 1);
  edge(0, 0);
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
    let generated = arch()
        .arg("sim")
        .arg(&source)
        .arg("--outdir")
        .arg(&out_dir)
        .output()
        .expect("generate top-level synchronizer model");
    assert!(
        generated.status.success(),
        "top-level codegen should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&generated.stdout),
        String::from_utf8_lossy(&generated.stderr)
    );

    let tb = td.path().join("tb.cpp");
    std::fs::write(
        &tb,
        r#"#include "VFlagSync.h"
static void edge(VFlagSync& d, unsigned src, unsigned dst) {
  d.src_clk = src;
  d.dst_clk = dst;
  d.eval();
}
int main() {
  VFlagSync d;
  d.data_in = 0;
  edge(d, 0, 0);
  d.data_in = 1;
  edge(d, 1, 0);
  edge(d, 0, 0);
  edge(d, 0, 1);
  if (d.data_out) return 10;
  edge(d, 0, 0);
  edge(d, 0, 1);
  return d.data_out ? 0 : 11;
}
"#,
    )
    .unwrap();

    let mut cpp_files = std::fs::read_dir(&out_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "cpp"))
        .collect::<Vec<_>>();
    cpp_files.sort();
    let executable = td.path().join("top_sim");
    let compile = Command::new("clang++")
        .arg("-std=c++20")
        .arg("-O0")
        .arg("-I")
        .arg(&out_dir)
        .args(&cpp_files)
        .arg(&tb)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("compile top-level synchronizer model");
    assert!(
        compile.status.success(),
        "top-level model should compile\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&compile.stdout),
        String::from_utf8_lossy(&compile.stderr)
    );
    let run = Command::new(&executable)
        .output()
        .expect("run top-level synchronizer model");
    assert!(
        run.status.success(),
        "top-level synchronizer should propagate after two destination edges\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
}
