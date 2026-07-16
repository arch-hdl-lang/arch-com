//! `fma<pipelined, 6>` codegen (proposal phase 3,
//! `doc/proposal_pipelined_operators.md`) — cross-backend verification.
//!
//! `src/pipelined_ops.rs`'s module doc comment argues sequential equivalence
//! to the comb `fma` operator holds *by construction*: the "staged IR" is
//! literally the comb operator feeding the ordinary `pipe_reg` register
//! cascade, not an independently hand-written pipeline that could diverge.
//! This file is the empirical half of that verification obligation:
//!
//! - [`pipelined_fma_latency_is_exactly_six_native_sim`]: the native-sim
//!   result appears at exactly cycle `t+6` for an input driven at cycle `t`,
//!   not `t+5` or `t+7`.
//! - [`pipelined_fma_lockstep_sim_vs_verilator`]: a >=1000-cycle randomized,
//!   back-to-back-throughput lock-step run — same stimulus fed to the
//!   native-sim backend and to Verilator on the emitted SV, every-cycle
//!   output compared bit-for-bit, including a mid-stream reset pulse.
//!   Skips cleanly when Verilator is not installed.

fn arch() -> std::process::Command {
    std::process::Command::new(env!("CARGO_BIN_EXE_arch"))
}

fn verilator_available() -> bool {
    std::process::Command::new("verilator")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

const MODULE_SRC: &str = r#"
module F32FmaPipe6Lockstep
  port clk: in Clock<Sys>;
  port rst: in Reset<Sync, High>;
  port a: in FP32;
  port b: in FP32;
  port c: in FP32;
  port y: out pipe_reg<FP32, 6> reset rst => 0.0;

  seq on clk rising
    y@6 <= fma<pipelined, 6>(a, b, c);
  end seq
end module F32FmaPipe6Lockstep
"#;

/// Deterministic stimulus generator shared verbatim (byte-for-byte, via
/// string interpolation of the same Rust `const`) between the native-sim and
/// Verilator testbenches below — the whole point of the lock-step check is
/// that both backends see *identical* inputs, so any output divergence is a
/// backend bug, not a stimulus mismatch. `hash` is the public-domain
/// murmur3-style integer finalizer (splitmix-adjacent bit-mixing, no
/// dependency on either backend's own RNG); operating on the full 32-bit
/// range means many cycles land on NaN/Inf/subnormal bit patterns, not just
/// "nice" floats — deliberately, since `fma` must be bit-exact there too.
const STIM_CPP: &str = r#"
static inline uint32_t lockstep_hash(uint32_t x) {
    x ^= x >> 16; x *= 0x7feb352du;
    x ^= x >> 15; x *= 0x846ca68bu;
    x ^= x >> 16;
    return x;
}
static inline void gen_stim(int cyc, uint32_t &a, uint32_t &b, uint32_t &c, int &rst) {
    a = lockstep_hash((uint32_t)cyc * 3u + 1u);
    b = lockstep_hash((uint32_t)cyc * 3u + 2u);
    c = lockstep_hash((uint32_t)cyc * 3u + 3u);
    // Mid-stream reset pulse (back-to-back inputs keep flowing before/after).
    rst = (cyc >= 700 && cyc < 703) ? 1 : 0;
}
"#;

const TOTAL_CYCLES: usize = 1200;

fn tb_native() -> String {
    format!(
        r#"
#include "VF32FmaPipe6Lockstep.h"
#include <cstdio>
#include <cstdint>
{STIM_CPP}
int main() {{
    VF32FmaPipe6Lockstep dut;
    dut.rst = 1; dut.a = 0; dut.b = 0; dut.c = 0;
    dut.clk = 0; dut.eval();
    dut.clk = 1; dut.eval();
    dut.rst = 0;
    for (int cyc = 0; cyc < {TOTAL_CYCLES}; cyc++) {{
        uint32_t a, b, c; int rst;
        gen_stim(cyc, a, b, c, rst);
        dut.a = a; dut.b = b; dut.c = c; dut.rst = (unsigned)rst;
        dut.clk = 0; dut.eval();
        dut.clk = 1; dut.eval();
        printf("%d %u %08x\n", cyc, (unsigned)dut.rst, (unsigned)dut.y);
    }}
    return 0;
}}
"#
    )
}

fn tb_verilator() -> String {
    format!(
        r#"
#include "VF32FmaPipe6Lockstep.h"
#include <cstdio>
#include <cstdint>
{STIM_CPP}
int main() {{
    VF32FmaPipe6Lockstep dut;
    dut.rst = 1; dut.a = 0; dut.b = 0; dut.c = 0;
    dut.clk = 0; dut.eval();
    dut.clk = 1; dut.eval();
    dut.rst = 0;
    for (int cyc = 0; cyc < {TOTAL_CYCLES}; cyc++) {{
        uint32_t a, b, c; int rst;
        gen_stim(cyc, a, b, c, rst);
        dut.a = a; dut.b = b; dut.c = c; dut.rst = (unsigned)rst;
        dut.clk = 0; dut.eval();
        dut.clk = 1; dut.eval();
        printf("%d %u %08x\n", cyc, (unsigned)dut.rst, (unsigned)dut.y);
    }}
    return 0;
}}
"#
    )
}

/// Latency-exactness: a single fma driven for one cycle then held constant
/// (comb inputs stay stable) must surface at the output register at exactly
/// cycle index 6 (0-indexed from the first clocked input), not 5 or 7.
#[test]
fn pipelined_fma_latency_is_exactly_six_native_sim() {
    let td = tempfile::tempdir().expect("tempdir");
    let arch_path = td.path().join("F32FmaPipe6Lockstep.arch");
    std::fs::write(&arch_path, MODULE_SRC).expect("write .arch");

    let tb_path = td.path().join("tb.cpp");
    std::fs::write(
        &tb_path,
        r#"
#include "VF32FmaPipe6Lockstep.h"
#include <cstdio>
union F32 { float f; unsigned u; };
static unsigned f32(float x) { F32 v; v.f = x; return v.u; }
static float f32_from(unsigned u) { F32 v; v.u = u; return v.f; }
int main() {
    VF32FmaPipe6Lockstep dut;
    dut.rst = 1; dut.a = 0; dut.b = 0; dut.c = 0;
    dut.clk = 0; dut.eval();
    dut.clk = 1; dut.eval();
    dut.rst = 0;
    dut.a = f32(2.0f);
    dut.b = f32(3.0f);
    dut.c = f32(1.0f);
    for (int cyc = 0; cyc < 10; cyc++) {
        dut.clk = 0; dut.eval();
        dut.clk = 1; dut.eval();
        printf("%d %.6f\n", cyc, f32_from(dut.y));
    }
    return 0;
}
"#,
    )
    .expect("write tb.cpp");

    let out = arch()
        .arg("sim")
        .arg(&arch_path)
        .arg("--tb")
        .arg(&tb_path)
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim");
    assert!(
        out.status.success(),
        "arch sim should build/run\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    // fma(2,3,1) = 7. Cycles 0..4 must NOT show 7.0 (result not arrived
    // yet); cycle 5 (the 6th posedge after the input was driven, 0-indexed)
    // must show 7.0 — and hold from there on (comb inputs are stable).
    let lines: Vec<&str> = stdout
        .lines()
        .filter(|l| l.starts_with(char::is_numeric))
        .collect();
    assert!(lines.len() >= 10, "expected 10 cycle lines, got:\n{stdout}");
    for (cyc, line) in lines.iter().enumerate().take(10) {
        let val: f32 = line
            .split_whitespace()
            .nth(1)
            .expect("cycle value field")
            .parse()
            .expect("parse cycle value");
        if cyc < 5 {
            assert!(
                (val - 7.0).abs() > 1e-6,
                "cycle {cyc} should NOT yet show fma result 7.0 (latency-6 not \
                 reached), got {val} — full output:\n{stdout}"
            );
        } else {
            assert!(
                (val - 7.0).abs() < 1e-6,
                "cycle {cyc} should show fma(2,3,1)=7.0 (latency-6 reached), \
                 got {val} — full output:\n{stdout}"
            );
        }
    }
}

/// The main lock-step check: >=1000 cycles, randomized full-32-bit operand
/// coverage, back-to-back inputs every cycle (throughput=1, no bubbles), and
/// a mid-stream reset pulse (cycles 700..703) — native sim and Verilator
/// (on the same `arch build` SV) must agree bit-for-bit on `y` and on `rst`
/// echo, every single cycle.
#[test]
fn pipelined_fma_lockstep_sim_vs_verilator() {
    if !verilator_available() {
        eprintln!("skipping pipelined_fma_lockstep_sim_vs_verilator: verilator not in PATH");
        return;
    }

    let td = tempfile::tempdir().expect("tempdir");
    let arch_path = td.path().join("F32FmaPipe6Lockstep.arch");
    std::fs::write(&arch_path, MODULE_SRC).expect("write .arch");

    // ── Native sim run ──────────────────────────────────────────────────
    let native_tb = td.path().join("tb_native.cpp");
    std::fs::write(&native_tb, tb_native()).expect("write native tb");
    let native_outdir = td.path().join("native_out");
    std::fs::create_dir_all(&native_outdir).unwrap();
    let sim_out = arch()
        .arg("sim")
        .arg(&arch_path)
        .arg("--tb")
        .arg(&native_tb)
        .arg("--outdir")
        .arg(&native_outdir)
        .output()
        .expect("run arch sim");
    assert!(
        sim_out.status.success(),
        "arch sim should build/run\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&sim_out.stdout),
        String::from_utf8_lossy(&sim_out.stderr)
    );
    let native_stdout = String::from_utf8_lossy(&sim_out.stdout).to_string();

    // ── SV build + Verilator run ────────────────────────────────────────
    let sv_path = td.path().join("F32FmaPipe6Lockstep.sv");
    let build_out = arch()
        .arg("build")
        .arg(&arch_path)
        .arg("-o")
        .arg(&sv_path)
        .output()
        .expect("run arch build");
    assert!(
        build_out.status.success(),
        "arch build should succeed (phase 3: fma<pipelined, 6> now binds to \
         comb+cascade codegen)\nstderr:\n{}",
        String::from_utf8_lossy(&build_out.stderr)
    );

    let verilator_tb = td.path().join("tb_verilator.cpp");
    std::fs::write(&verilator_tb, tb_verilator()).expect("write verilator tb");
    let obj_dir = td.path().join("obj_dir");
    let verilate = std::process::Command::new("verilator")
        .arg("--cc")
        .arg("--exe")
        .arg("--build")
        .arg("--sv")
        .arg("--assert")
        .arg("-Wno-fatal")
        .arg("-Wno-WIDTH")
        .arg("-Wno-DECLFILENAME")
        .arg("--top-module")
        .arg("F32FmaPipe6Lockstep")
        .arg("-Mdir")
        .arg(&obj_dir)
        .arg(&sv_path)
        .arg(&verilator_tb)
        .output()
        .expect("verilate");
    assert!(
        verilate.status.success(),
        "Verilator build should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verilate.stdout),
        String::from_utf8_lossy(&verilate.stderr)
    );
    let exe = obj_dir.join("VF32FmaPipe6Lockstep");
    let run = std::process::Command::new(&exe)
        .output()
        .expect("run verilated sim");
    assert!(
        run.status.success(),
        "Verilator sim should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    let sv_stdout = String::from_utf8_lossy(&run.stdout).to_string();

    // ── Lock-step comparison ────────────────────────────────────────────
    let native_lines: Vec<&str> = native_stdout.lines().collect();
    let sv_lines: Vec<&str> = sv_stdout.lines().collect();
    assert_eq!(
        native_lines.len(),
        TOTAL_CYCLES,
        "native sim should print exactly {TOTAL_CYCLES} lines, got {}:\n{native_stdout}",
        native_lines.len()
    );
    assert_eq!(
        sv_lines.len(),
        TOTAL_CYCLES,
        "Verilator should print exactly {TOTAL_CYCLES} lines, got {}:\n{sv_stdout}",
        sv_lines.len()
    );
    let mut mismatches = Vec::new();
    for cyc in 0..TOTAL_CYCLES {
        if native_lines[cyc] != sv_lines[cyc] {
            mismatches.push(format!(
                "cycle {cyc}: native=`{}` sv=`{}`",
                native_lines[cyc], sv_lines[cyc]
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "sim/SV lock-step mismatch(es) — sequential-equivalence-by-construction \
         violated ({} of {TOTAL_CYCLES} cycles differ):\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );

    // Sanity: the reset pulse must actually have been exercised by both
    // backends (not a vacuously-passing empty-diff from a stimulus bug) —
    // `y` must return to the reset value (0.0 = 0x00000000) at some cycle
    // shortly after the 700..703 reset pulse, then diverge from it again
    // once fresh post-reset results arrive 6 cycles later.
    let post_reset_zero = native_lines
        .iter()
        .skip(703)
        .take(6)
        .any(|l| l.ends_with(" 00000000"));
    assert!(
        post_reset_zero,
        "expected a reset-value (0x00000000) cycle shortly after the mid-stream \
         reset pulse in native sim output — reset pulse may not have been \
         exercised:\n{native_stdout}"
    );
}

// ─── phase 3.5: staged emission (`arch build --staged-ops`) ─────────────────
//
// The staged datapath is a GENUINE second implementation (hand-scheduled
// per-stage logic + live-set register layers, not the comb cone + delay
// line), so unlike the cascade form its equivalence is NOT true by
// construction — the lock-step test below is the discharge of that
// obligation (proposal §4 / phase 3.5): Verilator on the staged SV vs the
// native sim (always cascade), bit-for-bit every cycle, including the
// mid-stream reset pulse and its N-1-cycle warm-up window (reproduced in
// the staged form by the 1-bit validity chain that replaces the cascade).

/// Builds the fixture with `--staged-ops` and returns the SV path. Asserts
/// the emitted SV has the staged shape (staged module + instance + validity
/// chain) and did NOT silently fall back to the cascade.
fn build_staged(td: &tempfile::TempDir, arch_path: &std::path::Path) -> std::path::PathBuf {
    let sv_path = td.path().join("staged.sv");
    let build_out = arch()
        .arg("build")
        .arg("--staged-ops")
        .arg(arch_path)
        .arg("-o")
        .arg(&sv_path)
        .output()
        .expect("run arch build --staged-ops");
    assert!(
        build_out.status.success(),
        "arch build --staged-ops should succeed\nstderr:\n{}",
        String::from_utf8_lossy(&build_out.stderr)
    );
    let stderr = String::from_utf8_lossy(&build_out.stderr);
    assert!(
        !stderr.contains("falls back to the cascade emission"),
        "the builtin fma<pipelined, 6> site must emit staged, not fall back:\n{stderr}"
    );
    let sv = std::fs::read_to_string(&sv_path).expect("read staged SV");
    assert!(
        sv.contains("module ArchF32FmaStaged6 ("),
        "staged module definition missing"
    );
    assert!(
        sv.contains("ArchF32FmaStaged6 __staged_fma_0 (.clk(clk), .a(a), .b(b), .c(c), .y(__staged_fma_0_y));"),
        "staged instance missing"
    );
    for k in 1..=5 {
        assert!(
            sv.contains(&format!("// ── register layer {k} (")),
            "staged register layer {k} missing"
        );
    }
    // The parent's cascade became a 1-bit validity chain gating the result.
    assert!(
        sv.contains("y_stg1 <= 1'b1;"),
        "validity chain head missing"
    );
    assert!(
        sv.contains("y <= y_stg5 ? __staged_fma_0_y : 32'h00000000;"),
        "warm-up gate missing"
    );
    // The parent module must NOT compute the result through the comb
    // function call anymore (that's the cascade form).
    assert!(
        !sv.contains("y_stg1 <= arch_fma_f32("),
        "comb call survived --staged-ops"
    );
    sv_path
}

/// Cheap shape check + default-mode regression: `--staged-ops` emits the
/// staged structure; the SAME fixture without the flag emits the cascade,
/// with no staged artifacts. Runs without Verilator.
#[test]
fn staged_ops_emits_staged_shape_and_default_stays_cascade() {
    let td = tempfile::tempdir().expect("tempdir");
    let arch_path = td.path().join("F32FmaPipe6Lockstep.arch");
    std::fs::write(&arch_path, MODULE_SRC).expect("write .arch");
    build_staged(&td, &arch_path);

    let sv_path = td.path().join("cascade.sv");
    let build_out = arch()
        .arg("build")
        .arg(&arch_path)
        .arg("-o")
        .arg(&sv_path)
        .output()
        .expect("run arch build");
    assert!(build_out.status.success());
    let sv = std::fs::read_to_string(&sv_path).expect("read cascade SV");
    assert!(
        sv.contains("y_stg1 <= arch_fma_f32(a, b, c);"),
        "default build must keep the comb+cascade form"
    );
    assert!(
        !sv.contains("ArchF32FmaStaged6") && !sv.contains("__staged"),
        "default build must contain no staged artifacts"
    );
}

/// A conditional (nested) pipelined call is outside the staged v1 shape:
/// `--staged-ops` must warn and fall back to the cascade — never error,
/// never emit a half-rewritten site.
#[test]
fn staged_ops_conditional_call_falls_back_with_warning() {
    let src = r#"
module F32FmaPipe6Cond
  port clk: in Clock<Sys>;
  port rst: in Reset<Sync, High>;
  port en: in Bool;
  port a: in FP32;
  port b: in FP32;
  port c: in FP32;
  port y: out pipe_reg<FP32, 6> reset rst => 0.0;

  seq on clk rising
    if en
      y@6 <= fma<pipelined, 6>(a, b, c);
    end if
  end seq
end module F32FmaPipe6Cond
"#;
    let td = tempfile::tempdir().expect("tempdir");
    let arch_path = td.path().join("F32FmaPipe6Cond.arch");
    std::fs::write(&arch_path, src).expect("write .arch");
    let sv_path = td.path().join("cond.sv");
    let build_out = arch()
        .arg("build")
        .arg("--staged-ops")
        .arg(&arch_path)
        .arg("-o")
        .arg(&sv_path)
        .output()
        .expect("run arch build --staged-ops");
    assert!(
        build_out.status.success(),
        "conditional site must fall back, not error\nstderr:\n{}",
        String::from_utf8_lossy(&build_out.stderr)
    );
    let stderr = String::from_utf8_lossy(&build_out.stderr);
    assert!(
        stderr.contains("falls back to the cascade emission"),
        "expected a fallback warning for the conditional site:\n{stderr}"
    );
    let sv = std::fs::read_to_string(&sv_path).expect("read SV");
    assert!(
        sv.contains("arch_fma_f32(") && !sv.contains("ArchF32FmaStaged6"),
        "conditional site must emit the cascade form"
    );
}

/// Latency-exactness on the STAGED build: fma(2,3,1)=7.0 must first appear
/// at exactly the 6th posedge (cycle index 5), same as the cascade —
/// checked via Verilator since the native sim always runs the cascade form.
#[test]
fn staged_ops_latency_is_exactly_six_verilator() {
    if !verilator_available() {
        eprintln!("skipping staged_ops_latency_is_exactly_six_verilator: verilator not in PATH");
        return;
    }
    let td = tempfile::tempdir().expect("tempdir");
    let arch_path = td.path().join("F32FmaPipe6Lockstep.arch");
    std::fs::write(&arch_path, MODULE_SRC).expect("write .arch");
    let sv_path = build_staged(&td, &arch_path);

    let tb = td.path().join("tb_lat.cpp");
    std::fs::write(
        &tb,
        r#"
#include "VF32FmaPipe6Lockstep.h"
#include <cstdio>
#include <cstdint>
int main() {
    VF32FmaPipe6Lockstep dut;
    dut.rst = 1; dut.a = 0; dut.b = 0; dut.c = 0;
    dut.clk = 0; dut.eval(); dut.clk = 1; dut.eval();
    dut.rst = 0;
    dut.a = 0x40000000u; dut.b = 0x40400000u; dut.c = 0x3F800000u; // 2*3+1=7
    int first = -1;
    for (int cyc = 0; cyc < 10; cyc++) {
        dut.clk = 0; dut.eval(); dut.clk = 1; dut.eval();
        if (first < 0 && dut.y == 0x40E00000u) first = cyc;
    }
    printf("first=%d\n", first);
    return first == 5 ? 0 : 1;
}
"#,
    )
    .expect("write tb");
    let obj_dir = td.path().join("obj_lat");
    let verilate = std::process::Command::new("verilator")
        .args([
            "--cc",
            "--exe",
            "--build",
            "--sv",
            "-Wno-fatal",
            "-Wno-WIDTH",
        ])
        .arg("--top-module")
        .arg("F32FmaPipe6Lockstep")
        .arg("-Mdir")
        .arg(&obj_dir)
        .arg(&sv_path)
        .arg(&tb)
        .output()
        .expect("verilate");
    assert!(
        verilate.status.success(),
        "Verilator build should pass\nstderr:\n{}",
        String::from_utf8_lossy(&verilate.stderr)
    );
    let run = std::process::Command::new(obj_dir.join("VF32FmaPipe6Lockstep"))
        .output()
        .expect("run");
    assert!(
        run.status.success(),
        "staged latency must be exactly 6 edges — {}",
        String::from_utf8_lossy(&run.stdout)
    );
}

/// **The phase-3.5 equivalence obligation.** Same >=1000-cycle randomized
/// stimulus (mid-stream reset pulse included) as the cascade lock-step:
/// Verilator on the STAGED SV vs the native sim (cascade form) must agree
/// bit-for-bit on every cycle — including the reset warm-up window, which
/// the staged form reproduces via its validity chain.
#[test]
fn staged_ops_lockstep_sim_vs_verilator() {
    if !verilator_available() {
        eprintln!("skipping staged_ops_lockstep_sim_vs_verilator: verilator not in PATH");
        return;
    }
    let td = tempfile::tempdir().expect("tempdir");
    let arch_path = td.path().join("F32FmaPipe6Lockstep.arch");
    std::fs::write(&arch_path, MODULE_SRC).expect("write .arch");

    // Native sim (cascade form — the trusted reference).
    let native_tb = td.path().join("tb_native.cpp");
    std::fs::write(&native_tb, tb_native()).expect("write native tb");
    let native_outdir = td.path().join("native_out");
    std::fs::create_dir_all(&native_outdir).unwrap();
    let sim_out = arch()
        .arg("sim")
        .arg(&arch_path)
        .arg("--tb")
        .arg(&native_tb)
        .arg("--outdir")
        .arg(&native_outdir)
        .output()
        .expect("run arch sim");
    assert!(
        sim_out.status.success(),
        "arch sim should build/run\nstderr:\n{}",
        String::from_utf8_lossy(&sim_out.stderr)
    );
    let native_stdout = String::from_utf8_lossy(&sim_out.stdout).to_string();

    // Staged SV + Verilator.
    let sv_path = build_staged(&td, &arch_path);
    let verilator_tb = td.path().join("tb_verilator.cpp");
    std::fs::write(&verilator_tb, tb_verilator()).expect("write verilator tb");
    let obj_dir = td.path().join("obj_staged");
    let verilate = std::process::Command::new("verilator")
        .args([
            "--cc",
            "--exe",
            "--build",
            "--sv",
            "--assert",
            "-Wno-fatal",
            "-Wno-WIDTH",
            "-Wno-DECLFILENAME",
        ])
        .arg("--top-module")
        .arg("F32FmaPipe6Lockstep")
        .arg("-Mdir")
        .arg(&obj_dir)
        .arg(&sv_path)
        .arg(&verilator_tb)
        .output()
        .expect("verilate");
    assert!(
        verilate.status.success(),
        "Verilator build should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verilate.stdout),
        String::from_utf8_lossy(&verilate.stderr)
    );
    let run = std::process::Command::new(obj_dir.join("VF32FmaPipe6Lockstep"))
        .output()
        .expect("run verilated sim");
    assert!(run.status.success());
    let sv_stdout = String::from_utf8_lossy(&run.stdout).to_string();

    let native_lines: Vec<&str> = native_stdout.lines().collect();
    let sv_lines: Vec<&str> = sv_stdout.lines().collect();
    assert_eq!(native_lines.len(), TOTAL_CYCLES);
    assert_eq!(sv_lines.len(), TOTAL_CYCLES);
    let mut mismatches = Vec::new();
    for cyc in 0..TOTAL_CYCLES {
        if native_lines[cyc] != sv_lines[cyc] {
            mismatches.push(format!(
                "cycle {cyc}: native(cascade)=`{}` staged=`{}`",
                native_lines[cyc], sv_lines[cyc]
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "STAGED-vs-cascade lock-step mismatch(es) — the staged datapath is a \
         real second implementation and this is its equivalence obligation \
         ({} of {TOTAL_CYCLES} cycles differ):\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
    // Reset pulse must have been exercised (not a vacuous pass).
    let post_reset_zero = native_lines
        .iter()
        .skip(703)
        .take(6)
        .any(|l| l.ends_with(" 00000000"));
    assert!(
        post_reset_zero,
        "reset pulse not exercised:\n{native_stdout}"
    );
}
