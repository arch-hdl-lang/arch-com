use arch::codegen::Codegen;
use arch::elaborate;
use arch::lexer;
use arch::parser::Parser;
use arch::resolve;
use arch::typecheck::TypeChecker;
use std::fs;
use std::process::Command;

fn compile_to_sv(source: &str) -> String {
    let tokens = lexer::tokenize(source).expect("lexer error");
    let mut parser = Parser::new(tokens, source);
    let parsed = parser.parse_source_file().expect("parse error");
    let ast = elaborate::elaborate(parsed).expect("elaborate error");
    let ast = elaborate::expand_auto_connect(ast, false)
        .expect("auto-connect error")
        .0;
    let ast = elaborate::lower_tlm_target_threads(ast).expect("target lowering error");
    let ast = elaborate::lower_tlm_initiator_calls(ast).expect("initiator lowering error");
    let ast = elaborate::lower_threads(ast).expect("thread lowering error");
    let ast = elaborate::lower_pipe_reg_ports(ast)
        .expect("pipe-reg lowering error")
        .0;
    let ast = elaborate::lower_credit_channel_dispatch(ast).expect("credit-channel lowering error");
    let symbols = resolve::resolve(&ast).expect("resolve error");
    let checker = TypeChecker::new(&symbols, &ast);
    let (_warnings, overload_map) = checker.check().expect("type check error");
    Codegen::new(&symbols, &ast, overload_map).generate()
}

#[test]
fn counter_codegen_uses_max_parameter_for_all_bound_behaviors() {
    let sv = compile_to_sv(
        r#"
        counter UpWrap
          kind wrap;
          direction: up;
          param MAX: const = 5;
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Async, Low>;
          port inc: in Bool;
          port value: out UInt<4>;
          port at_max: out Bool;
        end counter UpWrap

        counter DownWrap
          kind wrap;
          direction: down;
          param MAX: const = 5;
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Async, Low>;
          port dec: in Bool;
          port value: out UInt<4>;
        end counter DownWrap

        counter UpSat
          kind saturate;
          direction: up;
          param MAX: const = 5;
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Async, Low>;
          port inc: in Bool;
          port value: out UInt<4>;
        end counter UpSat
        "#,
    );

    assert!(
        sv.contains("if (count_r == MAX) count_r <= '0;"),
        "up wrap ignored MAX:\n{sv}"
    );
    assert!(
        sv.contains("if (count_r == '0) count_r <= MAX;"),
        "down wrap ignored MAX:\n{sv}"
    );
    assert!(
        sv.contains("if (count_r < MAX) count_r <= count_r + 1;"),
        "up saturate ignored MAX:\n{sv}"
    );
    assert!(
        sv.contains("assign at_max = (count_r == MAX);"),
        "at_max ignored MAX:\n{sv}"
    );
    assert!(
        sv.contains("count_r <= MAX)"),
        "range SVA ignored MAX:\n{sv}"
    );
}

#[test]
fn counter_codegen_prefers_runtime_max_over_parameter_and_defaults_to_width() {
    let sv = compile_to_sv(
        r#"
        counter RuntimeMax
          kind saturate;
          direction: up;
          param MAX: const = 5;
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Async, Low>;
          port inc: in Bool;
          port max: in UInt<4>;
          port value: out UInt<4>;
          port at_max: out Bool;
        end counter RuntimeMax

        counter NaturalMax
          kind wrap;
          direction: up;
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Async, Low>;
          port inc: in Bool;
          port value: out UInt<4>;
          port at_max: out Bool;
        end counter NaturalMax
        "#,
    );

    assert!(
        sv.contains("if (count_r < max) count_r <= count_r + 1;"),
        "runtime max did not win:\n{sv}"
    );
    assert!(
        sv.contains("assign at_max = (count_r == max);"),
        "runtime at_max did not win:\n{sv}"
    );
    assert!(
        sv.contains("if (count_r == '1) count_r <= '0;"),
        "natural width max missing:\n{sv}"
    );
    assert!(
        sv.contains("assign at_max = (count_r == '1);"),
        "natural width at_max missing:\n{sv}"
    );
}

#[test]
fn counter_codegen_default_and_parameter_bounds_execute_in_iverilog() {
    if Command::new("iverilog").arg("-V").output().is_err() {
        eprintln!("skipping counter Icarus regression: iverilog not found");
        return;
    }
    let source = r#"
        counter DefaultUp
          kind wrap;
          direction: up;
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync, Low>;
          port inc: in Bool;
          port value: out UInt<4>;
          port at_max: out Bool;
        end counter DefaultUp
        counter DefaultDown
          kind wrap;
          direction: down;
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync, Low>;
          port dec: in Bool;
          port value: out UInt<4>;
        end counter DefaultDown
        counter ParamSat
          kind saturate;
          direction: up;
          param MAX: const = 5;
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync, Low>;
          port inc: in Bool;
          port value: out UInt<4>;
        end counter ParamSat
        counter RuntimeSat
          kind saturate;
          direction: up;
          param MAX: const = 5;
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync, Low>;
          port inc: in Bool;
          port max: in UInt<4>;
          port value: out UInt<4>;
        end counter RuntimeSat
    "#;
    let sv = compile_to_sv(source);
    let tb = r#"
        module tb;
          logic clk = 0, rst = 0, inc = 0, dec = 0;
          logic [3:0] up, down, ps, rs;
          logic up_at_max;
          DefaultUp u(clk, rst, inc, up, up_at_max);
          DefaultDown d(clk, rst, dec, down);
          ParamSat p(clk, rst, inc, ps);
          RuntimeSat r(clk, rst, inc, 4'd3, rs);
          always #5 clk = ~clk;
          task tick;
            begin @(posedge clk); #1; end
          endtask
          initial begin
            repeat (2) tick;
            rst = 1;
            inc = 1;
            dec = 1;
            tick;
            if (down !== 4'd15) $fatal(1, "default down did not wrap to all ones");
            repeat (14) tick;
            if (up !== 4'd15 || !up_at_max) $fatal(1, "default up did not reach all ones");
            tick;
            if (up !== 4'd0 || up_at_max) $fatal(1, "default up did not wrap");
            repeat (5) tick;
            if (ps !== 4'd5) $fatal(1, "MAX parameter saturation ignored");
            if (rs !== 4'd3) $fatal(1, "runtime max precedence ignored");
            $finish;
          end
        endmodule
    "#;
    let dir = tempfile::tempdir().expect("tempdir");
    let sv_path = dir.path().join("counter.sv");
    let tb_path = dir.path().join("tb.sv");
    let exe_path = dir.path().join("sim");
    fs::write(&sv_path, sv).expect("write generated SV");
    fs::write(&tb_path, tb).expect("write testbench");
    let build = Command::new("iverilog")
        .args(["-g2012", "-gno-assertions", "-o"])
        .arg(&exe_path)
        .arg(&sv_path)
        .arg(&tb_path)
        .output()
        .expect("run iverilog");
    assert!(
        build.status.success(),
        "iverilog failed:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new("vvp")
        .arg(&exe_path)
        .output()
        .expect("run vvp");
    assert!(
        run.status.success(),
        "counter simulation failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
}
