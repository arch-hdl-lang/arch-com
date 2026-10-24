//! Regression for composite access recovery of storage-only narrow floats.

use std::fs;
use std::process::Command;

#[test]
fn bug_hunt_r2_b08_narrow_composite_conversion() {
    let td = tempfile::tempdir().expect("tempdir");
    let arch_path = td.path().join("VerifyNarrow.arch");
    let sv_path = td.path().join("VerifyNarrow.sv");
    let vvp_path = td.path().join("VerifyNarrow.vvp");
    fs::write(
        &arch_path,
        r#"struct Narrow
  f4: FP4E2M1;
  f62: FP6E2M3;
  f63: FP6E3M2;
end struct Narrow
module VerifyNarrow
  port f4_scalar: in FP4E2M1;
  port f4_vec: in Vec<FP4E2M1, 2>;
  port f4_ys: out FP32;
  port f4_yv: out FP32;
  port f4_yf: out FP32;
  port f62_scalar: in FP6E2M3;
  port f62_vec: in Vec<FP6E2M3, 2>;
  port f62_ys: out FP32;
  port f62_yv: out FP32;
  port f62_yf: out FP32;
  port f63_scalar: in FP6E3M2;
  port f63_vec: in Vec<FP6E3M2, 2>;
  port f63_ys: out FP32;
  port f63_yv: out FP32;
  port f63_yf: out FP32;
  wire fields: Narrow;
  comb
    fields.f4 = f4_scalar;
    f4_ys = f4_scalar.to_fp32();
    f4_yv = f4_vec[0].to_fp32();
    f4_yf = fields.f4.to_fp32();
    fields.f62 = f62_scalar;
    f62_ys = f62_scalar.to_fp32();
    f62_yv = f62_vec[0].to_fp32();
    f62_yf = fields.f62.to_fp32();
    fields.f63 = f63_scalar;
    f63_ys = f63_scalar.to_fp32();
    f63_yv = f63_vec[0].to_fp32();
    f63_yf = fields.f63.to_fp32();
  end comb
end module VerifyNarrow
"#,
    )
    .unwrap();

    let arch = env!("CARGO_BIN_EXE_arch");
    let check = Command::new(arch)
        .arg("check")
        .arg(&arch_path)
        .output()
        .expect("run arch check");
    assert!(
        check.status.success(),
        "arch check failed: {}",
        String::from_utf8_lossy(&check.stderr)
    );

    let build = Command::new(arch)
        .arg("build")
        .arg(&arch_path)
        .arg("-o")
        .arg(&sv_path)
        .output()
        .expect("run arch build");
    assert!(
        build.status.success(),
        "arch build failed: {}",
        String::from_utf8_lossy(&build.stderr)
    );
    let sv = fs::read_to_string(&sv_path).unwrap();
    for needle in [
        "arch_e2m1_to_f32(f4_vec[0])",
        "arch_e2m1_to_f32(fields.f4)",
        "arch_e2m3_to_f32(f62_vec[0])",
        "arch_e2m3_to_f32(fields.f62)",
        "arch_e3m2_to_f32(f63_vec[0])",
        "arch_e3m2_to_f32(fields.f63)",
    ] {
        assert!(
            sv.contains(needle),
            "missing composite dispatch {needle} in SV"
        );
    }

    let tb_path = td.path().join("tb.sv");
    fs::write(
        &tb_path,
        r#"module tb;
reg [3:0] f4_scalar; reg [1:0][3:0] f4_vec; wire [31:0] f4_ys, f4_yv, f4_yf;
reg [5:0] f62_scalar; reg [1:0][5:0] f62_vec; wire [31:0] f62_ys, f62_yv, f62_yf;
reg [5:0] f63_scalar; reg [1:0][5:0] f63_vec; wire [31:0] f63_ys, f63_yv, f63_yf;
VerifyNarrow dut(.f4_scalar(f4_scalar),.f4_vec(f4_vec),.f4_ys(f4_ys),.f4_yv(f4_yv),.f4_yf(f4_yf),.f62_scalar(f62_scalar),.f62_vec(f62_vec),.f62_ys(f62_ys),.f62_yv(f62_yv),.f62_yf(f62_yf),.f63_scalar(f63_scalar),.f63_vec(f63_vec),.f63_ys(f63_ys),.f63_yv(f63_yv),.f63_yf(f63_yf));
initial begin
  f4_scalar=4'd2; f4_vec=0; f4_vec[0]=4'd2;
  f62_scalar=6'd8; f62_vec=0; f62_vec[0]=6'd8;
  f63_scalar=6'd12; f63_vec=0; f63_vec[0]=6'd12;
  #1;
  if (f4_ys !== 32'h3f800000 || f4_yv !== 32'h3f800000 || f4_yf !== 32'h3f800000) $fatal(1,"FP4 conversion failed");
  if (f62_ys !== 32'h3f800000 || f62_yv !== 32'h3f800000 || f62_yf !== 32'h3f800000) $fatal(1,"FP6 E2M3 conversion failed");
  if (f63_ys !== 32'h3f800000 || f63_yv !== 32'h3f800000 || f63_yf !== 32'h3f800000) $fatal(1,"FP6 E3M2 conversion failed");
  $finish;
end
endmodule
"#,
    )
    .unwrap();
    let iverilog = match Command::new("iverilog")
        .args(["-g2012", "-s", "tb", "-o"])
        .arg(&vvp_path)
        .arg(&sv_path)
        .arg(&tb_path)
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => panic!("run iverilog: {error}"),
    };
    assert!(
        iverilog.status.success(),
        "iverilog failed: {}",
        String::from_utf8_lossy(&iverilog.stderr)
    );
    let sim = match Command::new("vvp").arg(&vvp_path).output() {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => panic!("run vvp: {error}"),
    };
    assert!(
        sim.status.success(),
        "vvp failed: {}",
        String::from_utf8_lossy(&sim.stderr)
    );
}
