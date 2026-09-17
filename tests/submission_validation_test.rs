use std::{fs, process::Command};
fn check(src: &str, error: Option<&str>) {
    let d = tempfile::tempdir().unwrap();
    let f = d.path().join("test.arch");
    fs::write(&f, src).unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_arch"))
        .env("ARCH_NO_LEARN", "1")
        .args(["check", f.to_str().unwrap()])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&o.stderr);
    if let Some(msg) = error {
        assert!(!o.status.success());
        assert!(text.contains(msg), "{text}");
    } else {
        assert!(o.status.success(), "{text}");
    }
}
#[test]
fn repeated_port_keyword_hint_preserves_reset_name() {
    check(
        "module M\n port clk: in Clock<D>;\n reset: in Reset<Async, High>;\nend module M",
        Some("repeat `port`"),
    );
    check(
        "module M\n port clk: in Clock<D>;\n port reset: in Reset<Async, High>;\nend module M",
        None,
    );
}
#[test]
fn continuously_driven_let_cannot_also_have_comb_driver() {
    check("module M\n port y: out UInt<8>;\n let x: UInt<8> = 0;\n comb x = 1; y = x; end comb\nend module M", Some("multiple"));
    check("module M\n port y: out UInt<8>;\n wire x: UInt<8>;\n comb x = 1; y = x; end comb\nend module M", None);
}

fn build(src: &str) -> String {
    let d = tempfile::tempdir().unwrap();
    let f = d.path().join("test.arch");
    let out = d.path().join("test.sv");
    fs::write(&f, src).unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_arch"))
        .env("ARCH_NO_LEARN", "1")
        .args(["build", "-o", out.to_str().unwrap(), f.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    fs::read_to_string(out).unwrap()
}
#[test]
fn implicit_instance_outputs_precede_let_uses() {
    let sv = build("module Child\n port y: out UInt<8>;\n let y = 8'b00000001;\nend module Child\nmodule Parent\n port y: out UInt<8>;\n inst c: Child\n y -> target;\n end inst c\n let y = target;\nend module Parent");
    assert!(
        sv.find("logic [7:0] target;").unwrap() < sv.find("assign y = target;").unwrap(),
        "{sv}"
    );
}
#[test]
fn fsm_wires_precede_let_uses() {
    let sv = build("fsm M\n port clk: in Clock<D>;\n port rst: in Reset<Async>;\n port y: out UInt<8>;\n wire target: UInt<8>;\n let delta: UInt<8> = target;\n state [S]\n default state S;\n default\n comb target = 0; y = delta; end comb\n end default\n state S\n -> S when true;\n end state S\nend fsm M");
    assert!(
        sv.find("logic [7:0] target;").unwrap() < sv.find("assign delta = target;").unwrap(),
        "{sv}"
    );
}
