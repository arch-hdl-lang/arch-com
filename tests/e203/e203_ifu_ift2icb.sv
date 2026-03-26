// E203 HBirdv2 IFU Ift2Icb — Instruction Fetch to ICB bridge
// Converts IFU fetch requests into ITCM ICB transactions.
// Simplified: ITCM-only path (no external memory).
// Response path has a 1-cycle pipeline register.
module Ift2Icb (
  input logic clk,
  input logic rst_n,
  input logic ifu_req_valid,
  input logic [32-1:0] ifu_req_pc,
  output logic ifu_req_ready,
  output logic ifu_rsp_valid,
  output logic [32-1:0] ifu_rsp_instr,
  input logic ifu_rsp_ready,
  output logic itcm_cmd_valid,
  output logic [14-1:0] itcm_cmd_addr,
  input logic itcm_cmd_ready,
  input logic itcm_rsp_valid,
  input logic [32-1:0] itcm_rsp_data,
  output logic itcm_rsp_ready
);

  // IFU fetch request
  // IFU fetch response
  // ITCM ICB master interface
  // Response pipeline register
  logic rsp_valid_r = 0;
  logic [32-1:0] rsp_data_r = 0;
  // Backpressure: don't accept new request if response register is full
  // and downstream is not ready
  logic stall_pipe;
  assign stall_pipe = rsp_valid_r & ~ifu_rsp_ready;
  // Request path: pass through to ITCM when not stalled
  assign itcm_cmd_valid = ifu_req_valid & ~stall_pipe;
  assign itcm_cmd_addr = ifu_req_pc[15:2];
  assign ifu_req_ready = itcm_cmd_ready & ~stall_pipe;
  // Accept ITCM response when we can
  assign itcm_rsp_ready = ~stall_pipe;
  // Response pipeline: register ITCM response for IFU
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      rsp_data_r <= 0;
      rsp_valid_r <= 0;
    end else begin
      if (~stall_pipe) begin
        rsp_valid_r <= itcm_rsp_valid;
        rsp_data_r <= itcm_rsp_data;
      end
    end
  end
  // IFU response output
  assign ifu_rsp_valid = rsp_valid_r;
  assign ifu_rsp_instr = rsp_data_r;

endmodule

