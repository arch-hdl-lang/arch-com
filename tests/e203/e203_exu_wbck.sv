// E203 HBirdv2 write-back arbiter
// Arbitrates between ALU (lower priority) and long-pipeline (higher priority)
// write-back requests, forwarding the winner to the integer register file.
// Purely combinational — no registers, no reset used.
module e203_exu_wbck #(
  parameter int XLEN = 32,
  parameter int RFIDX_WIDTH = 5
) (
  input logic clk,
  input logic rst_n,
  input logic alu_wbck_i_valid,
  output logic alu_wbck_i_ready,
  input logic [32-1:0] alu_wbck_i_wdat,
  input logic [5-1:0] alu_wbck_i_rdidx,
  input logic longp_wbck_i_valid,
  output logic longp_wbck_i_ready,
  input logic [32-1:0] longp_wbck_i_wdat,
  input logic [5-1:0] longp_wbck_i_flags,
  input logic [5-1:0] longp_wbck_i_rdidx,
  input logic longp_wbck_i_rdfpu,
  output logic rf_wbck_o_ena,
  output logic [32-1:0] rf_wbck_o_wdat,
  output logic [5-1:0] rf_wbck_o_rdidx
);

  // present for interface compatibility; unused
  // ALU write-back (lower priority)
  // Long-pipeline write-back (higher priority)
  // Register file write port
  assign longp_wbck_i_ready = 1;
  assign alu_wbck_i_ready = ~longp_wbck_i_valid;
  assign rf_wbck_o_wdat = longp_wbck_i_valid ? longp_wbck_i_wdat : alu_wbck_i_wdat;
  assign rf_wbck_o_rdidx = longp_wbck_i_valid ? longp_wbck_i_rdidx : alu_wbck_i_rdidx;
  assign rf_wbck_o_ena = longp_wbck_i_valid & ~longp_wbck_i_rdfpu | ~longp_wbck_i_valid & alu_wbck_i_valid;

endmodule

// RF is seq ready; longp has unconditional priority.
// Priority mux: longp_valid selects longp, else ALU passthrough
// ena: wbck_valid & ~rdfpu
// wbck_valid = longp_valid | (alu_valid & ~longp_valid)
// rdfpu = longp_valid ? longp_rdfpu : 0
