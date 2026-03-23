// E203 Clock Control
// Generates gated clocks for core subsystems: IFU, EXU, LSU, BIU
// Each sub-block clock can be independently gated for power savings.
module ClkCtrl (
  input logic clk,
  input logic rst_n,
  input logic test_en,
  input logic ifu_gate_en,
  input logic exu_gate_en,
  input logic lsu_gate_en,
  input logic biu_gate_en,
  output logic clk_ifu,
  output logic clk_exu,
  output logic clk_lsu,
  output logic clk_biu,
  output logic ifu_clk_active,
  output logic exu_clk_active,
  output logic lsu_clk_active,
  output logic biu_clk_active
);

  // Gate enables from CSR/power management
  // Gated clocks out
  // Status: whether each clock is active
  // ICG instances
  E203ClkGate icg_ifu (
    .clk_in(clk),
    .enable(ifu_gate_en),
    .test_en(test_en),
    .clk_out(clk_ifu_w)
  );
  E203ClkGate icg_exu (
    .clk_in(clk),
    .enable(exu_gate_en),
    .test_en(test_en),
    .clk_out(clk_exu_w)
  );
  E203ClkGate icg_lsu (
    .clk_in(clk),
    .enable(lsu_gate_en),
    .test_en(test_en),
    .clk_out(clk_lsu_w)
  );
  E203ClkGate icg_biu (
    .clk_in(clk),
    .enable(biu_gate_en),
    .test_en(test_en),
    .clk_out(clk_biu_w)
  );
  assign clk_ifu = clk_ifu_w;
  assign clk_exu = clk_exu_w;
  assign clk_lsu = clk_lsu_w;
  assign clk_biu = clk_biu_w;
  assign ifu_clk_active = (ifu_gate_en | test_en);
  assign exu_clk_active = (exu_gate_en | test_en);
  assign lsu_clk_active = (lsu_gate_en | test_en);
  assign biu_clk_active = (biu_gate_en | test_en);

endmodule

