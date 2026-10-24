//! Regression coverage for runtime unary negation of packed arithmetic floats.

use std::process::Command;

fn arch() -> Command {
    Command::new(env!("CARGO_BIN_EXE_arch"))
}

#[test]
fn runtime_float_negation_flips_only_the_sign_bit() {
    let td = tempfile::tempdir().expect("tempdir");
    let arch_path = td.path().join("Neg.arch");
    let sv_path = td.path().join("Neg.sv");
    let vvp_path = td.path().join("Neg.vvp");
    let tb_path = td.path().join("tb.sv");
    std::fs::write(
        &arch_path,
        r#"module Neg
  port f: in FP32;
  port hf: in BF16;
  port e4: in FP8E4M3;
  port e5: in FP8E5M2;
  port y: out FP32;
  port hy: out BF16;
  port y4: out FP8E4M3;
  port y5: out FP8E5M2;
  comb
    y = -f;
    hy = -hf;
    y4 = -e4;
    y5 = -e5;
  end comb
end module Neg
"#,
    )
    .expect("write ARCH fixture");
    std::fs::write(
        &tb_path,
        r#"module tb;
  reg [31:0] f;
  reg [15:0] hf;
  reg [7:0] e4, e5;
  wire [31:0] y;
  wire [15:0] hy;
  wire [7:0] y4, y5;
  Neg dut(.f(f), .hf(hf), .e4(e4), .e5(e5), .y(y), .hy(hy), .y4(y4), .y5(y5));
  task check;
    input [31:0] f_i, y_i;
    input [15:0] hf_i, hy_i;
    input [7:0] e4_i, y4_i, e5_i, y5_i;
    begin
      f=f_i; hf=hf_i; e4=e4_i; e5=e5_i; #1;
      if (y !== y_i || hy !== hy_i || y4 !== y4_i || y5 !== y5_i) begin
        $display("FAIL f=%h y=%h/%h hf=%h hy=%h/%h e4=%h y4=%h/%h e5=%h y5=%h/%h",
                 f, y, y_i, hf, hy, hy_i, e4, y4, y4_i, e5, y5, y5_i);
        $fatal;
      end
    end
  endtask
  initial begin
    check(32'h3fc00000, 32'hbfc00000, 16'h3fc0, 16'hbfc0,
          8'h3c, 8'hbc, 8'h3e, 8'hbe);
    check(32'hbfc00000, 32'h3fc00000, 16'hbfc0, 16'h3fc0,
          8'hbc, 8'h3c, 8'hbe, 8'h3e);
    check(32'h00000000, 32'h80000000, 16'h0000, 16'h8000,
          8'h00, 8'h80, 8'h00, 8'h80);
    check(32'h80000000, 32'h00000000, 16'h8000, 16'h0000,
          8'h80, 8'h00, 8'h80, 8'h00);
    $display("B07 unary float negation: PASS");
    $finish;
  end
endmodule
"#,
    )
    .expect("write SV testbench");

    let build = arch()
        .args(["build"])
        .arg(&arch_path)
        .args(["-o"])
        .arg(&sv_path)
        .output()
        .expect("run arch build");
    assert!(
        build.status.success(),
        "arch build failed:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let sv = std::fs::read_to_string(&sv_path).expect("read generated SV");
    assert!(
        sv.contains("f ^ 32'h80000000"),
        "FP32 sign flip missing:\n{sv}"
    );
    assert!(
        sv.contains("hf ^ 16'h8000"),
        "BF16 sign flip missing:\n{sv}"
    );
    assert!(
        sv.contains("e4 ^ 8'h80"),
        "FP8 E4M3 sign flip missing:\n{sv}"
    );
    assert!(
        sv.contains("e5 ^ 8'h80"),
        "FP8 E5M2 sign flip missing:\n{sv}"
    );

    if Command::new("iverilog").arg("-V").output().is_err()
        || Command::new("vvp").arg("-V").output().is_err()
    {
        eprintln!("iverilog/vvp unavailable; skipping executable SV oracle");
        return;
    }

    let compile = Command::new("iverilog")
        .args(["-g2012", "-s", "tb", "-o"])
        .arg(&vvp_path)
        .arg(&sv_path)
        .arg(&tb_path)
        .output()
        .expect("run iverilog");
    assert!(
        compile.status.success(),
        "iverilog failed:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let run = Command::new("vvp")
        .arg(&vvp_path)
        .output()
        .expect("run vvp");
    assert!(
        run.status.success(),
        "vvp failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(String::from_utf8_lossy(&run.stdout).contains("B07 unary float negation: PASS"));
}
