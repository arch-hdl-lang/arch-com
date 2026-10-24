//! Regression coverage for I05-2: Vec-method expansions must stay grouped.

use std::process::Command;

const SOURCE: &str = r#"
module VecMethodPrecedence
  port vec: in Vec<UInt<8>, 2>;
  port needle: in UInt<8>;
  port gate: in Bool;
  port any_gate: out Bool;
  port any_not: out Bool;
  port all_gate: out Bool;
  port all_predicate: out Bool;
  port contains_gate: out Bool;
  port reduce_or_eq: out Bool;
  port reduce_and_gate: out Bool;
  port reduce_xor_eq: out Bool;
  comb
    any_gate = vec.any(item == needle) and gate;
    any_not = not vec.any(item == needle);
    all_gate = vec.all(item == needle) and gate;
    all_predicate = vec.all((item == needle) or (item == 0));
    contains_gate = vec.contains(needle) and gate;
    reduce_or_eq = vec.reduce_or() == needle;
    reduce_and_gate = vec.reduce_and() and gate;
    reduce_xor_eq = vec.reduce_xor() == needle;
  end comb
end module VecMethodPrecedence
"#;

fn build_source(dir: &std::path::Path) -> std::path::PathBuf {
    let source = dir.join("VecMethodPrecedence.arch");
    let output = dir.join("VecMethodPrecedence.sv");
    std::fs::write(&source, SOURCE).expect("write ARCH source");
    let status = Command::new(env!("CARGO_BIN_EXE_arch"))
        .args([
            "build",
            source.to_str().unwrap(),
            "--o",
            output.to_str().unwrap(),
        ])
        .status()
        .expect("run arch build");
    assert!(status.success(), "arch build failed");
    output
}

#[test]
fn vec_methods_are_grouped_in_sv() {
    let dir = tempfile::tempdir().expect("tempdir");
    let output = build_source(dir.path());
    let sv = std::fs::read_to_string(output).expect("read generated SV");
    assert!(sv.contains("assign any_gate = (vec[0] == needle || vec[1] == needle) && gate;"));
    assert!(sv.contains("assign any_not = !(vec[0] == needle || vec[1] == needle);"));
    assert!(sv.contains("assign all_gate = (vec[0] == needle && vec[1] == needle) && gate;"));
    assert!(
        sv.contains("assign contains_gate = ((vec[0] == needle) || (vec[1] == needle)) && gate;")
    );
    assert!(sv.contains("assign reduce_or_eq = (vec[0] | vec[1]) == needle;"));
    assert!(sv.contains("assign reduce_and_gate = (vec[0] & vec[1]) && gate;"));
    assert!(sv.contains("assign reduce_xor_eq = (vec[0] ^ vec[1]) == needle;"));
}

#[test]
fn vec_any_precedence_matches_icarus() {
    let dir = tempfile::tempdir().expect("tempdir");
    let output = build_source(dir.path());
    let Some(iverilog) = which("iverilog") else {
        eprintln!("skipping Icarus regression: iverilog not in PATH");
        return;
    };
    let tb = dir.path().join("tb.sv");
    std::fs::write(
        &tb,
        r#"module tb;
  logic [1:0][7:0] vec;
  logic [7:0] needle;
  logic gate;
  logic any_gate, any_not, all_gate, all_predicate, contains_gate, reduce_or_eq, reduce_and_gate, reduce_xor_eq;
  VecMethodPrecedence dut(.*);
  initial begin
    vec = 16'h0100; needle = 8'h01; gate = 1'b0; #1;
    if (any_gate !== 1'b0 || any_not !== 1'b0 || all_gate !== 1'b0 || contains_gate !== 1'b0 ||
        reduce_or_eq !== 1'b1 || reduce_and_gate !== 1'b0 || reduce_xor_eq !== 1'b1) $fatal;
    gate = 1'b1; #1;
    if (any_gate !== 1'b1 || all_gate !== 1'b0 || contains_gate !== 1'b1 || reduce_and_gate !== 1'b0) $fatal;
    vec = 16'h0201; #1;
    if (all_predicate !== 1'b0) $fatal(1, "all must group each OR predicate");
    $display("I05-2 PASS");
  end
endmodule
"#,
    )
    .expect("write testbench");
    let vvp = dir.path().join("a.out");
    let compile = Command::new(iverilog)
        .args([
            "-g2012",
            "-o",
            vvp.to_str().unwrap(),
            output.to_str().unwrap(),
            tb.to_str().unwrap(),
        ])
        .output()
        .expect("run iverilog");
    assert!(
        compile.status.success(),
        "iverilog failed: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let run = Command::new("vvp").arg(vvp).output().expect("run vvp");
    assert!(
        run.status.success(),
        "vvp failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(String::from_utf8_lossy(&run.stdout).contains("I05-2 PASS"));
}

fn which(program: &str) -> Option<String> {
    Command::new("sh")
        .args(["-c", &format!("command -v {program}")])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
}
