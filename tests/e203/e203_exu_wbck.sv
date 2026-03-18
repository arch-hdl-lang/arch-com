// E203 HBirdv2 write-back arbiter
// Arbitrates between ALU (lower priority) and long-pipeline (higher priority)
// write-back requests, forwarding the winner to the integer register file.
// Purely combinational — no registers, no reset used.
module ExuWbck #(
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
  always_comb begin
    longp_wbck_i_ready = 1;
    alu_wbck_i_ready = (~longp_wbck_i_valid);
    if (longp_wbck_i_valid) begin
      rf_wbck_o_wdat = longp_wbck_i_wdat;
      rf_wbck_o_rdidx = longp_wbck_i_rdidx;
      rf_wbck_o_ena = (~longp_wbck_i_rdfpu);
    end else begin
      rf_wbck_o_wdat = alu_wbck_i_wdat;
      rf_wbck_o_rdidx = alu_wbck_i_rdidx;
      rf_wbck_o_ena = alu_wbck_i_valid;
    end
  end

endmodule

// RF is always ready; longp has unconditional priority.
// Data / index mux: longp wins when valid
// Write enable: suppress if longp is writing to FPU register
