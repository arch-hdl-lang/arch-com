// E203 ICB-to-APB Bridge
// Converts ICB bus transactions to APB protocol.
// APB is a 2-phase protocol: SETUP (psel=1, penable=0) → ACCESS (penable=1).
// ICB cmd maps to APB write/read; ICB rsp returns when pready=1.
module Icb2Apb (
  input logic clk,
  input logic rst_n,
  input logic icb_cmd_valid,
  output logic icb_cmd_ready,
  input logic [32-1:0] icb_cmd_addr,
  input logic [32-1:0] icb_cmd_wdata,
  input logic [4-1:0] icb_cmd_wmask,
  input logic icb_cmd_read,
  output logic icb_rsp_valid,
  input logic icb_rsp_ready,
  output logic [32-1:0] icb_rsp_rdata,
  output logic icb_rsp_err,
  output logic psel,
  output logic penable,
  output logic [32-1:0] paddr,
  output logic [32-1:0] pwdata,
  output logic [4-1:0] pstrb,
  output logic pwrite,
  input logic [32-1:0] prdata,
  input logic pready,
  input logic pslverr
);

  // ICB slave interface
  // APB master interface
  // ── FSM states ──────────────────────────────────────────────────
  // Idle → Setup → Access → (back to Idle or respond)
  logic [2-1:0] fsm_st = 0;
  // 0=IDLE, 1=SETUP, 2=ACCESS
  // Latched command
  logic [32-1:0] cmd_addr_r = 0;
  logic [32-1:0] cmd_wdata_r = 0;
  logic [4-1:0] cmd_wmask_r = 0;
  logic cmd_read_r = 1'b0;
  // Latched response
  logic [32-1:0] rsp_rdata_r = 0;
  logic rsp_err_r = 1'b0;
  logic rsp_valid_r = 1'b0;
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      cmd_addr_r <= 0;
      cmd_read_r <= 1'b0;
      cmd_wdata_r <= 0;
      cmd_wmask_r <= 0;
      fsm_st <= 0;
      rsp_err_r <= 1'b0;
      rsp_rdata_r <= 0;
      rsp_valid_r <= 1'b0;
    end else begin
      if ((fsm_st == 0)) begin
        if (icb_cmd_valid) begin
          cmd_addr_r <= icb_cmd_addr;
          cmd_wdata_r <= icb_cmd_wdata;
          cmd_wmask_r <= icb_cmd_wmask;
          cmd_read_r <= icb_cmd_read;
          fsm_st <= 1;
        end
        if ((rsp_valid_r & icb_rsp_ready)) begin
          rsp_valid_r <= 1'b0;
        end
      end else if ((fsm_st == 1)) begin
        fsm_st <= 2;
      end else if ((fsm_st == 2)) begin
        if (pready) begin
          rsp_rdata_r <= prdata;
          rsp_err_r <= pslverr;
          rsp_valid_r <= 1'b1;
          fsm_st <= 0;
        end
      end
    end
  end
  // IDLE: accept new ICB command
  // Clear response after it's been accepted
  // SETUP phase: assert psel, move to ACCESS next cycle
  // ACCESS phase: wait for pready
  assign psel = ((fsm_st == 1) | (fsm_st == 2));
  assign penable = (fsm_st == 2);
  assign paddr = cmd_addr_r;
  assign pwdata = cmd_wdata_r;
  assign pstrb = cmd_wmask_r;
  assign pwrite = (~cmd_read_r);
  assign icb_cmd_ready = ((fsm_st == 0) & (~rsp_valid_r));
  assign icb_rsp_valid = rsp_valid_r;
  assign icb_rsp_rdata = rsp_rdata_r;
  assign icb_rsp_err = rsp_err_r;

endmodule

// APB signals
// ICB command ready: accept in IDLE when no pending response
// ICB response
