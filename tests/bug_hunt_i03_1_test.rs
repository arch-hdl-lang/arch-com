//! Regression tests for reset override variant naming (bug-hunt I03-1).

use arch::codegen::Codegen;
use arch::elaborate;
use arch::lexer;
use arch::parser::Parser;
use arch::resolve;
use arch::typecheck::TypeChecker;

fn compile_to_sv(source: &str) -> String {
    let tokens = lexer::tokenize(source).expect("lexer error");
    let mut parser = Parser::new(tokens, source);
    let parsed = parser.parse_source_file().expect("parse error");
    let ast = elaborate::elaborate(parsed).expect("elaborate error");
    let symbols = resolve::resolve(&ast).expect("resolve error");
    let checker = TypeChecker::new(&symbols, &ast);
    let (_warnings, overload_map) = checker.check().expect("type check error");
    Codegen::new(&symbols, &ast, overload_map).generate()
}

fn reset_source(kind: &str, first_level: &str, second_level: &str) -> String {
    format!(
        r#"
module GenCounter
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port en: in Bool;
  port count: out UInt<8>;
  reg count_r: UInt<8> init 0 reset rst => 0;
  seq on clk rising
    if en
      count_r <= (count_r + 1).trunc<8>();
    end if
  end seq
  comb
    count = count_r;
  end comb
end module GenCounter

module ParamResetTop
  port clk: in Clock<SysDomain>;
  port rst_a: in Reset<{kind}, {first_level}>;
  port rst_b: in Reset<{kind}, {second_level}>;
  port en_a: in Bool;
  port en_b: in Bool;
  port count_a: out UInt<8>;
  port count_b: out UInt<8>;
  wire cnt_a: UInt<8>;
  wire cnt_b: UInt<8>;
  inst a: GenCounter
    clk <- clk; rst <- rst_a as Reset<{kind}, {first_level}>; en <- en_a;
    count -> cnt_a;
  end inst a
  inst b: GenCounter
    clk <- clk; rst <- rst_b as Reset<{kind}, {second_level}>; en <- en_b;
    count -> cnt_b;
  end inst b
  comb
    count_a = cnt_a;
    count_b = cnt_b;
  end comb
end module ParamResetTop
"#,
        kind = kind,
        first_level = first_level,
        second_level = second_level,
    )
}

#[test]
fn reset_variant_names_include_polarity_when_only_level_varies() {
    let sv = compile_to_sv(&reset_source("Sync", "High", "Low"));

    assert!(sv.contains("module GenCounter__rst_Sync_High"), "{sv}");
    assert!(sv.contains("module GenCounter__rst_Sync_Low"), "{sv}");
    assert!(sv.contains("GenCounter__rst_Sync_High a ("), "{sv}");
    assert!(sv.contains("GenCounter__rst_Sync_Low b ("), "{sv}");
    assert!(
        sv.contains("always_ff @(posedge clk)"),
        "sync reset must stay synchronous\n{sv}"
    );
}

#[test]
fn reset_variant_names_distinguish_kind_and_dedup_identical_overrides() {
    let sv = compile_to_sv(&reset_source("Async", "High", "Low"));

    assert!(sv.contains("module GenCounter__rst_Async_High"), "{sv}");
    assert!(sv.contains("module GenCounter__rst_Async_Low"), "{sv}");
    assert!(
        sv.contains("always_ff @(posedge clk or posedge rst)"),
        "{sv}"
    );
    assert!(
        sv.contains("always_ff @(posedge clk or negedge rst)"),
        "{sv}"
    );

    let identical = compile_to_sv(&reset_source("Async", "High", "High"));
    assert_eq!(
        identical.matches("module GenCounter (").count(),
        1,
        "{identical}"
    );
    assert!(
        !identical.contains("GenCounter__rst_"),
        "identical overrides must dedup\n{identical}"
    );
}
