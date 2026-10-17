//! Regression coverage for arch#1031 — `arch sim` emitted non-compiling C++
//! for a `module` using an `enum`-typed signal, while `arch check` and
//! `arch build` were clean (a new instance of the #1020 class: `arch sim`
//! never type-checks its own emitted C++).
//!
//! Facet 1: an enum variant referenced by its bare short name (`mode <= Run`)
//! lowered to the bare identifier `Run`, but the emitted enum constants are
//! prefixed (`Mode_Run`), so the C++ referenced an undeclared symbol.
//!
//! Facet 2: a `let m: Mode = ...` declares a `_let_m` storage member of the
//! named enum type, but the header's VStructs.h include scan did not look at
//! `let` declarations, so the header referenced an undeclared type.
//!
//! Both are build failures (the sim binary fails to compile), so a value
//! assertion is belt-and-suspenders — the compile must succeed first.

use std::process::Command;

fn arch() -> Command {
    Command::new(env!("CARGO_BIN_EXE_arch"))
}

/// Facet 1: bare enum-variant reference in a reg/seq/comb module.
#[test]
fn sim_enum_bare_variant_reg() {
    let td = tempfile::tempdir().expect("tempdir");
    let out = arch()
        .arg("sim")
        .arg("tests/sim_enum_bare_variant.arch")
        .arg("--tb")
        .arg("tests/sim_enum_bare_variant_tb.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "arch sim should compile+pass for sim_enum_bare_variant\nstdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(
        stdout.contains("2 pass / 0 fail"),
        "expected both enum-state checks to pass; got:\n{stdout}"
    );
}

/// Facet 2: a `let` whose declared type is an enum pulls in VStructs.h.
#[test]
fn sim_enum_let_type() {
    let td = tempfile::tempdir().expect("tempdir");
    let out = arch()
        .arg("sim")
        .arg("tests/sim_enum_let_type.arch")
        .arg("--tb")
        .arg("tests/sim_enum_let_type_tb.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "arch sim should compile+pass for sim_enum_let_type\nstdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(
        stdout.contains("2 pass / 0 fail"),
        "expected both let-typed-enum checks to pass; got:\n{stdout}"
    );
}

/// `arch build` side of arch#1031: the SV enum typedef canonicalizes variant
/// names to uppercase, so a bare-variant reference must be emitted uppercased
/// too — otherwise the emitted SV references an undeclared member and fails to
/// elaborate (Verilator: "Can't find definition of variable 'Run'"). This is
/// a verilator-independent string check so the SV regression is gated per-PR,
/// not only by the nightly verilator_lint sweep.
#[test]
fn build_enum_bare_variant_sv_uppercases_references() {
    let td = tempfile::tempdir().expect("tempdir");
    let sv = td.path().join("E.sv");
    let out = arch()
        .arg("build")
        .arg("tests/sim_enum_bare_variant.arch")
        .arg("-o")
        .arg(&sv)
        .output()
        .expect("run arch build");
    assert!(
        out.status.success(),
        "arch build failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = std::fs::read_to_string(&sv).expect("read emitted SV");
    // References match the uppercased typedef members.
    assert!(
        text.contains("mode <= RUN;") && text.contains("mode <= IDLE;"),
        "expected uppercased bare-variant references in seq block; got:\n{text}"
    );
    assert!(
        text.contains("== RUN"),
        "expected uppercased bare-variant reference in comb comparison; got:\n{text}"
    );
    // The original-case bare names must NOT survive as member references
    // (they would be undeclared against the `IDLE`/`RUN` typedef). Use the
    // statement-terminated forms so the match can't collide with prose in a
    // source comment the compiler copies into the SV header.
    assert!(
        !text.contains("<= Run;") && !text.contains("<= Idle;"),
        "original-case bare-variant reference leaked into SV (case mismatch with typedef):\n{text}"
    );
}
