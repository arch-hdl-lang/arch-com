use std::fs;
use std::process::Command;

const SOURCE: &str = r#"
domain Src
  freq_mhz: 100
end domain Src
domain Dst
  freq_mhz: 50
end domain Dst
synchronizer ConfigSync
  kind handshake;
  param STAGES: const = 2;
  port src_clk: in Clock<Src>;
  port dst_clk: in Clock<Dst>;
  port rst: in Reset<Sync>;
  port data_in: in UInt<8>;
  port data_out: out UInt<8>;
end synchronizer ConfigSync
"#;

const SV_TB: &str = r#"
module tb;
  logic src_clk = 0, dst_clk = 0, rst = 1;
  logic [7:0] data_in = 0;
  wire [7:0] data_out;
  ConfigSync dut(.*);
  task src; begin #1 src_clk = 1; #1 src_clk = 0; end endtask
  task dst; begin #1 dst_clk = 1; #1 dst_clk = 0; end endtask
  task check_value; input [7:0] value; begin
    if (data_out !== value) $fatal(1, "expected data_out=%h, got %h", value, data_out);
  end endtask
  initial begin
    src(); dst(); rst = 0;
    data_in = 8'h96; src(); check_value(0); // source capture must not publish
    dst(); check_value(0);                  // request is only in sync stage 0
    dst(); check_value(0);                  // request is only in sync stage 1
    dst(); check_value(8'h96);              // destination captures the stable bus
    data_in = 8'ha5; src(); check_value(8'h96); // source is busy until ack returns
    dst(); dst(); dst(); src(); src(); src();
    dst(); dst(); dst(); check_value(8'ha5);
    rst = 1; dst(); src(); check_value(0);  // both destination output and source state reset
    rst = 0; data_in = 8'h3c; src(); dst(); dst(); dst(); check_value(8'h3c);
    rst = 1; dst(); src(); rst = 0; data_in = 8'h5a;
    src(); dst(); check_value(0); src(); dst(); check_value(0);
    src(); dst(); check_value(0); src(); dst(); check_value(8'h5a);
    $finish;
  end
endmodule
"#;

const CPP_TB: &str = r#"
#include "ConfigSync.h"
#include <cstdint>
#include <cstdio>
#include <cstdlib>

static ConfigSync dut;
static void eval() { dut.eval(); }
static void src_tick() { dut.src_clk = 0; eval(); dut.src_clk = 1; eval(); dut.src_clk = 0; eval(); }
static void dst_tick() { dut.dst_clk = 0; eval(); dut.dst_clk = 1; eval(); dut.dst_clk = 0; eval(); }
static void both_tick() { dut.src_clk = 0; dut.dst_clk = 0; eval(); dut.src_clk = 1; dut.dst_clk = 1; eval(); dut.src_clk = 0; dut.dst_clk = 0; eval(); }
static void expect(unsigned value) {
  if (dut.data_out != value) { std::fprintf(stderr, "expected %u got %u\n", value, (unsigned)dut.data_out); std::abort(); }
}
int main() {
  dut.rst = 1; dut.data_in = 0; dut.src_clk = 0; dut.dst_clk = 0; eval();
  src_tick(); dst_tick(); dut.rst = 0;
  dut.data_in = 0x96; src_tick(); expect(0);
  dst_tick(); expect(0); dst_tick(); expect(0); dst_tick(); expect(0x96);
  dut.data_in = 0xa5; src_tick(); expect(0x96);
  dst_tick(); dst_tick(); dst_tick(); src_tick(); src_tick(); src_tick();
  dst_tick(); dst_tick(); dst_tick(); expect(0xa5);
  dut.rst = 1; dst_tick(); src_tick(); expect(0);
  dut.rst = 0; dut.data_in = 0x3c; src_tick(); dst_tick(); dst_tick(); dst_tick(); expect(0x3c);
  dut.rst = 1; dst_tick(); src_tick(); dut.rst = 0; dut.data_in = 0x5a;
  both_tick(); expect(0); both_tick(); expect(0); both_tick(); expect(0); both_tick(); expect(0x5a);
  return 0;
}
"#;

fn available(tool: &str) -> bool {
    Command::new(tool)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
fn handshake_destination_capture_native_and_sv() {
    let td = tempfile::tempdir().expect("tempdir");
    let src = td.path().join("ConfigSync.arch");
    fs::write(&src, SOURCE).expect("write source");

    if available("iverilog") && available("vvp") {
        let build = Command::new(env!("CARGO_BIN_EXE_arch"))
            .args(["build"])
            .arg(&src)
            .arg("--o")
            .arg(td.path().join("ConfigSync.sv"))
            .output()
            .expect("run arch build");
        assert!(
            build.status.success(),
            "arch build failed: {}",
            String::from_utf8_lossy(&build.stderr)
        );
        let sv_tb = td.path().join("tb.sv");
        fs::write(&sv_tb, SV_TB).expect("write SV testbench");
        let vvp = td.path().join("sim.vvp");
        let compile = Command::new("iverilog")
            .args(["-g2012", "-s", "tb", "-o"])
            .arg(&vvp)
            .arg(td.path().join("ConfigSync.sv"))
            .arg(&sv_tb)
            .output()
            .expect("run iverilog");
        assert!(
            compile.status.success(),
            "iverilog failed: {}",
            String::from_utf8_lossy(&compile.stderr)
        );
        let run = Command::new("vvp").arg(&vvp).output().expect("run vvp");
        assert!(
            run.status.success(),
            "SV handshake regression failed: {}",
            String::from_utf8_lossy(&run.stderr)
        );
    }

    let cpp_tb = td.path().join("tb.cpp");
    fs::write(&cpp_tb, CPP_TB).expect("write C++ testbench");
    let native = Command::new(env!("CARGO_BIN_EXE_arch"))
        .args(["sim"])
        .arg(&src)
        .arg("--tb")
        .arg(&cpp_tb)
        .arg("--outdir")
        .arg(td.path().join("native"))
        .output()
        .expect("run native arch sim");
    assert!(
        native.status.success(),
        "native handshake regression failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&native.stdout),
        String::from_utf8_lossy(&native.stderr)
    );
}
