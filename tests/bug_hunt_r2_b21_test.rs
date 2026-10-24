//! Regression coverage for parameterized bus field reads from wires (B21).

use arch::codegen::Codegen;
use arch::elaborate;
use arch::lexer;
use arch::parser::Parser;
use arch::resolve;
use arch::typecheck::TypeChecker;

fn check_source(source: &str) -> Result<String, String> {
    let tokens = lexer::tokenize(source).map_err(|e| format!("lex: {e:?}"))?;
    let mut parser = Parser::new(tokens, source);
    let parsed = parser
        .parse_source_file()
        .map_err(|e| format!("parse: {e}"))?;
    let ast = elaborate::elaborate(parsed).map_err(|e| format!("elaborate: {e:?}"))?;
    let symbols = resolve::resolve(&ast).map_err(|e| format!("resolve: {e:?}"))?;
    let checker = TypeChecker::new(&symbols, &ast);
    let (_warnings, overloads) = checker.check().map_err(|errors| {
        errors
            .into_iter()
            .map(|error| error.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    })?;
    Ok(Codegen::new(&symbols, &ast, overloads).generate())
}

const BUS: &str = r#"
bus B
  param W: const = 32;
  param D: const = W + 1;
  data: in UInt<W>;
  tag: in UInt<D>;
end bus B
"#;

#[test]
fn direct_parameterized_bus_wire_field_reads_use_wire_overrides() {
    let source = format!(
        r#"{BUS}
module Narrow
  wire x: B<W=8, D=2>;
  port data: out UInt<8>;
  port tag: out UInt<2>;
  comb
    x.data = 8'hff;
    x.tag = 2'h3;
    data = x.data;
    tag = x.tag;
  end comb
end module Narrow
"#
    );
    let sv = check_source(&source).expect("parameterized bus wire should typecheck");
    assert!(sv.contains("logic [7:0] x_data"), "generated SV:\n{sv}");
    assert!(sv.contains("logic [1:0] x_tag"), "generated SV:\n{sv}");
}

#[test]
fn direct_parameterized_bus_wire_field_read_rejects_narrower_destination() {
    let source = format!(
        r#"{BUS}
module Wider
  wire x: B<W=8>;
  port data: out UInt<4>;
  comb
    x.data = 8'hff;
    data = x.data;
  end comb
end module Wider
"#
    );
    let error =
        check_source(&source).expect_err("narrower destination must require an explicit cast");
    assert!(
        error.contains("width mismatch"),
        "unexpected error: {error}"
    );
}

#[test]
fn parameterized_bus_wire_field_reads_preserve_defaults_and_dependent_fields() {
    let source = format!(
        r#"{BUS}
module Defaults
  wire x: B;
  port data: out UInt<32>;
  port tag: out UInt<33>;
  comb
    x.data = 32'hffff;
    x.tag = 33'h1ffff;
    data = x.data;
    tag = x.tag;
  end comb
end module Defaults
"#
    );
    let sv = check_source(&source).expect("bus defaults and dependent field width should work");
    assert!(sv.contains("logic [31:0] x_data"), "generated SV:\n{sv}");
}

#[test]
fn parameterized_bus_wire_field_reads_resolve_module_param_overrides() {
    let source = format!(
        r#"{BUS}
module ParamWire
  param WIDTH: const = 8;
  wire x: B<W=WIDTH>;
  port data: out UInt<8>;
  port tag: out UInt<9>;
  comb
    x.data = 8'hff;
    x.tag = 9'h1ff;
    data = x.data;
    tag = x.tag;
  end comb
end module ParamWire
"#
    );
    check_source(&source).expect("wire override should fold in module parameter scope");
}
