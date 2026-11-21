use arch::ast::RamWriteMode;
use arch::elaborate;
use arch::lexer;
use arch::parser::Parser;
use arch::resolve;
use arch::sim_codegen::SimCodegen;
use std::collections::HashMap;

#[test]
fn test_ram_read_first_sim_codegen_987() {
    let source = "
domain SysDomain
  freq_mhz: 100
end domain SysDomain

ram RfRam
  kind single;
  latency 1;
  write: read_first;

  param DEPTH: const = 8;
  param WIDTH: const = 8;
  param T: type = UInt<8>;
  port clk: in Clock<SysDomain>;
  ports p
    addr: in UInt<3>; en: in Bool; wen: in Bool;
    wdata: in UInt<8>; data: out UInt<8>;
  end ports p
end ram RfRam
";
    let tokens = lexer::tokenize(source).expect("tokenize");
    let mut parser = Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let ast = elaborate::elaborate(ast).expect("elaborate");
    let symbols = resolve::resolve(&ast).expect("resolve");

    let ram = match &ast.items[1] {
        arch::ast::Item::Ram(r) => r,
        _ => panic!("expected ram"),
    };
    assert_eq!(ram.write_mode, Some(RamWriteMode::ReadFirst));

    let sim_cg = SimCodegen::new(&symbols, &ast, HashMap::new());
    let models = sim_cg.generate();
    let model = models
        .iter()
        .find(|m| m.class_name == "VRfRam")
        .expect("expected VRfRam model");

    // Under read_first, read must happen on write cycles too (not guarded by !p_wen)
    assert!(
        !model.impl_.contains("if (!p_wen)"),
        "read_first must not suppress read when p_wen is asserted, got:\n{}",
        model.impl_
    );
    assert!(
        model.impl_.contains("_r_p_data = _mem[p_addr];"),
        "expected read_first to sample old mem value into read reg:\n{}",
        model.impl_
    );
}

#[test]
fn test_ram_write_first_sim_codegen_987() {
    let source = "
domain SysDomain
  freq_mhz: 100
end domain SysDomain

ram WfRam
  kind single;
  latency 1;
  write: first;

  param DEPTH: const = 8;
  param WIDTH: const = 8;
  param T: type = UInt<8>;
  port clk: in Clock<SysDomain>;
  ports p
    addr: in UInt<3>; en: in Bool; wen: in Bool;
    wdata: in UInt<8>; data: out UInt<8>;
  end ports p
end ram WfRam
";
    let tokens = lexer::tokenize(source).expect("tokenize");
    let mut parser = Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let ast = elaborate::elaborate(ast).expect("elaborate");
    let symbols = resolve::resolve(&ast).expect("resolve");

    let sim_cg = SimCodegen::new(&symbols, &ast, HashMap::new());
    let models = sim_cg.generate();
    let model = models
        .iter()
        .find(|m| m.class_name == "VWfRam")
        .expect("expected VWfRam model");

    assert!(
        model.impl_.contains("_r_p_data = p_wdata;"),
        "expected write_first to forward write data to read register on write cycle:\n{}",
        model.impl_
    );
}

#[test]
fn test_ram_no_change_sim_codegen_987() {
    let source = "
domain SysDomain
  freq_mhz: 100
end domain SysDomain

ram NcRam
  kind single;
  latency 1;
  write: no_change;

  param DEPTH: const = 8;
  param WIDTH: const = 8;
  param T: type = UInt<8>;
  port clk: in Clock<SysDomain>;
  ports p
    addr: in UInt<3>; en: in Bool; wen: in Bool;
    wdata: in UInt<8>; data: out UInt<8>;
  end ports p
end ram NcRam
";
    let tokens = lexer::tokenize(source).expect("tokenize");
    let mut parser = Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let ast = elaborate::elaborate(ast).expect("elaborate");
    let symbols = resolve::resolve(&ast).expect("resolve");

    let sim_cg = SimCodegen::new(&symbols, &ast, HashMap::new());
    let models = sim_cg.generate();
    let model = models
        .iter()
        .find(|m| m.class_name == "VNcRam")
        .expect("expected VNcRam model");

    assert!(
        model.impl_.contains("if (!p_wen)"),
        "no_change must guard read with !wen:\n{}",
        model.impl_
    );
}

#[test]
fn test_ram_true_dual_sv_and_sim_write_mode_parity_987() {
    let source = "
domain SysDomain
  freq_mhz: 100
end domain SysDomain

ram TrueDualWf
  kind true_dual;
  latency 1;
  write: first;

  param DEPTH: const = 8;
  param WIDTH: const = 8;
  param T: type = UInt<8>;
  port clk: in Clock<SysDomain>;
  ports a
    addr: in UInt<3>; en: in Bool; wen: in Bool;
    wdata: in UInt<8>; rdata: out UInt<8>;
  end ports a
  ports b
    addr: in UInt<3>; en: in Bool; wen: in Bool;
    wdata: in UInt<8>; rdata: out UInt<8>;
  end ports b
end ram TrueDualWf
";
    let tokens = lexer::tokenize(source).expect("tokenize");
    let mut parser = Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let ast = elaborate::elaborate(ast).expect("elaborate");
    let symbols = resolve::resolve(&ast).expect("resolve");

    // SV codegen must emit write_first forwarding for true_dual
    let mut cg = arch::codegen::Codegen::new(&symbols, &ast, HashMap::new());
    let sv = cg.generate();
    assert!(
        sv.contains("a_rdata_r <= a_wdata;"),
        "SV codegen must forward write data on port a in write_first mode:\n{sv}"
    );
    assert!(
        sv.contains("b_rdata_r <= b_wdata;"),
        "SV codegen must forward write data on port b in write_first mode:\n{sv}"
    );

    // Sim codegen must also forward write data for true_dual
    let sim_cg = SimCodegen::new(&symbols, &ast, HashMap::new());
    let models = sim_cg.generate();
    let model = models
        .iter()
        .find(|m| m.class_name == "VTrueDualWf")
        .expect("expected VTrueDualWf model");
    assert!(
        model.impl_.contains("_r_a_rdata = a_wdata;"),
        "sim must forward write data on port a:\n{}",
        model.impl_
    );
    assert!(
        model.impl_.contains("_r_b_rdata = b_wdata;"),
        "sim must forward write data on port b:\n{}",
        model.impl_
    );
}
