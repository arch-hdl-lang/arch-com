//! Regression tests for arch#1058: non-power-of-two FIFO `DEPTH`.
//!
//! The pre-fix SystemVerilog `fifo` codegen assumed a power-of-two DEPTH
//! (pointers wrapping at `2^PTR_W`, memory indexed with the low `PTR_W-1`
//! bits). arch sim and the construct formal IR handle arbitrary depths, so
//! the emitted SV silently diverged: DEPTH=1 never asserted `full`, DEPTH=3
//! accepted four items and popped garbage.
//!
//! Each fixture ships one behavioral C++ testbench ([`fifo_cap_tb.h`]) that
//! fills until `!push_ready` (capacity must equal DEPTH exactly), drains in
//! FIFO order, then streams `>4*DEPTH` items with `<=DEPTH` in flight to
//! exercise the modular pointer wrap. Because the same DUT-agnostic TB runs
//! under BOTH arch sim and Verilator, a dual pass proves the two agree.
//!
//! Covered here: sync latency-0 (DEPTH 1 and 3) and latency-1 FWFT (DEPTH 3).
//! The async (dual-clock) FIFO is intentionally out of scope — see arch#1058.

use std::path::{Path, PathBuf};
use std::process::Command;

struct Case {
    top: &'static str,
    arch: &'static str,
    tb: &'static str,
}

const CASES: &[Case] = &[
    Case {
        top: "FifoCapD1",
        arch: "tests/fifo_nonpow2/FifoCapD1.arch",
        tb: "tests/fifo_nonpow2/tb_d1.cpp",
    },
    Case {
        top: "FifoCapD3",
        arch: "tests/fifo_nonpow2/FifoCapD3.arch",
        tb: "tests/fifo_nonpow2/tb_d3.cpp",
    },
    Case {
        top: "FifoCapD3L1",
        arch: "tests/fifo_nonpow2/FifoCapD3L1.arch",
        tb: "tests/fifo_nonpow2/tb_d3l1.cpp",
    },
];

fn manifest() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn verilator_available() -> bool {
    Command::new("verilator")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn build_sv(arch_rel: &str) -> String {
    let td = tempfile::tempdir().expect("tempdir");
    let sv_out = td.path().join("out.sv");
    let out = Command::new(env!("CARGO_BIN_EXE_arch"))
        .arg("build")
        .arg(manifest().join(arch_rel))
        .arg("-o")
        .arg(&sv_out)
        .output()
        .expect("invoke arch build");
    assert!(
        out.status.success(),
        "arch build failed for {arch_rel}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    std::fs::read_to_string(&sv_out).expect("read emitted SV")
}

/// Guards the byte-identity contract from the opposite side of
/// `scripts/refactor_diff.sh` (which runs nightly, not per-PR): a
/// power-of-two DEPTH must keep emitting the historical pointer body, while a
/// non-power-of-two DEPTH must use the general modular-pointer body. If these
/// ever converge, the pow2 corpus output has silently changed.
#[test]
fn test_fifo_pow2_keeps_historical_body_nonpow2_uses_general() {
    // Power-of-two DEPTH (=16): historical body, low-bits slice, no modular idx.
    let pow2 = build_sv("examples/sync_fifo.arch");
    assert!(
        pow2.contains("wr_ptr[PTR_W-2:0]"),
        "pow2 FIFO must keep the historical low-bits index:\n{pow2}"
    );
    assert!(
        !pow2.contains("wr_idx"),
        "pow2 FIFO must NOT use the general modular-pointer body:\n{pow2}"
    );

    // Non-power-of-two DEPTH (=3): general body, modular wrap, no low-bits slice.
    let general = build_sv("tests/fifo_nonpow2/FifoCapD3.arch");
    assert!(
        general.contains("wr_idx") && general.contains("PTR_LAST"),
        "non-pow2 FIFO must use the general modular-pointer body:\n{general}"
    );
    assert!(
        !general.contains("PTR_W-2:0"),
        "non-pow2 FIFO must not emit a degenerate low-bits slice:\n{general}"
    );
}

/// Native arch sim must pass the behavioral TB for every non-pow2 fixture.
#[test]
fn test_fifo_nonpow2_runs_in_native_sim() {
    let td = tempfile::tempdir().expect("tempdir");
    for c in CASES {
        let out = Command::new(env!("CARGO_BIN_EXE_arch"))
            .arg("sim")
            .arg(manifest().join(c.arch))
            .arg("--tb")
            .arg(manifest().join(c.tb))
            .arg("--outdir")
            .arg(td.path().join(c.top))
            .output()
            .expect("run native sim");
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            out.status.success() && stdout.contains("ALL TESTS PASSED"),
            "{} native sim failed\nstdout:\n{stdout}\nstderr:\n{stderr}",
            c.top
        );
    }
}

/// The `arch build` SystemVerilog, verilated and run against the same TB,
/// must agree with arch sim (both print `ALL TESTS PASSED`). This is the
/// end-to-end guard against the arch#1058 sim≠SV miscompile. Skipped when
/// Verilator is unavailable, matching the rest of the suite.
#[test]
fn test_fifo_nonpow2_sv_matches_sim_under_verilator() {
    if !verilator_available() {
        eprintln!("skipping test_fifo_nonpow2_sv_matches_sim_under_verilator: verilator not found");
        return;
    }
    let td = tempfile::tempdir().expect("tempdir");
    let arch_bin = env!("CARGO_BIN_EXE_arch");

    for c in CASES {
        let sv_out: PathBuf = td.path().join(format!("{}.sv", c.top));
        let obj_dir = td.path().join(format!("obj_{}", c.top));

        let build = Command::new(arch_bin)
            .arg("build")
            .arg(manifest().join(c.arch))
            .arg("-o")
            .arg(&sv_out)
            .output()
            .expect("invoke arch build");
        assert!(
            build.status.success(),
            "arch build failed for {}\nstdout:\n{}\nstderr:\n{}",
            c.top,
            String::from_utf8_lossy(&build.stdout),
            String::from_utf8_lossy(&build.stderr)
        );

        let verilate = Command::new("verilator")
            .arg("--cc")
            .arg("--exe")
            .arg("--build")
            .arg("--sv")
            .arg("--assert")
            .arg("-Wno-fatal")
            .arg("-Wno-WIDTH")
            .arg("-Wno-DECLFILENAME")
            .arg("--top-module")
            .arg(c.top)
            .arg("-Mdir")
            .arg(&obj_dir)
            .arg(&sv_out)
            .arg(manifest().join(c.tb))
            .output()
            .expect("invoke verilator");
        assert!(
            verilate.status.success(),
            "verilator build failed for {}\nstdout:\n{}\nstderr:\n{}",
            c.top,
            String::from_utf8_lossy(&verilate.stdout),
            String::from_utf8_lossy(&verilate.stderr)
        );

        let exe = obj_dir.join(format!("V{}", c.top));
        let run = Command::new(&exe)
            .output()
            .unwrap_or_else(|e| panic!("invoke V{}: {e}", c.top));
        let stdout = String::from_utf8_lossy(&run.stdout);
        let stderr = String::from_utf8_lossy(&run.stderr);
        assert!(
            run.status.success() && stdout.contains("ALL TESTS PASSED"),
            "V{} (Verilator) did not match arch sim\nstdout:\n{stdout}\nstderr:\n{stderr}",
            c.top
        );
    }
}
