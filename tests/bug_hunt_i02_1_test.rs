use std::fs;
use std::path::Path;
use std::process::Command;

use tempfile::tempdir;

fn arch_check(source: &str, build: bool) -> std::process::Output {
    let dir = tempdir().expect("create test directory");
    let file = dir.path().join("ternary.arch");
    fs::write(&file, source).expect("write ARCH source");
    let command = if build { "build" } else { "check" };
    Command::new(env!("CARGO_BIN_EXE_arch"))
        .arg(command)
        .arg(Path::new(&file))
        .output()
        .expect("run arch")
}

fn assert_rejected(source: &str, needle: &str) {
    let output = arch_check(source, false);
    assert!(
        !output.status.success(),
        "source unexpectedly passed: {source}"
    );
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        diagnostics.contains(needle),
        "diagnostic did not contain `{needle}`:\n{diagnostics}"
    );
}

#[test]
fn ternary_resolves_every_child_and_requires_matching_runtime_types() {
    let missing_cond = r#"
module MissingCond
  port a: in UInt<8>;
  port b: in UInt<8>;
  port y: out UInt<8>;
  comb
    y = missing ? a : b;
  end comb
end module MissingCond
"#;
    assert_rejected(missing_cond, "undefined name: `missing`");

    let missing_else = r#"
module MissingElse
  port sel: in Bool;
  port a: in UInt<8>;
  port y: out UInt<8>;
  comb
    y = sel ? a : missing;
  end comb
end module MissingElse
"#;
    assert_rejected(missing_else, "undefined name: `missing`");

    for (name, then_ty, else_ty) in [
        ("WidthForward", "UInt<8>", "UInt<16>"),
        ("WidthReverse", "UInt<16>", "UInt<8>"),
        ("SignedForward", "UInt<8>", "SInt<8>"),
        ("SignedReverse", "SInt<8>", "UInt<8>"),
    ] {
        let source = format!(
            "module {name}\n  port sel: in Bool;\n  port a: in {then_ty};\n  port b: in {else_ty};\n  port y: out UInt<16>;\n  comb\n    y = sel ? a : b;\n  end comb\nend module {name}\n"
        );
        assert_rejected(&source, "ternary branches must have matching types");
    }
}

#[test]
fn ternary_accepts_explicit_width_conversions_and_context_typed_float_arms() {
    let explicit = r#"
module ExplicitTernary
  port sel: in Bool;
  port a: in UInt<8>;
  port b: in UInt<16>;
  port y: out UInt<16>;
  comb
    y = sel ? a.zext<16>() : b;
  end comb
end module ExplicitTernary
"#;
    assert!(arch_check(explicit, true).status.success());

    let integer_literal = r#"
module LiteralTernary
  port sel: in Bool;
  port a: in UInt<8>;
  port y: out UInt<8>;
  comb
    y = sel ? a : 0;
  end comb
end module LiteralTernary
"#;
    assert!(arch_check(integer_literal, true).status.success());

    let uint1_condition = r#"
module UInt1Condition
  port sel: in UInt<1>;
  port a: in UInt<8>;
  port y: out UInt<8>;
  comb
    y = sel ? a : 0;
  end comb
end module UInt1Condition
"#;
    assert!(arch_check(uint1_condition, true).status.success());

    let literal_contexts = r#"
module LiteralContexts
  port sel: in Bool;
  port a: in SInt<8>;
  port b: in Bool;
  port y: out SInt<8>;
  port z: out Bool;
  comb
    y = sel ? a : -1;
    z = sel ? b : false;
  end comb
end module LiteralContexts
"#;
    assert!(arch_check(literal_contexts, true).status.success());

    let floats = r#"
module FloatTernary
  port sel: in Bool;
  port a: in BF16;
  port y: out BF16;
  comb
    y = sel ? 0.5 : a;
  end comb
end module FloatTernary
"#;
    assert!(arch_check(floats, true).status.success());
}

#[test]
fn ternary_preserves_nested_unsized_literal_widths() {
    let source = r#"
module NestedLiteralTernary
  port sel: in Bool;
  port subsel: in Bool;
  port y: out UInt<2>;
  comb y = sel ? 3 : (subsel ? 2 : 0); end comb
end module NestedLiteralTernary
"#;
    let output = arch_check(source, true);
    assert!(
        output.status.success(),
        "nested literal arms must preserve their widest value: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
