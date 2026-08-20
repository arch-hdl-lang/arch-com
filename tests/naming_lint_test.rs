//! Tests for the opt-in naming-convention lint (issue #648, `--lint-naming`).
//!
//! Two layers:
//!   - Library-level: parse a snippet and call `arch::typecheck::check_naming`
//!     directly. The lint is pure casing logic over the *parsed* (pre-
//!     elaboration) AST, so no resolve/typecheck/elaborate is needed — this
//!     mirrors how `param_where_constraints.rs` tests `check_precedence`-style
//!     passes.
//!   - CLI-level: exercise the `--lint-naming` flag itself (opt-in gating,
//!     `warn`/`off`/`error` values, the per-file suppression pragma, and
//!     byte-identical codegen with/without the flag) via the built binary.

use arch::lexer;
use arch::parser::Parser;
use arch::typecheck;

/// Parse `source` and run the naming lint. Panics on a parse error (every
/// fixture below is meant to parse cleanly — semantic validity doesn't
/// matter since `check_naming` never resolves/typechecks).
fn naming_warnings(source: &str) -> Vec<String> {
    let tokens = lexer::tokenize(source).expect("lexer error");
    let mut parser = Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse error");
    typecheck::check_naming(&ast)
        .into_iter()
        .map(|w| w.message)
        .collect()
}

fn assert_no_warnings(source: &str) {
    let warnings = naming_warnings(source);
    assert!(
        warnings.is_empty(),
        "expected zero naming warnings, got: {warnings:?}"
    );
}

/// Assert exactly one warning fires and its message equals `expected`
/// verbatim (exact-text check, not just `contains`).
fn assert_one_warning(source: &str, expected: &str) {
    let warnings = naming_warnings(source);
    assert_eq!(warnings, vec![expected.to_string()]);
}

// ── module (PascalCase) ────────────────────────────────────────────────────

const MODULE_TMPL: &str = r#"
module {NAME}
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port a: in Bool;
  port y: out Bool;
  comb
    y = a;
  end comb
end module {NAME}
"#;

#[test]
fn module_pascal_case_conforming_is_silent() {
    assert_no_warnings(&MODULE_TMPL.replace("{NAME}", "FetchUnit"));
}

#[test]
fn module_acronym_prefix_is_silent() {
    // AXIBridge / FIFOCtrl-style acronym prefixes are legitimate PascalCase.
    assert_no_warnings(&MODULE_TMPL.replace("{NAME}", "AXIBridge"));
    assert_no_warnings(&MODULE_TMPL.replace("{NAME}", "FIFOCtrl"));
}

#[test]
fn module_camelcase_violation_flagged_with_exact_text() {
    assert_one_warning(
        &MODULE_TMPL.replace("{NAME}", "moduleFoo"),
        "module `moduleFoo` should be PascalCase (e.g. `ModuleFoo`) — see naming conventions",
    );
}

#[test]
fn module_snake_case_violation_suggests_titlecase_join() {
    assert_one_warning(
        &MODULE_TMPL.replace("{NAME}", "fetch_unit"),
        "module `fetch_unit` should be PascalCase (e.g. `FetchUnit`) — see naming conventions",
    );
}

// ── port (snake_case) ──────────────────────────────────────────────────────

#[test]
fn port_snake_case_conforming_is_silent() {
    assert_no_warnings(
        r#"
module M
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port req_valid: in Bool;
  port fifo2_ptr: out UInt<8>;
  comb
    fifo2_ptr = req_valid.zext<8>();
  end comb
end module M
"#,
    );
}

#[test]
fn port_pascal_case_violation_flagged() {
    assert_one_warning(
        r#"
module M
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port ReqValid: in Bool;
  port y: out Bool;
  comb
    y = ReqValid;
  end comb
end module M
"#,
        "port `ReqValid` should be snake_case (e.g. `req_valid`) — see naming conventions",
    );
}

// ── param (UPPER_SNAKE) ────────────────────────────────────────────────────

#[test]
fn param_upper_snake_conforming_is_silent() {
    assert_no_warnings(
        r#"
module M
  param XLEN: const = 32;
  local param CACHE_DEPTH: const = 4;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port a: in UInt<XLEN>;
  port y: out UInt<XLEN>;
  comb
    y = a;
  end comb
end module M
"#,
    );
}

#[test]
fn param_lowercase_violation_flagged() {
    assert_one_warning(
        r#"
module M
  param cache_depth: const = 4;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port a: in Bool;
  port y: out Bool;
  comb
    y = a;
  end comb
end module M
"#,
        "param `cache_depth` should be UPPER_SNAKE (e.g. `CACHE_DEPTH`) — see naming conventions",
    );
}

// ── reg / wire (snake_case) ────────────────────────────────────────────────

#[test]
fn reg_wire_conforming_is_silent() {
    assert_no_warnings(
        r#"
module M
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port a: in Bool;
  port y: out Bool;
  reg state_reg: Bool reset rst => 0;
  wire next_state: Bool;
  comb
    next_state = a;
    y = state_reg;
  end comb
  seq on clk rising
    state_reg <= next_state;
  end seq
end module M
"#,
    );
}

#[test]
fn reg_violation_flagged() {
    let warnings = naming_warnings(
        r#"
module M
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port a: in Bool;
  port y: out Bool;
  reg StateReg: Bool reset rst => 0;
  comb
    y = a;
  end comb
  seq on clk rising
    StateReg <= a;
  end seq
end module M
"#,
    );
    assert_eq!(
        warnings,
        vec!["reg `StateReg` should be snake_case (e.g. `state_reg`) — see naming conventions"]
    );
}

#[test]
fn wire_violation_flagged() {
    assert_one_warning(
        r#"
module M
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port a: in Bool;
  port y: out Bool;
  wire NextState: Bool;
  comb
    NextState = a;
    y = NextState;
  end comb
end module M
"#,
        "wire `NextState` should be snake_case (e.g. `next_state`) — see naming conventions",
    );
}

// ── let binding (snake_case), including struct destructuring ──────────────

#[test]
fn let_binding_violation_flagged() {
    assert_one_warning(
        r#"
module M
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port a: in UInt<8>;
  port y: out UInt<8>;
  let ComputedVal: UInt<8> = a;
  comb
    y = ComputedVal;
  end comb
end module M
"#,
        "let binding `ComputedVal` should be snake_case (e.g. `computed_val`) — see naming conventions",
    );
}

#[test]
fn destructure_let_checks_field_names_not_placeholder() {
    // The parser stores a synthesized `_destructure` placeholder in
    // `LetBinding::name` for this form; it must never itself be flagged.
    // The real user-written names live in `destructure_fields` and *are*
    // checked individually.
    let warnings = naming_warnings(
        r#"
struct Pair
  FieldA: UInt<8>;
  FieldB: UInt<8>;
end struct Pair

module M
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port p: in Pair;
  port y: out UInt<8>;
  let {FieldA, FieldB} = p;
  comb
    y = FieldA +% FieldB;
  end comb
end module M
"#,
    );
    assert!(
        !warnings.iter().any(|w| w.contains("_destructure")),
        "parser's synthesized destructure placeholder leaked into a diagnostic: {warnings:?}"
    );
    assert_eq!(
        warnings,
        vec![
            "let binding `FieldA` should be snake_case (e.g. `field_a`) — see naming conventions",
            "let binding `FieldB` should be snake_case (e.g. `field_b`) — see naming conventions",
        ]
    );
}

#[test]
fn destructure_let_conforming_is_silent() {
    assert_no_warnings(
        r#"
struct Pair
  a: UInt<8>;
  b: UInt<8>;
end struct Pair

module M
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port p: in Pair;
  port y: out UInt<8>;
  let {a, b} = p;
  comb
    y = a +% b;
  end comb
end module M
"#,
    );
}

// ── function args / locals (snake_case) ────────────────────────────────────

#[test]
fn function_arg_and_local_violation_flagged() {
    let warnings = naming_warnings(
        r#"
module M
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port a: in UInt<8>;
  port y: out UInt<8>;

  function helperFn(InputArg: UInt<8>) -> UInt<8>
    let LocalTmp: UInt<8> = InputArg;
    return LocalTmp;
  end function helperFn

  comb
    y = helperFn(a);
  end comb
end module M
"#,
    );
    assert_eq!(
        warnings,
        vec![
            "function arg `InputArg` should be snake_case (e.g. `input_arg`) — see naming conventions",
            "let binding `LocalTmp` should be snake_case (e.g. `local_tmp`) — see naming conventions",
        ]
    );
}

#[test]
fn function_conforming_is_silent_including_function_name_itself() {
    // `function` names have no documented casing convention (not in issue
    // #648's construct-name list) — a camelCase function name must not be
    // flagged, only its args/locals are in scope.
    assert_no_warnings(
        r#"
module M
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port a: in UInt<8>;
  port y: out UInt<8>;

  function helperFn(input_arg: UInt<8>) -> UInt<8>
    let local_tmp: UInt<8> = input_arg;
    return local_tmp;
  end function helperFn

  comb
    y = helperFn(a);
  end comb
end module M
"#,
    );
}

// ── other first-class constructs (PascalCase names) ─────────────────────────

#[test]
fn fsm_pascal_case() {
    assert_one_warning(
        r#"
fsm vending_machine
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port active: out Bool;

  state [Idle, Running]
  default state Idle;

  default
    comb
      active = false;
    end comb
  end default

  state Idle
    -> Running when true;
  end state Idle

  state Running
    comb
      active = true;
    end comb
    -> Idle when true;
  end state Running
end fsm vending_machine
"#,
        "fsm `vending_machine` should be PascalCase (e.g. `VendingMachine`) — see naming conventions",
    );
}

#[test]
fn fifo_pascal_case() {
    assert_one_warning(
        r#"
fifo tx_queue
  param DEPTH: const = 8;
  param T: type = UInt<8>;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port push_valid: in Bool;
  port push_ready: out Bool;
  port push_data: in T;
  port pop_valid: out Bool;
  port pop_ready: in Bool;
  port pop_data: out T;
end fifo tx_queue
"#,
        "fifo `tx_queue` should be PascalCase (e.g. `TxQueue`) — see naming conventions",
    );
}

#[test]
fn arbiter_pascal_case_and_port_array_signals() {
    let warnings = naming_warnings(
        r#"
arbiter bus_arbiter
  policy round_robin;
  param N: const = 4;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  ports[N] req
    Valid: in Bool;
    ready: out Bool;
  end ports req
  port grant_valid: out Bool;
  port grant_requester: out UInt<2>;
end arbiter bus_arbiter
"#,
    );
    assert_eq!(
        warnings,
        vec![
            "arbiter `bus_arbiter` should be PascalCase (e.g. `BusArbiter`) — see naming conventions",
            "port `Valid` should be snake_case (e.g. `valid`) — see naming conventions",
        ]
    );
}

#[test]
fn bus_pascal_case_and_signal_snake_case() {
    let warnings = naming_warnings(
        r#"
bus axi_lite
  param ADDR_W: const = 32;
  AwValid: out Bool;
  aw_ready: in Bool;
end bus axi_lite
"#,
    );
    assert_eq!(
        warnings,
        vec![
            "bus `axi_lite` should be PascalCase (e.g. `AxiLite`) — see naming conventions",
            "port `AwValid` should be snake_case (e.g. `aw_valid`) — see naming conventions",
        ]
    );
}

#[test]
fn struct_and_enum_pascal_case() {
    assert_one_warning(
        r#"
struct my_pair
  a: UInt<8>;
  b: UInt<8>;
end struct my_pair
"#,
        "struct `my_pair` should be PascalCase (e.g. `MyPair`) — see naming conventions",
    );
    assert_one_warning(
        r#"
enum alu_op
  Add,
  Sub,
end enum alu_op
"#,
        "enum `alu_op` should be PascalCase (e.g. `AluOp`) — see naming conventions",
    );
}

#[test]
fn domain_pascal_case() {
    assert_one_warning(
        r#"
domain my_domain
  freq_mhz: 100
end domain my_domain
"#,
        "domain `my_domain` should be PascalCase (e.g. `MyDomain`) — see naming conventions",
    );
}

#[test]
fn template_name_unchecked_but_params_and_ports_checked() {
    // `template` isn't in issue #648's construct-name list (no documented
    // casing convention for it), so its own name must be silent — but its
    // params/ports are still real `param`/`port` declarations.
    let warnings = naming_warnings(
        r#"
template my_interface
  param num_req: const;
  port clk: in Clock<SysDomain>;
  port GrantValid: out Bool;
end template my_interface
"#,
    );
    assert_eq!(
        warnings,
        vec![
            "param `num_req` should be UPPER_SNAKE (e.g. `NUM_REQ`) — see naming conventions",
            "port `GrantValid` should be snake_case (e.g. `grant_valid`) — see naming conventions",
        ]
    );
}

// ── generate blocks (ports/wires declared inside `generate for`/`if`) ──────

#[test]
fn generate_for_port_and_wire_checked() {
    let warnings = naming_warnings(
        r#"
module M
  param N: const = 2;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;

  generate_for i in 0..N-1
    wire GenWire: Bool;
    comb
      GenWire = true;
    end comb
  end generate_for
end module M
"#,
    );
    assert_eq!(
        warnings,
        vec!["wire `GenWire` should be snake_case (e.g. `gen_wire`) — see naming conventions"]
    );
}

// ── `.archi` interface stubs are skipped entirely ──────────────────────────

#[test]
fn is_interface_stub_is_skipped() {
    let source = MODULE_TMPL.replace("{NAME}", "fetchUnit");
    let tokens = lexer::tokenize(&source).expect("lexer error");
    let mut parser = Parser::new(tokens, &source);
    let mut ast = parser.parse_source_file().expect("parse error");
    // Simulates the post-parse tagger main.rs applies to items loaded from
    // a `.archi` interface stub (port-only mirror of an already-linted
    // `.arch` file) — must not be double-reported.
    for item in &mut ast.items {
        item.set_is_interface(true);
    }
    let warnings = typecheck::check_naming(&ast);
    assert!(
        warnings.is_empty(),
        "is_interface item must be skipped entirely, got: {warnings:?}"
    );
}

// ── CLI-level: flag plumbing, opt-in guarantee, suppression pragma ─────────

mod cli {
    use std::process::Command;

    const BAD_MODULE: &str = r#"
module fetchUnit
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port ReqValid: in Bool;
  port dataOut: out UInt<8>;
  comb
    dataOut = ReqValid.zext<8>();
  end comb
end module fetchUnit
"#;

    fn run_check(path: &std::path::Path, extra: &[&str]) -> (i32, String) {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_arch"));
        cmd.arg("check").arg(path);
        for a in extra {
            cmd.arg(a);
        }
        let out = cmd.output().expect("run arch check");
        let mut combined = String::from_utf8_lossy(&out.stdout).into_owned();
        combined.push_str(&String::from_utf8_lossy(&out.stderr));
        (out.status.code().unwrap_or(-1), combined)
    }

    #[test]
    fn absent_flag_emits_zero_naming_warnings() {
        let td = tempfile::tempdir().expect("tempdir");
        let path = td.path().join("Bad.arch");
        std::fs::write(&path, BAD_MODULE).expect("write");
        let (code, out) = run_check(&path, &[]);
        assert_eq!(code, 0, "output: {out}");
        assert!(
            !out.contains("see naming conventions"),
            "flag absent must never emit naming diagnostics: {out}"
        );
    }

    #[test]
    fn bare_flag_and_explicit_warn_both_enable_the_lint() {
        let td = tempfile::tempdir().expect("tempdir");
        let path = td.path().join("Bad.arch");
        std::fs::write(&path, BAD_MODULE).expect("write");

        let (code_bare, out_bare) = run_check(&path, &["--lint-naming"]);
        assert_eq!(code_bare, 0, "output: {out_bare}");
        assert!(out_bare.contains(
            "module `fetchUnit` should be PascalCase (e.g. `FetchUnit`) — see naming conventions"
        ));
        assert!(out_bare.contains(
            "port `ReqValid` should be snake_case (e.g. `req_valid`) — see naming conventions"
        ));
        assert!(out_bare.contains(
            "port `dataOut` should be snake_case (e.g. `data_out`) — see naming conventions"
        ));

        let (code_warn, out_warn) = run_check(&path, &["--lint-naming=warn"]);
        assert_eq!(code_warn, 0, "output: {out_warn}");
        assert_eq!(out_bare, out_warn, "bare flag and `=warn` must agree");
    }

    #[test]
    fn explicit_off_is_silent() {
        let td = tempfile::tempdir().expect("tempdir");
        let path = td.path().join("Bad.arch");
        std::fs::write(&path, BAD_MODULE).expect("write");
        let (code, out) = run_check(&path, &["--lint-naming=off"]);
        assert_eq!(code, 0, "output: {out}");
        assert!(!out.contains("see naming conventions"));
    }

    #[test]
    fn error_severity_is_rejected_not_silently_accepted() {
        let td = tempfile::tempdir().expect("tempdir");
        let path = td.path().join("Bad.arch");
        std::fs::write(&path, BAD_MODULE).expect("write");
        let (code, out) = run_check(&path, &["--lint-naming=error"]);
        assert_ne!(code, 0, "output: {out}");
        assert!(
            out.contains("--lint-naming: expected `warn` or `off`"),
            "output: {out}"
        );
    }

    #[test]
    fn per_file_suppression_pragma_silences_the_lint() {
        let td = tempfile::tempdir().expect("tempdir");
        let path = td.path().join("Bad.arch");
        let source = format!("// arch-lint-naming: off\n{BAD_MODULE}");
        std::fs::write(&path, source).expect("write");
        let (code, out) = run_check(&path, &["--lint-naming=warn"]);
        assert_eq!(code, 0, "output: {out}");
        assert!(!out.contains("see naming conventions"), "output: {out}");
    }

    #[test]
    fn build_emission_is_byte_identical_regardless_of_flag_value() {
        let td = tempfile::tempdir().expect("tempdir");
        let arch_path = td.path().join("Bad.arch");
        std::fs::write(&arch_path, BAD_MODULE).expect("write");

        let run_build = |extra: &[&str]| -> Vec<u8> {
            let out_path = td.path().join(format!("out_{}.sv", extra.join("_")));
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_arch"));
            cmd.arg("build").arg(&arch_path).arg("-o").arg(&out_path);
            for a in extra {
                cmd.arg(a);
            }
            let status = cmd.status().expect("run arch build");
            assert!(status.success());
            std::fs::read(&out_path).expect("read sv output")
        };

        let no_flag = run_build(&[]);
        let warn = run_build(&["--lint-naming=warn"]);
        let off = run_build(&["--lint-naming=off"]);
        assert_eq!(
            no_flag, warn,
            "SV emission must not depend on --lint-naming"
        );
        assert_eq!(no_flag, off, "SV emission must not depend on --lint-naming");
    }
}
