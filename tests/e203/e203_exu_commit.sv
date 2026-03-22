// E203 HBirdv2 Execution Commit Unit (9th E203 benchmark)
// Arbitrates between ALU (single-cycle) and long-pipe (multi-cycle muldiv)
// results. ALU has priority. Commits winning result to register file
// write-back port. Manages valid/ready handshake on both channels.
//
// Features exercised: {a,b} concat, {N{expr}} repeat, elsif, let
// domain SysDomain
//   freq_mhz: 100

module ExuCommit #(
  parameter int XLEN = 32
) (
  input logic clk,
  input logic rst,
  input logic alu_valid,
  output logic alu_ready,
  input logic [32-1:0] alu_wdat,
  input logic [5-1:0] alu_rd_idx,
  input logic alu_rd_en,
  input logic long_valid,
  output logic long_ready,
  input logic [32-1:0] long_wdat,
  input logic [5-1:0] long_rd_idx,
  input logic long_rd_en,
  output logic wbck_valid,
  input logic wbck_ready,
  output logic [32-1:0] wbck_wdat,
  output logic [5-1:0] wbck_rd_idx,
  output logic wbck_rd_en,
  output logic commit_valid,
  output logic [2-1:0] commit_src
);

  // ALU result channel (single-cycle, higher priority)
  // Long-pipe result channel (multi-cycle muldiv)
  // Register file write-back port
  // Commit status
  // 0=none, 1=alu, 2=long
  // Arbitration: ALU wins when both valid
  logic alu_win;
  assign alu_win = alu_valid;
  logic long_win;
  assign long_win = (long_valid & (~alu_valid));
  logic any_valid;
  assign any_valid = (alu_valid | long_valid);
  // Selected result (mux)
  logic [32-1:0] sel_wdat;
  assign sel_wdat = (alu_win) ? (alu_wdat) : (long_wdat);
  logic [5-1:0] sel_rd_idx;
  assign sel_rd_idx = (alu_win) ? (alu_rd_idx) : (long_rd_idx);
  logic sel_rd_en;
  assign sel_rd_en = (alu_win) ? (alu_rd_en) : (long_rd_en);
  // Handshake: forward wbck_ready to winning channel only
  logic alu_can_go;
  assign alu_can_go = (alu_win & wbck_ready);
  logic long_can_go;
  assign long_can_go = (long_win & wbck_ready);
  // Commit source encoding using concat
  logic [2-1:0] src_bits;
  assign src_bits = (alu_win) ? (1) : ((long_win) ? (2) : (0));
  assign wbck_valid = any_valid;
  assign wbck_wdat = sel_wdat;
  assign wbck_rd_idx = sel_rd_idx;
  assign wbck_rd_en = sel_rd_en;
  assign alu_ready = alu_can_go;
  assign long_ready = long_can_go;
  assign commit_valid = (any_valid & wbck_ready);
  assign commit_src = src_bits;

endmodule

// Write-back port drives
// Ready back to sources
// Status outputs
