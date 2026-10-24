//! Regression coverage for native FIFO constant-expression parameters.

use std::path::Path;
use std::process::Command;

const FIFO_SOURCE: &str = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

fifo TxQueue
  param DEPTH: const = {DEPTH};
  param T: type = UInt<8>;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port push_valid: in Bool;
  port push_ready: out Bool;
  port push_data: in T;
  port pop_valid: out Bool;
  port pop_ready: in Bool;
  port pop_data: out T;
  port full: out Bool;
  port empty: out Bool;
end fifo TxQueue
"#;

const FIFO_TB: &str = r#"
#include "VTxQueue.h"
#include <cstdio>

int main() {
    VTxQueue d;
    auto tick = [&]() {
        d.clk = 0;
        d.eval();
        d.clk = 1;
        d.eval();
    };
    d.rst = 1;
    tick();
    d.rst = 0;
    d.pop_ready = 0;
    d.push_valid = 1;
    for (int i = 0; i < 4; ++i) {
        d.push_data = i;
        tick();
    }
    std::printf("full=%d ready=%d\n", d.full, d.push_ready);
    return d.full && !d.push_ready ? 0 : 1;
}
"#;

fn run_fifo(dir: &Path, depth: &str) -> std::process::Output {
    std::fs::write(dir.join("fifo.arch"), FIFO_SOURCE.replace("{DEPTH}", depth)).unwrap();
    std::fs::write(dir.join("fifo_tb.cpp"), FIFO_TB).unwrap();
    Command::new(env!("CARGO_BIN_EXE_arch"))
        .current_dir(dir)
        .args([
            "sim",
            "fifo.arch",
            "--tb",
            "fifo_tb.cpp",
            "--outdir",
            "build",
        ])
        .output()
        .expect("run native FIFO simulation")
}

#[test]
fn native_fifo_evaluates_arithmetic_and_param_reference_depths() {
    let arithmetic = tempfile::tempdir().unwrap();
    let out = run_fifo(arithmetic.path(), "2 + 2");
    assert!(
        out.status.success(),
        "arithmetic DEPTH FIFO should have capacity four\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("full=1 ready=0"));

    let literal = tempfile::tempdir().unwrap();
    let out = run_fifo(literal.path(), "4");
    assert!(
        out.status.success(),
        "literal DEPTH control should retain capacity four\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let referenced = tempfile::tempdir().unwrap();
    let source = FIFO_SOURCE
        .replace("{DEPTH}", "BASE + 2")
        .replace("param DEPTH", "param BASE: const = 2;\n  param DEPTH");
    std::fs::write(referenced.path().join("fifo.arch"), source).unwrap();
    std::fs::write(referenced.path().join("fifo_tb.cpp"), FIFO_TB).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_arch"))
        .current_dir(referenced.path())
        .args([
            "sim",
            "fifo.arch",
            "--tb",
            "fifo_tb.cpp",
            "--outdir",
            "build",
        ])
        .output()
        .expect("run parameter-reference FIFO simulation");
    assert!(
        out.status.success(),
        "parameter-reference DEPTH FIFO should have capacity four\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("full=1 ready=0"));
}
