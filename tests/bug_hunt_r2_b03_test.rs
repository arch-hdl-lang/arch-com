use std::fs;
use std::process::Command;

fn check_source(source: &str) -> (bool, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("match.arch");
    fs::write(&path, source).expect("write ARCH source");
    let output = Command::new(env!("CARGO_BIN_EXE_arch"))
        .arg("check")
        .arg(&path)
        .output()
        .expect("run arch check");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    (output.status.success(), text)
}

const MATCH_PREFIX: &str = r#"
module MatchCase
  port c: in UInt<2>;
  port a: in UInt<8>;
  port o: out UInt<8>;
"#;

fn finish_module(body: &str) -> String {
    format!("{MATCH_PREFIX}{body}\nend module MatchCase\n")
}

#[test]
fn expr_match_checks_later_named_and_wildcard_values() {
    for value in ["missing", "1 => missing,"] {
        let body = if value == "missing" {
            format!(
                "  let value: UInt<8> = match c\n    0 => a,\n    _ => {value},\n  end match;\n  comb o = value; end comb\n"
            )
        } else {
            format!(
                "  let value: UInt<8> = match c\n    0 => a,\n    {value}\n    _ => a,\n  end match;\n  comb o = value; end comb\n"
            )
        };
        let (ok, diagnostics) = check_source(&finish_module(&body));
        assert!(
            !ok,
            "later undefined expression-match value was accepted: {diagnostics}"
        );
        assert!(
            diagnostics.contains("undefined name") && diagnostics.contains("missing"),
            "expected undefined-name diagnostic, got:\n{diagnostics}"
        );
    }
}

#[test]
fn expr_match_wider_later_signal_is_checked_by_enclosing_target() {
    let source = r#"
module MatchWide
  port c: in UInt<2>;
  port a: in UInt<8>;
  port b: in UInt<16>;
  port o: out UInt<8>;
  let value: UInt<8> = match c
    0 => a,
    _ => b,
  end match;
  comb o = value; end comb
end module MatchWide
"#;
    let (ok, diagnostics) = check_source(source);
    assert!(
        !ok,
        "wider later match arm was silently truncated: {diagnostics}"
    );
    assert!(
        diagnostics.contains("UInt<8>") && diagnostics.contains("UInt<16>"),
        "expected width mismatch diagnostic, got:\n{diagnostics}"
    );
}

#[test]
fn expr_match_preserves_compatible_literals_casts_and_todo() {
    let cases = [
        r#"  let value: UInt<8> = match c
    0 => a,
    1 => a,
    _ => a,
  end match;
  comb o = value; end comb
"#,
        r#"  let value: UInt<8> = match c
    0 => a,
    1 => a.zext<16>().trunc<8>(),
    _ => 0,
  end match;
  comb o = value; end comb
"#,
        r#"  let value: UInt<8> = match c
    0 => 10,
    1 => 20,
    _ => 0,
  end match;
  comb o = value; end comb
"#,
        r#"  let value: UInt<1> = match c
    0 => a[0],
    _ => todo!,
  end match;
  comb o = value; end comb
"#,
        r#"  let value: UInt<8> = match c
    0 => todo!,
    _ => a,
  end match;
  comb o = value; end comb
"#,
    ];
    for body in cases {
        let (ok, diagnostics) = check_source(&finish_module(body));
        assert!(
            ok,
            "compatible expression-match case was rejected: {diagnostics}"
        );
    }
}

#[test]
fn expr_match_leading_todo_does_not_hide_later_width_or_format_errors() {
    let wide = r#"
module MatchTodoWide
  port c: in UInt<1>;
  port a: in UInt<16>;
  port o: out UInt<8>;
  let value: UInt<8> = match c
    0 => todo!,
    _ => a,
  end match;
  comb o = value; end comb
end module MatchTodoWide
"#;
    let (ok, diagnostics) = check_source(wide);
    assert!(!ok, "leading todo! hid a later wider arm: {diagnostics}");
    assert!(
        diagnostics.contains("UInt<8>") && diagnostics.contains("UInt<16>"),
        "expected width mismatch diagnostic, got:\n{diagnostics}"
    );

    let mixed_float = r#"
module MatchTodoFloat
  port c: in UInt<2>;
  port a: in FP32;
  port b: in BF16;
  port o: out FP32;
  let value: FP32 = match c
    0 => todo!,
    1 => b,
    _ => a,
  end match;
  comb o = value; end comb
end module MatchTodoFloat
"#;
    let (ok, diagnostics) = check_source(mixed_float);
    assert!(
        !ok,
        "leading todo! hid incompatible float arms: {diagnostics}"
    );
    assert!(
        diagnostics.contains("FP32") && diagnostics.contains("BF16"),
        "expected float format mismatch diagnostic, got:\n{diagnostics}"
    );
}

#[test]
fn expr_match_rejects_incompatible_float_arm_formats() {
    let source = r#"
module MatchFloat
  port c: in UInt<1>;
  port a: in FP32;
  port b: in BF16;
  port o: out FP32;
  let value: FP32 = match c
    0 => a,
    _ => b,
  end match;
  comb o = value; end comb
end module MatchFloat
"#;
    let (ok, diagnostics) = check_source(source);
    assert!(
        !ok,
        "incompatible float match arms were accepted: {diagnostics}"
    );
}
