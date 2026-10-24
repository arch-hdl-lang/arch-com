//! Regression coverage for `##N` parser inputs that end before the cycle count.

use arch::lexer::tokenize;
use arch::parser::Parser;

fn parse_result(source: &str) -> Result<(), String> {
    let tokens = tokenize(source).map_err(|err| format!("lexer error: {err:?}"))?;
    let mut parser = Parser::new(tokens, source);
    parser
        .parse_source_file()
        .map(|_| ())
        .map_err(|err| err.to_string())
}

#[test]
fn hashhash_eof_returns_parse_error_without_panicking() {
    for source in [
        "module Bad\n assert p: ##",
        "module Bad\n assert p: ##\n/// trailing",
    ] {
        let error = parse_result(source).expect_err("incomplete ## must be rejected");
        assert!(
            error.contains("unexpected end of file"),
            "expected EOF diagnostic, got: {error}"
        );
    }
}

#[test]
fn hashhash_diagnostics_and_valid_forms_are_preserved() {
    let error = parse_result("module Bad\n assert p: ##true;\nend module Bad")
        .expect_err("nonnumeric ## count must be rejected");
    assert!(
        error.contains("`##N` requires an integer literal cycle count"),
        "unexpected diagnostic: {error}"
    );

    parse_result("module Good\n assert p: ##1 true;\nend module Good")
        .expect("valid ## count and operand must parse");

    let error =
        parse_result("module Bad\n assert p: ##1").expect_err("missing operand must be rejected");
    assert!(
        error.contains("unexpected end of file"),
        "expected operand EOF diagnostic, got: {error}"
    );
}
