//! Regression coverage for B13: native simulation synchronizer models must
//! use the same `V<Name>` class/file naming contract as parent modules.

use std::process::Command;

const FIXTURE: &str = "tests/rdc/rdc_m5_cdc_distinct_sources_ok.arch";

fn arch() -> Command {
    Command::new(env!("CARGO_BIN_EXE_arch"))
}

#[test]
fn synchronizer_codegen_uses_v_prefix_everywhere() {
    let td = tempfile::tempdir().expect("create temporary output directory");
    let out = arch()
        .arg("sim")
        .arg(FIXTURE)
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim codegen");
    assert!(
        out.status.success(),
        "arch sim codegen should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(td.path().join("VFlagSync.h").is_file());
    assert!(td.path().join("VFlagSync.cpp").is_file());
    assert!(!td.path().join("FlagSync.h").exists());
    assert!(!td.path().join("FlagSync.cpp").exists());

    let header = std::fs::read_to_string(td.path().join("VFlagSync.h")).unwrap();
    let implementation = std::fs::read_to_string(td.path().join("VFlagSync.cpp")).unwrap();
    assert!(header.contains("class VFlagSync"));
    assert!(implementation.contains("#include \"VFlagSync.h\""));
    assert!(implementation.contains("void VFlagSync::eval()"));
}

#[test]
fn parent_synchronizer_model_compiles_and_runs_when_cxx_is_available() {
    if Command::new("clang++").arg("--version").output().is_err() {
        eprintln!("skipping: clang++ not available on PATH");
        return;
    }

    let td = tempfile::tempdir().expect("create temporary output directory");
    let tb = td.path().join("tb.cpp");
    std::fs::write(
        &tb,
        r#"#include "VM.h"
int main() {
  VM dut;
  dut.src_clk = 0;
  dut.dst_clk = 0;
  dut.flag_a = 0;
  dut.flag_b = 0;
  dut.eval();
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
        "parent synchronizer model should compile and run\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
