//! ARCH → SystemVerilog codegen certificates (`--emit-codegen-proof-lean`).
//!
//! The Lean replay tests skip when `lake` is unavailable. The negative
//! controls are the important half: a validator that accepts every input
//! would make every positive test pass, so each fixture is also replayed
//! against hand-miscompiled SV that must be rejected, and against
//! semantics-preserving rewrites that must still be accepted.

use std::path::{Path, PathBuf};
use std::process::Command;

const LEAN_PROJECT_DIR: &str = "proofs/lean_thread_lowering";
const FIXTURES: &[&str] = &[
    "accumulator",
    "operators",
    "comb_datapath",
    "counter_fsm",
    "param_datapath",
    "async_counter",
    "let_port_reg",
    "init_reg",
    "param_select_async",
];

fn find_lake() -> Option<PathBuf> {
    if Command::new("lake")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return Some(PathBuf::from("lake"));
    }
    let home = std::env::var_os("HOME")?;
    let home_lake = PathBuf::from(home).join(".elan/bin/lake");
    home_lake.exists().then_some(home_lake)
}

/// Build the Lean library once per test binary; `false` means skip.
fn lean_ready() -> Option<PathBuf> {
    static READY: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    READY
        .get_or_init(|| {
            let Some(lake) = find_lake() else {
                eprintln!("skipping Lean replay tests: `lake` not found");
                return None;
            };
            let out = Command::new(&lake)
                .arg("build")
                .arg("ArchCodegenProof")
                .current_dir(LEAN_PROJECT_DIR)
                .output()
                .expect("spawn lake build");
            assert!(
                out.status.success(),
                "lake build ArchCodegenProof failed:\n{}\n{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            Some(lake)
        })
        .clone()
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(format!("tests/codegen_proof/{name}.arch"))
}

fn build_sv(name: &str, dir: &Path) -> String {
    let sv_path = dir.join(format!("{name}.sv"));
    let out = Command::new(env!("CARGO_BIN_EXE_arch"))
        .env("ARCH_NO_LEARN", "1")
        .arg("build")
        .arg(fixture(name))
        .arg("-o")
        .arg(&sv_path)
        .output()
        .expect("spawn arch");
    assert!(
        out.status.success(),
        "arch build {name} failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    std::fs::read_to_string(sv_path).expect("read sv")
}

fn parse_items(name: &str) -> Vec<arch::ast::Item> {
    let source = std::fs::read_to_string(fixture(name)).expect("read fixture");
    let tokens = arch::lexer::tokenize(&source).expect("lex fixture");
    let mut parser = arch::parser::Parser::new(tokens, &source);
    parser.parse_source_file().expect("parse fixture").items
}

/// Replay a certificate file; returns (accepted, diagnostics).
fn replay(lake: &Path, lean: &str, dir: &Path, tag: &str) -> (bool, String) {
    let path = dir.join(format!("{tag}.lean"));
    std::fs::write(&path, lean).expect("write cert");
    let out = Command::new(lake)
        .arg("env")
        .arg("lean")
        .arg(path.canonicalize().expect("canonical cert path"))
        .current_dir(LEAN_PROJECT_DIR)
        .output()
        .expect("spawn lake env lean");
    let diag = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), diag)
}

fn cert_for(name: &str, sv: &str) -> String {
    let items = parse_items(name);
    let cert = arch::codegen_proof_cert::render_lean_items(items.iter(), sv)
        .unwrap_or_else(|e| panic!("certificate generation failed for {name}: {e}"));
    assert!(
        cert.skipped.is_empty(),
        "{name}: skipped {:?}",
        cert.skipped
    );
    cert.lean
}

fn mutate(sv: &str, from: &str, to: &str) -> String {
    assert_eq!(
        sv.matches(from).count(),
        1,
        "mutation anchor `{from}` must occur exactly once in:\n{sv}"
    );
    sv.replacen(from, to, 1)
}

#[test]
fn fixtures_certify_through_the_cli() {
    if lean_ready().is_none() {
        return;
    }
    let td = tempfile::tempdir().expect("tempdir");
    for name in FIXTURES {
        let sv_path = td.path().join(format!("{name}.sv"));
        let proof_path = td.path().join(format!("{name}.codegen-proof.lean"));
        let out = Command::new(env!("CARGO_BIN_EXE_arch"))
            .env("ARCH_NO_LEARN", "1")
            .arg("build")
            .arg(fixture(name))
            .arg("-o")
            .arg(&sv_path)
            .arg(format!(
                "--emit-codegen-proof-lean={}",
                proof_path.display()
            ))
            .arg("--check-codegen-proof-lean")
            .arg(format!("--codegen-proof-lean-project={LEAN_PROJECT_DIR}"))
            .output()
            .expect("spawn arch");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            out.status.success(),
            "{name}: certificate replay failed:\n{stderr}"
        );
        assert!(
            stderr.contains("Lean codegen proof replay OK"),
            "{name}:\n{stderr}"
        );
        let lean = std::fs::read_to_string(&proof_path).expect("read cert");
        assert!(lean.contains("theorem codegen_correct"), "{name}:\n{lean}");
        assert!(
            !lean.contains("sorry"),
            "{name}: certificate must not contain sorry"
        );
    }
}

/// The end-to-end theorem must rest on Lean's standard axioms only: no
/// `sorryAx` anywhere in the library, and no `Lean.ofReduceBool` (which
/// would mean trusting compiled code instead of the kernel).
#[test]
fn certificate_theorem_uses_only_standard_axioms() {
    let Some(lake) = lean_ready() else { return };
    let td = tempfile::tempdir().expect("tempdir");
    let sv = build_sv("counter_fsm", td.path());
    let mut lean = cert_for("counter_fsm", &sv);
    lean.push_str("\n#print axioms ArchCodegenCert.«CounterFsm».codegen_correct\n");
    let (accepted, diag) = replay(&lake, &lean, td.path(), "axioms");
    assert!(accepted, "{diag}");
    assert!(diag.contains("depends on axioms"), "{diag}");
    for bad in ["sorryAx", "ofReduceBool"] {
        assert!(!diag.contains(bad), "unexpected axiom `{bad}`:\n{diag}");
    }
}

/// Each mutation is a plausible codegen bug; the validator must refuse it.
#[test]
fn miscompiled_sv_is_rejected() {
    let Some(lake) = lean_ready() else { return };
    let td = tempfile::tempdir().expect("tempdir");
    let cases: &[(&str, &str, &str, &str)] = &[
        ("accumulator", "dropped carry", "9'(a + b)", "8'(a + b)"),
        ("accumulator", "inverted reset", "if (rst)", "if (!rst)"),
        ("accumulator", "wrong reset value", "acc <= 0;", "acc <= 1;"),
        ("accumulator", "wrong operator", "a ^ b", "a | b"),
        (
            "accumulator",
            "narrowed port",
            "output logic [8:0] sum",
            "output logic [7:0] sum",
        ),
        (
            "accumulator",
            "wrong flop clock",
            "posedge clk",
            "posedge en",
        ),
        ("operators", "wrong literal", "w = 'h5A;", "w = 'h5B;"),
        (
            "operators",
            "truncated difference",
            "o1 = 9'(a - b);",
            "o1 = 8'(a - b);",
        ),
        ("operators", "wrong slice", "a[7:4]", "a[6:3]"),
        (
            "counter_fsm",
            "lost else-if",
            "end else if (count == limit) begin",
            "end else if (count != limit) begin",
        ),
        (
            "comb_datapath",
            "narrowed product",
            "16'(a * b)",
            "8'(a * b)",
        ),
        (
            "param_datapath",
            "wrong parameter default",
            "parameter int W = 8,",
            "parameter int W = 9,",
        ),
        (
            "param_datapath",
            "wrong reset parameter",
            "parameter int INIT = 3,",
            "parameter int INIT = 2,",
        ),
        (
            "param_datapath",
            "shifted part-select",
            "a[W - 4 +: 4]",
            "a[W - 5 +: 4]",
        ),
        (
            "async_counter",
            "async reset made synchronous",
            "posedge clk or negedge rst_n",
            "posedge clk",
        ),
        (
            "async_counter",
            "async reset edge inverted",
            "or negedge rst_n",
            "or posedge rst_n",
        ),
        (
            "async_counter",
            "async reset level inverted",
            "if ((!rst_n))",
            "if (rst_n)",
        ),
        (
            "accumulator",
            "sync reset made asynchronous",
            "@(posedge clk)",
            "@(posedge clk or posedge rst)",
        ),
        (
            "let_port_reg",
            "wrong constant in a let",
            "(s + 5);",
            "(s + 6);",
        ),
        (
            "let_port_reg",
            "port reg reset dropped",
            "      flag <= 1'b0;\n",
            "",
        ),
        (
            "init_reg",
            "wrong initial value",
            "logic [7:0] r = 7;",
            "logic [7:0] r = 6;",
        ),
        (
            "init_reg",
            "dropped initializer",
            "logic [7:0] r = 7;",
            "logic [7:0] r;",
        ),
        (
            "init_reg",
            "initializer on a combinational net",
            "logic [7:0] w;",
            "logic [7:0] w = 3;",
        ),
        (
            "param_select_async",
            "inverted parameter comparison",
            "if (SEL == 0)",
            "if (SEL != 0)",
        ),
        (
            "param_select_async",
            "wrong comparison constant",
            "if (SEL == 0)",
            "if (SEL == 1)",
        ),
        (
            "param_select_async",
            "wrong async reset constant",
            "cnt <= 12'($unsigned(START));\n      q <= 0;",
            "cnt <= 12'($unsigned(START + 1));\n      q <= 0;",
        ),
        (
            "param_select_async",
            "async reset branch reads a signal",
            "      q <= 0;",
            "      q <= a;",
        ),
    ];
    let mut failures = Vec::new();
    for (i, (name, what, from, to)) in cases.iter().enumerate() {
        let sv = build_sv(name, td.path());
        let lean = cert_for(name, &mutate(&sv, from, to));
        let (accepted, diag) = replay(&lake, &lean, td.path(), &format!("bad{i}"));
        if accepted {
            failures.push(format!("{name}: `{what}` was ACCEPTED"));
        } else if !diag.contains("is false") {
            failures.push(format!(
                "{name}: `{what}` failed for an unexpected reason:\n{diag}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Rewrites that keep the meaning must still be accepted: the validator
/// compares semantics, not text.
#[test]
fn semantics_preserving_rewrites_are_accepted() {
    let Some(lake) = lean_ready() else { return };
    let td = tempfile::tempdir().expect("tempdir");
    let cases: &[(&str, &str, &str, &str)] = &[
        // The assignment context is already 9 bits wide.
        (
            "operators",
            "redundant cast removed",
            "o1 = 9'(a - b);",
            "o1 = a - b;",
        ),
        // A wider cast truncated by an 8-bit target is the same value.
        (
            "accumulator",
            "different cast width",
            "(8 > $bits(t) ? 8 : $bits(t))'(acc + t)",
            "16'(acc + t)",
        ),
        // Nonblocking resets to distinct registers commute.
        (
            "counter_fsm",
            "reordered resets",
            "count <= 0;\n      running <= 1'b0;",
            "running <= 1'b0;\n      count <= 0;",
        ),
        // Sized vs unsized literal of the same value.
        ("operators", "sized literal", "cnt <= 3;", "cnt <= 8'd3;"),
        // Parameter-width cast spelled as a literal of the same value.
        (
            "param_datapath",
            "literal cast width",
            "W'(r + a)",
            "8'(r + a)",
        ),
        // Indexed part-select spelled as a constant range.
        ("param_datapath", "range select", "a[W - 4 +: 4]", "a[7:4]"),
        // Sized vs unsized initializer of the same value.
        (
            "init_reg",
            "sized initializer",
            "logic [7:0] r = 7;",
            "logic [7:0] r = 8'd7;",
        ),
        // Operand order of a constant signed comparison.
        (
            "param_select_async",
            "swapped comparison",
            "if (SEL == 0)",
            "if (0 == SEL)",
        ),
        // Async reset constant spelled as a sized literal.
        (
            "param_select_async",
            "literal reset constant",
            "cnt <= 12'($unsigned(START));\n      q <= 0;",
            "cnt <= 12'd9;\n      q <= 0;",
        ),
    ];
    let mut failures = Vec::new();
    for (i, (name, what, from, to)) in cases.iter().enumerate() {
        let sv = build_sv(name, td.path());
        let lean = cert_for(name, &mutate(&sv, from, to));
        let (accepted, diag) = replay(&lake, &lean, td.path(), &format!("ok{i}"));
        if !accepted {
            failures.push(format!("{name}: `{what}` was REJECTED:\n{diag}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn modules_outside_the_subset_are_reported_not_certified() {
    let source = r#"
module Covered
  port a: in UInt<4>;
  port y: out UInt<4>;
  comb
    y = ~a;
  end comb
end module Covered

module Signed
  port a: in SInt<4>;
  port y: out SInt<4>;
  comb
    y = a;
  end comb
end module Signed
"#;
    let tokens = arch::lexer::tokenize(source).expect("lex");
    let items = arch::parser::Parser::new(tokens, source)
        .parse_source_file()
        .expect("parse")
        .items;
    let sv = "module Covered (\n  input logic [3:0] a,\n  output logic [3:0] y\n);\n  assign y = ~a;\nendmodule\n";
    let cert = arch::codegen_proof_cert::render_lean_items(items.iter(), sv).expect("cert");
    assert_eq!(cert.certified, vec!["Covered".to_string()]);
    assert_eq!(cert.skipped.len(), 1);
    assert_eq!(cert.skipped[0].0, "Signed");
    assert!(cert.skipped[0].1.contains("signed"), "{:?}", cert.skipped);
    assert!(cert.lean.contains("NOT covered"), "{}", cert.lean);

    // SV outside the reader subset is a skip with the reader's reason, and
    // then nothing is certified (the CLI turns that into an error).
    let only_bad = &items[..1];
    let bad_sv = "module Covered (\n  input logic [3:0] a,\n  output logic [3:0] y\n);\n  assign y = a << 1;\nendmodule\n";
    let cert = arch::codegen_proof_cert::render_lean_items(only_bad.iter(), bad_sv).expect("cert");
    assert!(cert.certified.is_empty() && cert.lean.is_empty());
    assert!(
        cert.skipped[0].1.contains("reader subset"),
        "{:?}",
        cert.skipped
    );
}
