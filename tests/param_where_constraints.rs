//! Tests for compile-time param `where` constraint clauses (issue #600).

use arch::codegen::Codegen;
use arch::elaborate;
use arch::lexer;
use arch::parser::Parser;
use arch::resolve;
use arch::typecheck::TypeChecker;

/// Parse only (used for parse-time rejections, e.g. `where` on a type param).
fn parse_source(source: &str) -> Result<(), String> {
    let tokens = lexer::tokenize(source).map_err(|e| format!("{e:?}"))?;
    let mut parser = Parser::new(tokens, source);
    parser
        .parse_source_file()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Full check pipeline (elaborate -> resolve -> typecheck), collapsing all
/// errors into a single joined string for `contains` assertions.
fn check_source(source: &str) -> Result<(), String> {
    let tokens = lexer::tokenize(source).map_err(|e| format!("{e:?}"))?;
    let mut parser = Parser::new(tokens, source);
    let parsed_ast = parser.parse_source_file().map_err(|e| e.to_string())?;
    let ast = elaborate::elaborate(parsed_ast).map_err(|errs| join_errs(&errs))?;
    let symbols = resolve::resolve(&ast).map_err(|errs| join_errs(&errs))?;
    let checker = TypeChecker::new(&symbols, &ast);
    checker.check().map(|_| ()).map_err(|errs| join_errs(&errs))
}

fn join_errs(errs: &[arch::diagnostics::CompileError]) -> String {
    errs.iter()
        .map(|e| e.to_string())
        .collect::<Vec<_>>()
        .join("; ")
}

fn compile_to_sv(source: &str) -> String {
    let tokens = lexer::tokenize(source).expect("lexer error");
    let mut parser = Parser::new(tokens, source);
    let parsed_ast = parser.parse_source_file().expect("parse error");
    let ast = elaborate::elaborate(parsed_ast).expect("elaborate error");
    let ast = elaborate::lower_tlm_target_threads(ast).expect("tlm_target lowering error");
    let ast = elaborate::lower_tlm_initiator_calls(ast).expect("tlm_initiator lowering error");
    let ast = elaborate::lower_threads_with_opts(ast, &elaborate::ThreadLowerOpts::default())
        .expect("lower_threads error");
    let ast = (elaborate::lower_pipe_reg_ports(ast).expect("lower_pipe_reg_ports error")).0;
    let ast = elaborate::lower_credit_channel_dispatch(ast).expect("credit_channel dispatch error");
    let symbols = resolve::resolve(&ast).expect("resolve error");
    let checker = TypeChecker::new(&symbols, &ast);
    let (_warnings, overload_map) = checker.check().expect("type check error");
    let mut codegen = Codegen::new(&symbols, &ast, overload_map);
    codegen.generate()
}

const CACHE_SRC: &str = r#"
module Cache
  param SIZE:   const = 1024 where SIZE > 0 and (SIZE & (SIZE - 1)) == 0;
  param DATA_W: const = 32   where DATA_W == 8 or DATA_W == 16 or DATA_W == 32 or DATA_W == 64;
  param STAGES: const = 2    where STAGES >= 1 and STAGES <= 8;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port din: in UInt<DATA_W>;
  port dout: out UInt<DATA_W>;
  comb
    dout = din;
  end comb
end module Cache
"#;

// ── Definition-site ─────────────────────────────────────────────────────────

#[test]
fn test_where_default_satisfied_compiles() {
    check_source(CACHE_SRC).expect("satisfied default must compile");
}

#[test]
fn test_where_default_violation_error_message() {
    let src = CACHE_SRC.replace("SIZE:   const = 1024", "SIZE:   const = 3");
    let err = check_source(&src).expect_err("SIZE=3 is not a power of two, must error");
    assert!(
        err.contains("default value SIZE=3 violates constraint"),
        "unexpected error: {err}"
    );
    assert!(
        err.contains("(SIZE & (SIZE - 1)) == 0") || err.contains("(SIZE & (SIZE - 1)) == 0)"),
        "constraint source must be quoted verbatim: {err}"
    );
}

#[test]
fn test_where_nonbool_definition_error() {
    let src = r#"
module NonBool
  param N: const = 4 where N + 1;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port x: in UInt<N>;
  port y: out UInt<N>;
  comb
    y = x;
  end comb
end module NonBool
"#;
    let err = check_source(src).expect_err("non-Bool where clause must error");
    assert!(
        err.contains("must be a Bool expression"),
        "unexpected error: {err}"
    );
}

#[test]
fn test_where_on_type_param_rejected() {
    let src = r#"
module TypeParam
  param T: type = UInt<8> where T == UInt<8>;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port x: in T;
  port y: out T;
  comb
    y = x;
  end comb
end module TypeParam
"#;
    let err = parse_source(src).expect_err("where on a type param must be a parse error");
    assert!(
        err.contains("only allowed on `const` params"),
        "unexpected error: {err}"
    );
}

// ── Instantiation-site ──────────────────────────────────────────────────────

fn top_with_size(size: u64) -> String {
    format!(
        r#"
{CACHE_SRC}
module Top
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port din: in UInt<32>;
  port dout: out UInt<32>;
  inst l1d: Cache
    clk <- clk;
    rst <- rst;
    din <- din;
    dout -> dout;
    param SIZE = {size};
  end inst l1d
end module Top
"#
    )
}

#[test]
fn test_where_instantiation_satisfied() {
    check_source(&top_with_size(512)).expect("SIZE=512 is a valid power of two");
}

#[test]
fn test_where_instantiation_violation_both_locations() {
    let err = check_source(&top_with_size(3)).expect_err("SIZE=3 must violate the constraint");
    assert!(
        err.contains("param `SIZE`=3 in inst `l1d`"),
        "must name the instantiation site: {err}"
    );
    assert!(
        err.contains("declared on `Cache`"),
        "must name the declaring construct: {err}"
    );
    assert!(
        err.contains("(SIZE & (SIZE - 1)) == 0"),
        "constraint source must be quoted verbatim: {err}"
    );
}

#[test]
fn test_where_derived_param_override_violation() {
    let src = r#"
module Derived
  param A: const = 16;
  param B: const = A * 2 where B <= 64;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port x: in UInt<A>;
  port y: out UInt<A>;
  comb
    y = x;
  end comb
end module Derived

module TopD
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port x: in UInt<40>;
  port y: out UInt<40>;
  inst d: Derived
    clk <- clk;
    rst <- rst;
    x <- x;
    y -> y;
    param A = 40;
  end inst d
end module TopD
"#;
    let err = check_source(src)
        .expect_err("A=40 => B=80 violates B <= 64 even though B itself was never overridden");
    assert!(
        err.contains("param `B`=80 in inst `d`"),
        "unexpected error: {err}"
    );
}

#[test]
fn test_where_derived_param_override_satisfied() {
    let src = r#"
module Derived
  param A: const = 16;
  param B: const = A * 2 where B <= 64;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port x: in UInt<A>;
  port y: out UInt<A>;
  comb
    y = x;
  end comb
end module Derived

module TopD
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port x: in UInt<20>;
  port y: out UInt<20>;
  inst d: Derived
    clk <- clk;
    rst <- rst;
    x <- x;
    y -> y;
    param A = 20;
  end inst d
end module TopD
"#;
    check_source(src).expect("A=20 => B=40 satisfies B <= 64");
}

#[test]
fn test_where_generate_for_computed_violation() {
    let src = r#"
module Leaf
  param W: const = 8 where W >= 4 and W <= 32;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port x: in UInt<W>;
  port y: out UInt<W>;
  comb
    y = x;
  end comb
end module Leaf

module Top
  param N: const = 3;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port x: in Vec<UInt<8>, 3>;
  port y: out Vec<UInt<8>, 3>;
  generate_for i in 0..N-1
    inst leaves: Leaf
      clk <- clk;
      rst <- rst;
      x <- x[i];
      y -> y[i];
      param W = i * 4;
    end inst leaves
  end generate_for
end module Top
"#;
    // i=0 => W=0, which violates `W >= 4`. Elaboration monomorphizes `Leaf`
    // per distinct generate_for-computed param set before typecheck runs,
    // so the violation surfaces as a definition-site error on the
    // monomorphized W=0 variant rather than at the (already-expanded) inst
    // site — either way, the elaborated design must be rejected.
    let err = check_source(src).expect_err("generate-for i=0 computes W=0, must violate W >= 4");
    assert!(
        err.contains("W=0") && err.contains("violates constraint"),
        "unexpected error: {err}"
    );
}

// ── Codegen purity ───────────────────────────────────────────────────────────

#[test]
fn test_where_erased_before_codegen_sv_identical() {
    let with_where = compile_to_sv(CACHE_SRC);
    let without_where = compile_to_sv(
        &CACHE_SRC
            .replace(
                "SIZE:   const = 1024 where SIZE > 0 and (SIZE & (SIZE - 1)) == 0;",
                "SIZE:   const = 1024;",
            )
            .replace(
                "DATA_W: const = 32   where DATA_W == 8 or DATA_W == 16 or DATA_W == 32 or DATA_W == 64;",
                "DATA_W: const = 32;",
            )
            .replace(
                "STAGES: const = 2    where STAGES >= 1 and STAGES <= 8;",
                "STAGES: const = 2;",
            ),
    );
    assert_eq!(
        with_where, without_where,
        "SV codegen must be byte-identical for a satisfying design with/without `where`"
    );
}

// ── .archi separate compilation (real binary, real files) ──────────────────

#[test]
fn test_where_archi_separate_compilation_enforces_at_downstream_site() {
    let bin = env!("CARGO_BIN_EXE_arch");
    let dir = std::env::temp_dir().join(format!(
        "arch_where_archi_test_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();

    let cache_path = dir.join("Cache.arch");
    std::fs::write(&cache_path, CACHE_SRC).unwrap();

    // Build Cache.arch to emit Cache.archi in `dir` (arch writes the .archi
    // next to the working directory — invoke with `dir` as cwd).
    let sv_out = dir.join("Cache.sv");
    let status = std::process::Command::new(bin)
        .current_dir(&dir)
        .args(["build", "Cache.arch", "-o", "Cache.sv"])
        .status()
        .expect("failed to run arch build");
    assert!(status.success(), "arch build Cache.arch failed");
    assert!(
        dir.join("Cache.archi").exists(),
        "Cache.archi was not emitted"
    );
    assert!(sv_out.exists());

    // Remove the original .arch source so downstream compilation can only
    // see the .archi interface stub (true separate-compilation test).
    std::fs::remove_file(&cache_path).unwrap();

    let top_src = r#"
module Top
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port din: in UInt<32>;
  port dout: out UInt<32>;
  inst l1d: Cache
    clk <- clk;
    rst <- rst;
    din <- din;
    dout -> dout;
    param SIZE = 3;
  end inst l1d
end module Top
"#;
    let top_path = dir.join("top.arch");
    std::fs::write(&top_path, top_src).unwrap();

    let output = std::process::Command::new(bin)
        .current_dir(&dir)
        .args(["check", "top.arch"])
        .output()
        .expect("failed to run arch check");
    assert!(
        !output.status.success(),
        "expected arch check to fail on a constraint-violating override via .archi"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("violates constraint"),
        "expected constraint violation in stderr, got: {stderr}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// ── Bus port param overrides ────────────────────────────────────────────────

const BUS_SRC: &str = r#"
bus MiniBus
  param DATA_W: const = 32 where DATA_W == 8 or DATA_W == 16 or DATA_W == 32 or DATA_W == 64;
  valid: out Bool;
  data:  out UInt<DATA_W>;
end bus MiniBus
"#;

#[test]
fn test_where_bus_port_override_satisfied() {
    let src = format!(
        r#"
{BUS_SRC}
module M
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port b: initiator MiniBus<DATA_W=64>;
  comb
    b.valid = 1;
    b.data = 0;
  end comb
end module M
"#
    );
    check_source(&src).expect("DATA_W=64 satisfies the bus constraint");
}

#[test]
fn test_where_bus_port_override_violation() {
    let src = format!(
        r#"
{BUS_SRC}
module M
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port b: initiator MiniBus<DATA_W=48>;
  comb
    b.valid = 1;
    b.data = 0;
  end comb
end module M
"#
    );
    let err = check_source(&src).expect_err("DATA_W=48 violates the bus constraint");
    assert!(
        err.contains("param `DATA_W`=48 on bus port `b` violates constraint declared on `MiniBus`"),
        "unexpected error: {err}"
    );
}

#[test]
fn test_where_bus_default_violation() {
    let src = BUS_SRC.replace("const = 32 where", "const = 48 where");
    let err = check_source(&src).expect_err("bus default DATA_W=48 violates its own constraint");
    assert!(
        err.contains("default value DATA_W=48 violates constraint"),
        "unexpected error: {err}"
    );
}
