// E203 Clock Control
// Generates gated clocks for core subsystems and TCMs.
// Port list matches RealBench testbench.
module e203_clk_ctrl (
  input logic clk,
  input logic rst_n,
  input logic test_mode,
  input logic core_cgstop,
  input logic core_ifu_active,
  input logic core_exu_active,
  input logic core_lsu_active,
  input logic core_biu_active,
  input logic itcm_active,
  input logic dtcm_active,
  input logic core_wfi,
  output logic clk_core_ifu,
  output logic clk_core_exu,
  output logic clk_core_lsu,
  output logic clk_core_biu,
  output logic clk_itcm,
  output logic clk_dtcm,
  output logic clk_aon,
  output logic itcm_ls,
  output logic dtcm_ls
);

  // Subsystem active flags (inputs — used for clock gating decisions)
  // Gated clocks out
  // TCM light-sleep outputs
  // Gate enable: active & not stopped
  logic ifu_gate_en;
  assign ifu_gate_en = core_ifu_active & ~core_cgstop;
  logic exu_gate_en;
  assign exu_gate_en = core_exu_active & ~core_cgstop;
  logic lsu_gate_en;
  assign lsu_gate_en = core_lsu_active & ~core_cgstop;
  logic biu_gate_en;
  assign biu_gate_en = core_biu_active & ~core_cgstop;
  logic itcm_gate_en;
  assign itcm_gate_en = itcm_active & ~core_cgstop;
  logic dtcm_gate_en;
  assign dtcm_gate_en = dtcm_active & ~core_cgstop;
  // ICG instances
  logic clk_ifu_w;
  e203_clkgate icg_ifu (
    .clk_in(clk),
    .clock_en(ifu_gate_en),
    .test_mode(test_mode),
    .clk_out(clk_ifu_w)
  );
  logic clk_exu_w;
  e203_clkgate icg_exu (
    .clk_in(clk),
    .clock_en(exu_gate_en),
    .test_mode(test_mode),
    .clk_out(clk_exu_w)
  );
  logic clk_lsu_w;
  e203_clkgate icg_lsu (
    .clk_in(clk),
    .clock_en(lsu_gate_en),
    .test_mode(test_mode),
    .clk_out(clk_lsu_w)
  );
  logic clk_biu_w;
  e203_clkgate icg_biu (
    .clk_in(clk),
    .clock_en(biu_gate_en),
    .test_mode(test_mode),
    .clk_out(clk_biu_w)
  );
  logic clk_itcm_w;
  e203_clkgate icg_itcm (
    .clk_in(clk),
    .clock_en(itcm_gate_en),
    .test_mode(test_mode),
    .clk_out(clk_itcm_w)
  );
  logic clk_dtcm_w;
  e203_clkgate icg_dtcm (
    .clk_in(clk),
    .clock_en(dtcm_gate_en),
    .test_mode(test_mode),
    .clk_out(clk_dtcm_w)
  );
  assign clk_core_ifu = clk_ifu_w;
  assign clk_core_exu = clk_exu_w;
  assign clk_core_lsu = clk_lsu_w;
  assign clk_core_biu = clk_biu_w;
  assign clk_itcm = clk_itcm_w;
  assign clk_dtcm = clk_dtcm_w;
  assign clk_aon = clk;
  assign itcm_ls = ~itcm_active & core_wfi;
  assign dtcm_ls = ~dtcm_active & core_wfi;

endmodule

// Always-on = ungated
// TCM light-sleep: enter when inactive and WFI
