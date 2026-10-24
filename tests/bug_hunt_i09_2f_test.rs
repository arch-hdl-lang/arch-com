//! Regression coverage for E8M0 conversion through a ternary receiver.

use std::process::Command;

fn arch() -> Command {
    Command::new(env!("CARGO_BIN_EXE_arch"))
}

#[test]
fn formal_e8m0_ternary_receiver_decodes_selected_scale() {
    if !Command::new("z3")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        eprintln!("skipping: z3 not in PATH");
        return;
    }

    let td = tempfile::tempdir().expect("tempdir");
    let source = td.path().join("E8M0Ternary.arch");
    std::fs::write(
        &source,
        r#"module E8M0Ternary
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port select: in Bool;
  port a: in E8M0;
  port b: in E8M0;
  port out: out FP32;
  port nested: out FP32;
  let picked: E8M0 = select ? (select ? a : b) : b;
  comb out = (select ? a : b).to_fp32(); end comb
  comb nested = picked.to_fp32(); end comb
  assert selected_scale: is_nan(a) || is_nan(b) ||
    (out == (select ? a.to_fp32() : b.to_fp32()));
  assert nested_scale: is_nan(a) || is_nan(b) ||
    (nested == (select ? (select ? a.to_fp32() : b.to_fp32()) : b.to_fp32()));
end module E8M0Ternary
"#,
    )
    .expect("write regression fixture");

    let out = arch()
        .arg("formal")
        .arg(&source)
        .arg("--bound")
        .arg("2")
        .output()
        .expect("run arch formal");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let report = format!("{stdout}{stderr}");
    assert!(
        out.status.success()
            && report.contains("selected_scale")
            && report.contains("nested_scale")
            && report.matches("PROVED").count() >= 2,
        "ternary E8M0 conversion should prove selected scale equivalence\nstdout:\n{report}\nstderr:\n{}",
        stderr
    );
}
