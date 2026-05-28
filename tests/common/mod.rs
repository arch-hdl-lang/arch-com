//! Shared helpers for arch-com integration tests.
//!
//! Lives under `tests/common/` (not `tests/common.rs`) so cargo doesn't
//! treat it as an integration test target — a bare `tests/common.rs`
//! would be compiled and run as its own test binary with zero tests.
//! Modules under a subdirectory are only built when imported via
//! `mod common;` from a sibling integration test file.

/// Build an .arch source through `arch build`, verilate the resulting
/// SV with `--assert`, run the simulation, and assert that it aborts
/// with a non-zero exit *and* that `expected_substr` appears somewhere
/// in the run's combined stdout + stderr.
///
/// This is the inverse of the success-path pattern used by
/// `test_axi_dma_tlm_indexed_burst_target_verilator_behavior` and
/// friends in `tests/integration_test.rs`: same build / verilate
/// plumbing, but the final assertions are flipped to catch a fatal
/// instead of a PASS marker.
///
/// `expected_substr` should name the *specific* SVA label that is
/// expected to trip (e.g. `"BOUNDS VIOLATION: Probe._auto_bound_vec_0"`
/// or `"ASSERTION FAILED: ar_burst_supported"`) so that a rename
/// regresses the test loudly rather than silently passing on some
/// unrelated fatal.
///
/// If `verilator --version` fails we print a skip message and return,
/// matching the convention of the rest of the suite.
///
/// All inputs are paths relative to the crate root.
pub fn expect_verilator_fatal(
    arch_src_path: &str,
    tb_cpp_path: &str,
    top_module: &str,
    expected_substr: &str,
) {
    expect_verilator_fatal_multi(&[arch_src_path], tb_cpp_path, top_module, expected_substr);
}

/// Variant of [`expect_verilator_fatal`] that accepts multiple `.arch`
/// sources — required for designs whose top module references a bus or
/// helper construct defined in a sibling file (e.g. `BusAxi4.arch` for
/// `Nic400WidthAdapter.arch`). The first entry is treated as the top
/// design; the rest are dependencies passed to `arch build` on the same
/// command line so the type checker can resolve cross-file references
/// without relying on pre-existing `.archi` artifacts (which are
/// gitignored).
pub fn expect_verilator_fatal_multi(
    arch_src_paths: &[&str],
    tb_cpp_path: &str,
    top_module: &str,
    expected_substr: &str,
) {
    assert!(
        !arch_src_paths.is_empty(),
        "expect_verilator_fatal_multi requires at least one .arch source"
    );
    if std::process::Command::new("verilator")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!(
            "skipping expect_verilator_fatal({top_module}, {expected_substr:?}): \
             verilator not found"
        );
        return;
    }

    let td = tempfile::tempdir().expect("tempdir");
    let sv_out = td.path().join(format!("{top_module}.sv"));
    let obj_dir = td.path().join("obj_dir");
    let arch_bin = env!("CARGO_BIN_EXE_arch");

    let mut build_cmd = std::process::Command::new(arch_bin);
    build_cmd.arg("build");
    for src in arch_src_paths {
        build_cmd.arg(src);
    }
    build_cmd.arg("-o").arg(&sv_out);
    let build = build_cmd.output().expect("invoke arch build");
    assert!(
        build.status.success(),
        "arch build should succeed for {arch_src_paths:?}\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );

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
        .arg(top_module)
        .arg("-Mdir")
        .arg(&obj_dir)
        .arg(&sv_out)
        .arg(tb_cpp_path)
        .output()
        .expect("invoke verilator");
    assert!(
        verilate.status.success(),
        "verilator build should succeed for {top_module}\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verilate.stdout),
        String::from_utf8_lossy(&verilate.stderr)
    );

    let exe = obj_dir.join(format!("V{top_module}"));
    let run = std::process::Command::new(&exe)
        .output()
        .unwrap_or_else(|e| panic!("invoke V{top_module}: {e}"));

    // Verilator may route `$fatal` to either stdout or stderr depending
    // on version + build flags, so concatenate before searching.
    let stdout = String::from_utf8_lossy(&run.stdout);
    let stderr = String::from_utf8_lossy(&run.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        !run.status.success(),
        "V{top_module} was expected to ABORT (non-zero exit) but exited with success.\n\
         Looking for substring: {expected_substr:?}\n\
         stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        combined.contains(expected_substr),
        "V{top_module} aborted as expected, but the fatal message did not contain \
         {expected_substr:?}.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
