//! CLI-level coverage for `arch build -o auto` (issue #251): derive the
//! output `.sv` filename stem from each top-level construct's declared
//! name instead of the source filename.
//!
//! Naming-convention decision (documented in the PR description): the
//! construct's declared name is used **verbatim** — no CamelCase-to-
//! snake_case conversion is performed by the compiler. The issue's own
//! proposal relies on ARCH's naming convention already recommending
//! snake_case construct names (`module ibex_alu` inside `IbexAlu.arch`);
//! the compiler does not attempt to re-case a construct that doesn't
//! follow the convention. `axi_bridge_verbatim_name_used` below pins this
//! choice so it can't silently regress into "helpful" re-casing later.

use std::path::Path;
use std::process::Command;

/// Run `arch build <args>` with `dir` as the working directory (so
/// relative input paths in `args` resolve inside the tempdir, and any
/// `Wrote <path>` output lands there too).
fn run_build(dir: &Path, args: &[&str]) -> (bool, String, String) {
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let out = Command::new(arch_bin)
        .current_dir(dir)
        .arg("build")
        .args(args)
        .output()
        .expect("failed to run `arch build`");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

const MINIMAL_MODULE: &str = "
module {NAME}
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port a: in Bool;
end module {NAME}
";

fn minimal_module(name: &str) -> String {
    MINIMAL_MODULE.replace("{NAME}", name)
}

#[test]
fn already_snake_case_module_name_round_trips() {
    // The arch-ibex motivating case: CamelCase source filename, snake_case
    // declared module name. `-o auto` must derive the SV stem from the
    // module name, not the filename.
    let td = tempfile::tempdir().expect("tempdir");
    std::fs::write(td.path().join("IbexAlu.arch"), minimal_module("ibex_alu")).expect("write arch");

    let (ok, _out, err) = run_build(td.path(), &["-o", "auto", "IbexAlu.arch"]);
    assert!(ok, "arch build -o auto failed: {err}");

    assert!(
        td.path().join("ibex_alu.sv").is_file(),
        "expected ibex_alu.sv (derived from module name) to exist"
    );
    assert!(
        !td.path().join("IbexAlu.sv").exists(),
        "must not also write the filename-derived IbexAlu.sv"
    );
    let sv = std::fs::read_to_string(td.path().join("ibex_alu.sv")).expect("read sv");
    assert!(sv.contains("module ibex_alu"), "got:\n{sv}");
}

#[test]
fn axi_bridge_verbatim_name_used_no_case_conversion() {
    // Decision: `-o auto` does NOT re-case a construct name that doesn't
    // follow the snake_case convention. `module AXIBridge` writes exactly
    // `AXIBridge.sv`, not `axi_bridge.sv`.
    let td = tempfile::tempdir().expect("tempdir");
    std::fs::write(td.path().join("Weird.arch"), minimal_module("AXIBridge")).expect("write arch");

    let (ok, _out, err) = run_build(td.path(), &["-o", "auto", "Weird.arch"]);
    assert!(ok, "arch build -o auto failed: {err}");

    assert!(
        td.path().join("AXIBridge.sv").is_file(),
        "expected verbatim AXIBridge.sv to exist"
    );
    assert!(
        !td.path().join("axi_bridge.sv").exists(),
        "-o auto must not case-convert AXIBridge -> axi_bridge"
    );
}

#[test]
fn multi_module_file_splits_one_sv_per_module() {
    // Issue #251: "multi-module source files (one .arch declaring two
    // modules) get one .sv per module rather than one mashed .sv."
    let td = tempfile::tempdir().expect("tempdir");
    let source = format!("{}\n{}", minimal_module("A"), minimal_module("B"));
    std::fs::write(td.path().join("Multi.arch"), &source).expect("write arch");

    let (ok, _out, err) = run_build(td.path(), &["-o", "auto", "Multi.arch"]);
    assert!(ok, "arch build -o auto failed: {err}");

    assert!(td.path().join("A.sv").is_file(), "expected A.sv");
    assert!(td.path().join("B.sv").is_file(), "expected B.sv");
    assert!(
        !td.path().join("Multi.sv").exists(),
        "-o auto must not also write the combined Multi.sv"
    );

    let a_sv = std::fs::read_to_string(td.path().join("A.sv")).expect("read A.sv");
    assert!(a_sv.contains("module A"), "A.sv missing module A:\n{a_sv}");
    assert!(
        !a_sv.contains("module B"),
        "A.sv must not also contain module B:\n{a_sv}"
    );

    let b_sv = std::fs::read_to_string(td.path().join("B.sv")).expect("read B.sv");
    assert!(b_sv.contains("module B"), "B.sv missing module B:\n{b_sv}");
    assert!(
        !b_sv.contains("module A"),
        "B.sv must not also contain module A:\n{b_sv}"
    );
}

#[test]
fn default_naming_unaffected_without_o_auto() {
    // Regression guard: omitting -o (or passing an explicit path) must
    // keep today's "one combined .sv named after the source file" default
    // — -o auto is opt-in only.
    let td = tempfile::tempdir().expect("tempdir");
    let source = format!("{}\n{}", minimal_module("A"), minimal_module("B"));
    std::fs::write(td.path().join("Multi.arch"), &source).expect("write arch");

    let (ok, _out, err) = run_build(td.path(), &["Multi.arch"]);
    assert!(ok, "arch build failed: {err}");

    assert!(
        td.path().join("Multi.sv").is_file(),
        "default (no -o) must still write the combined Multi.sv"
    );
    assert!(!td.path().join("A.sv").exists());
    assert!(!td.path().join("B.sv").exists());
    let sv = std::fs::read_to_string(td.path().join("Multi.sv")).expect("read sv");
    assert!(
        sv.contains("module A") && sv.contains("module B"),
        "got:\n{sv}"
    );
}

#[test]
fn explicit_o_path_unaffected() {
    // Regression guard: an explicit literal -o path (not the string
    // "auto") must behave exactly as before — combined single output at
    // the given path.
    let td = tempfile::tempdir().expect("tempdir");
    std::fs::write(td.path().join("Multi.arch"), minimal_module("A")).expect("write arch");

    let (ok, _out, err) = run_build(td.path(), &["-o", "custom_name.sv", "Multi.arch"]);
    assert!(ok, "arch build failed: {err}");
    assert!(td.path().join("custom_name.sv").is_file());
    assert!(!td.path().join("A.sv").exists());
    assert!(!td.path().join("Multi.sv").exists());
}

#[test]
fn shared_struct_is_duplicated_into_each_per_module_output() {
    // Each -o auto output must be independently compilable: a struct
    // typedef referenced by more than one module is emitted into every
    // module's own .sv file (self-contained-file trade-off, documented in
    // the PR description).
    let td = tempfile::tempdir().expect("tempdir");
    let source = r#"
struct Packet
  data: UInt<32>;
  valid: Bool;
end struct Packet

module A
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port p: in Packet;
end module A

module B
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port p: in Packet;
end module B
"#;
    std::fs::write(td.path().join("Shared.arch"), source).expect("write arch");

    let (ok, _out, err) = run_build(td.path(), &["-o", "auto", "Shared.arch"]);
    assert!(ok, "arch build -o auto failed: {err}");

    let a_sv = std::fs::read_to_string(td.path().join("A.sv")).expect("read A.sv");
    let b_sv = std::fs::read_to_string(td.path().join("B.sv")).expect("read B.sv");
    assert!(
        a_sv.contains("Packet"),
        "A.sv must carry the Packet typedef standalone:\n{a_sv}"
    );
    assert!(
        b_sv.contains("Packet"),
        "B.sv must carry the Packet typedef standalone:\n{b_sv}"
    );
}

#[test]
fn incompatible_with_emit_thread_map() {
    // -o auto's per-construct split doesn't yet compose with the
    // combined-output-oriented --emit-thread-map/--emit-thread-proof/
    // --emit-construct-proof-* family; must fail with a clear message
    // rather than silently mis-emitting.
    let td = tempfile::tempdir().expect("tempdir");
    std::fs::write(td.path().join("A.arch"), minimal_module("A")).expect("write arch");

    let (ok, _out, err) = run_build(td.path(), &["-o", "auto", "--emit-thread-map", "A.arch"]);
    assert!(!ok, "expected -o auto + --emit-thread-map to fail");
    assert!(
        err.contains("-o auto") && err.contains("emit-thread-map"),
        "expected a clear -o-auto-incompatibility message, got:\n{err}"
    );
}

#[test]
fn errors_clearly_when_no_emittable_construct_present() {
    // A file with only a struct (no module/fsm/fifo/... construct) has
    // nothing for -o auto to name a .sv file after — error clearly rather
    // than silently writing zero files.
    let td = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        td.path().join("TypesOnly.arch"),
        "struct Packet\n  data: UInt<32>;\nend struct Packet\n",
    )
    .expect("write arch");

    let (ok, _out, err) = run_build(td.path(), &["-o", "auto", "TypesOnly.arch"]);
    assert!(!ok, "expected -o auto with no primary construct to fail");
    assert!(
        err.contains("-o auto"),
        "expected a clear -o-auto error, got:\n{err}"
    );
}
