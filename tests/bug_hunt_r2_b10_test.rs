//! Regression coverage for single-port write-first RAM emission.

use std::path::Path;
use std::process::Command;

fn run_arch_build(dir: &Path, source: &Path, output: &Path) {
    let result = Command::new(env!("CARGO_BIN_EXE_arch"))
        .current_dir(dir)
        .args([
            "build",
            source.to_str().expect("source path is UTF-8"),
            "-o",
            output.to_str().expect("output path is UTF-8"),
        ])
        .output()
        .expect("run arch build");
    assert!(
        result.status.success(),
        "arch build failed:\n{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

fn write_first_source(latency: u8) -> String {
    format!(
        "\
ram WriteFirstRam
  kind single;
  latency {latency};
  write: first;
  param DEPTH: const = 4;
  port clk: in Clock<SysDomain>;
  ports access
    en: in Bool;
    wen: in Bool;
    addr: in UInt<2>;
    wdata: in UInt<8>;
    rdata: out UInt<8>;
  end ports access
  init: zero;
end ram WriteFirstRam
"
    )
}

fn protocol_tb(latency: u8) -> String {
    let read_checks = "\
  access_wen = 0; access_wdata = 8'ha5; tick();
  if (access_rdata !== 8'h5a) $fatal(1, \"read-only access lost stored value\");
  if (dut.mem[0] !== 8'h5a) $fatal(1, \"read-only access mutated RAM\");
  access_wdata = 8'h3c; tick();
  if (access_rdata !== 8'h5a) $fatal(1, \"next read lost stored value\");
";
    format!(
        "\
module tb;
  logic clk = 0, access_en = 0, access_wen = 1;
  logic [1:0] access_addr = 0;
  logic [7:0] access_wdata = 8'h99;
  wire [7:0] access_rdata;
  WriteFirstRam dut(.*);
  task tick; #1 clk = 1; #1 clk = 0; endtask
  initial begin
    tick();
    if (dut.mem[0] !== 8'h00) $fatal(1, \"en=0 incorrectly wrote\");
    access_en = 1; access_wdata = 8'h5a; tick();
    if (dut.mem[0] !== 8'h5a) $fatal(1, \"enabled write failed\");
    if ({write_output_check})
      $fatal(1, \"write-first output mismatch\");
{read_checks}    access_en = 0; access_wen = 1; access_wdata = 8'hee; tick();
    if (dut.mem[0] !== 8'h5a) $fatal(1, \"disabled enable mutated RAM\");
    $finish;
  end
endmodule
",
        write_output_check = if latency == 1 {
            "access_rdata !== 8'h5a"
        } else {
            // latency 2 exposes the write-first result one cycle later.
            "access_rdata === 8'h5a && 1'b0"
        },
        read_checks = read_checks
    )
}

#[test]
fn single_write_first_emits_runtime_wen_and_forwarding() {
    let td = tempfile::tempdir().expect("tempdir");
    for latency in [1_u8, 2_u8] {
        let source = td.path().join(format!("write_first_{latency}.arch"));
        let output = td.path().join(format!("write_first_{latency}.sv"));
        std::fs::write(&source, write_first_source(latency)).expect("write ARCH source");
        run_arch_build(td.path(), &source, &output);
        let sv = std::fs::read_to_string(&output).expect("read generated SV");
        assert!(
            sv.contains("if (access_wen) begin"),
            "missing runtime wen guard:\n{sv}"
        );
        assert!(
            sv.contains("access_rdata_r <= access_wdata;"),
            "missing write forwarding:\n{sv}"
        );
        assert!(
            sv.contains("access_rdata_r <= mem[access_addr];"),
            "missing stored read:\n{sv}"
        );
        assert!(
            sv.contains("if (access_en) begin"),
            "missing enable guard:\n{sv}"
        );

        if Command::new("iverilog").arg("-V").output().is_err() {
            continue;
        }
        let tb = td.path().join(format!("write_first_{latency}_tb.sv"));
        let vvp = td.path().join(format!("write_first_{latency}.vvp"));
        std::fs::write(&tb, protocol_tb(latency)).expect("write protocol TB");
        let compile = Command::new("iverilog")
            .args(["-g2012", "-s", "tb", "-o"])
            .arg(&vvp)
            .arg(&output)
            .arg(&tb)
            .output()
            .expect("run iverilog");
        assert!(
            compile.status.success(),
            "iverilog failed:\n{}",
            String::from_utf8_lossy(&compile.stderr)
        );
        let run = Command::new("vvp").arg(&vvp).output().expect("run vvp");
        assert!(
            run.status.success(),
            "write-first protocol failed:\n{}",
            String::from_utf8_lossy(&run.stderr)
        );
    }

    let source = td.path().join("write_first_without_wen.arch");
    let output = td.path().join("write_first_without_wen.sv");
    std::fs::write(
        &source,
        "\
ram ReadOnly
  kind single;
  latency 1;
  write: first;
  param DEPTH: const = 4;
  port clk: in Clock<SysDomain>;
  ports access
    en: in Bool;
    addr: in UInt<2>;
    rdata: out UInt<8>;
  end ports access
  init: zero;
end ram ReadOnly
",
    )
    .expect("write no-wen ARCH source");
    run_arch_build(td.path(), &source, &output);
    let sv = std::fs::read_to_string(&output).expect("read no-wen generated SV");
    assert!(
        !sv.contains("access_wdata") && !sv.contains("access_wen"),
        "no-wen read-only RAM must not reference absent write signals:\n{sv}"
    );
    assert!(
        sv.contains("access_rdata_r <= mem[access_addr]"),
        "no-wen write-first RAM must preserve registered reads:\n{sv}"
    );
}
