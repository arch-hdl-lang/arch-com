//! Regression coverage for arch#1019 — `arch sim` emitted non-compiling C++
//! for every non-module construct with a `Vec<T,N>` port.
//!
//! `add_trace_to_simple_construct` (src/sim_codegen/trace.rs) built the VCD
//! trace signal list from the port list but, unlike the module-path collector
//! `collect_trace_signals`, did not skip `Vec<T,N>` ports. A Vec port is
//! decomposed into N scalars (`din_0..din_{N-1}`), so the trace dump emitted a
//! reference to the aggregate name `din`, which has no C++ declaration. The
//! host compiler then rejected the generated model with
//! `error: use of undeclared identifier 'din'`. The trace methods are emitted
//! unconditionally (no `--wave` needed), so this broke `arch sim` outright for
//! the affected constructs (fsm, ram, cam, counter, linklist, synchronizer,
//! regfile). The per-element scalars are still traced via `extra_signals`, so
//! the aggregate line was pure surplus and dropping it loses no waveform data.

use std::process::Command;

fn arch() -> Command {
    Command::new(env!("CARGO_BIN_EXE_arch"))
}

/// End-to-end: an fsm with a `Vec<UInt<64>,4>` input port must produce a model
/// that the host compiler accepts and runs.
#[test]
fn fsm_vec_port_sim_compiles_and_runs() {
    let td = tempfile::tempdir().expect("tempdir");
    let out = arch()
        .arg("sim")
        .arg("tests/sim_fsm_vec_port_trace_regression.arch")
        .arg("--tb")
        .arg("tests/sim_fsm_vec_port_trace_regression_tb.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "arch sim should build and run the Vec-port fsm model\nstdout:\n{stdout}\nstderr:\n{stderr}",
    );
    // The pre-fix failure mode was a raw host-compiler diagnostic pointing at
    // the undeclared aggregate identifier.
    assert!(
        !stderr.contains("undeclared identifier") && !stdout.contains("undeclared identifier"),
        "generated model referenced an undeclared Vec aggregate:\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

/// Codegen shape: the generated trace dump must reference the decomposed
/// per-element scalars (`din_0`), never the bare aggregate `din` that has no
/// C++ declaration.
#[test]
fn fsm_vec_port_trace_uses_decomposed_scalars() {
    let td = tempfile::tempdir().expect("tempdir");
    let out = arch()
        .arg("sim")
        .arg("tests/sim_fsm_vec_port_trace_regression.arch")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim codegen");
    assert!(
        out.status.success(),
        "arch sim codegen should succeed\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );
    let model = td.path().join("VFsmVecPortTrace.cpp");
    let cpp = std::fs::read_to_string(&model).expect("read model");
    // The decomposed element scalar is declared and must appear in the trace.
    assert!(
        cpp.contains("din_0"),
        "generated model should trace the decomposed Vec element din_0:\n{cpp}"
    );
    // The trace dump must not shift the bare aggregate name (`(din >> ...)`),
    // which has no declaration.
    assert!(
        !cpp.contains("(din >>"),
        "trace dump must not reference the undeclared Vec aggregate `din`:\n{cpp}"
    );
}
