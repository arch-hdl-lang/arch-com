// E203 Core Top-Level Integration
// Ties together: EXU (decode+dispatch+ALU+commit+regfile) + LSU + CLINT
// IFU is simplified: instruction fetch is driven by testbench.
// This module validates that all generated SV modules instantiate and connect correctly.
module CoreTop #(
  parameter int XLEN = 32
) (
  input logic clk,
  input logic rst_n,
  input logic ifu_valid,
  output logic ifu_ready,
  input logic [32-1:0] ifu_instr,
  input logic [32-1:0] ifu_pc,
  output logic [32-1:0] mem_addr,
  output logic [32-1:0] mem_wdata,
  output logic [4-1:0] mem_wstrb,
  output logic mem_wen,
  input logic [32-1:0] mem_rdata,
  output logic tmr_irq,
  output logic commit_valid,
  output logic bjp_taken,
  output logic [32-1:0] bjp_tgt
);

  // ── Instruction interface (from testbench acting as IFU) ───────────
  // ── Data memory interface (directly from LSU to testbench) ─────────
  // ── Timer IRQ ──────────────────────────────────────────────────────
  // ── Status ─────────────────────────────────────────────────────────
  // ── EXU ────────────────────────────────────────────────────────────
  logic exu_ifu_ready;
  logic exu_bjp_valid;
  logic exu_bjp_taken;
  logic [32-1:0] exu_bjp_tgt;
  logic exu_lsu_valid;
  logic [32-1:0] exu_lsu_addr;
  logic [32-1:0] exu_lsu_wdata;
  logic exu_lsu_load;
  logic exu_lsu_store;
  logic exu_commit_valid;
  ExuTop exu (
    .clk(clk),
    .rst_n(rst_n),
    .ifu_valid(ifu_valid),
    .ifu_ready(exu_ifu_ready),
    .ifu_instr(ifu_instr),
    .ifu_pc(ifu_pc),
    .o_bjp_valid(exu_bjp_valid),
    .o_bjp_taken(exu_bjp_taken),
    .o_bjp_tgt(exu_bjp_tgt),
    .lsu_valid(exu_lsu_valid),
    .lsu_ready(1'b1),
    .lsu_addr(exu_lsu_addr),
    .lsu_wdata(exu_lsu_wdata),
    .lsu_load(exu_lsu_load),
    .lsu_store(exu_lsu_store),
    .lsu_resp_valid(1'b0),
    .lsu_resp_data(0),
    .o_commit_valid(exu_commit_valid)
  );
  // ── LSU byte/half/word alignment ───────────────────────────────────
  logic [32-1:0] lsu_mem_addr;
  logic [32-1:0] lsu_mem_wdata;
  logic [4-1:0] lsu_mem_wstrb;
  logic lsu_mem_wen;
  logic [32-1:0] lsu_load_result;
  LsuCtrl lsu (
    .addr(exu_lsu_addr),
    .wdata(exu_lsu_wdata),
    .funct3(2),
    .is_load(exu_lsu_load),
    .is_store(exu_lsu_store),
    .mem_addr(lsu_mem_addr),
    .mem_wdata(lsu_mem_wdata),
    .mem_wstrb(lsu_mem_wstrb),
    .mem_wen(lsu_mem_wen),
    .mem_rdata(mem_rdata),
    .load_result(lsu_load_result)
  );
  // ── CLINT Timer ────────────────────────────────────────────────────
  logic [32-1:0] timer_rdata;
  logic timer_irq;
  ClintTimer timer (
    .clk(clk),
    .rst(1'b0),
    .reg_addr(0),
    .reg_wdata(0),
    .reg_wen(1'b0),
    .reg_rdata(timer_rdata),
    .tmr_irq(timer_irq)
  );
  // ── Output connections ─────────────────────────────────────────────
  assign ifu_ready = exu_ifu_ready;
  assign mem_addr = lsu_mem_addr;
  assign mem_wdata = lsu_mem_wdata;
  assign mem_wstrb = lsu_mem_wstrb;
  assign mem_wen = lsu_mem_wen;
  assign tmr_irq = timer_irq;
  assign commit_valid = exu_commit_valid;
  assign bjp_taken = exu_bjp_taken;
  assign bjp_tgt = exu_bjp_tgt;

endmodule

