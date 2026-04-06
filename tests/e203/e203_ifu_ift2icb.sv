// E203 HBirdv2 IFU Ift2Icb — Instruction Fetch to ICB bridge
// Converts IFU fetch requests into ITCM or BIU ICB transactions.
// Routes to ITCM when address matches itcm_region_indic, else to BIU.
// Response path has a 1-cycle pipeline register.
module e203_ifu_ift2icb #(
  parameter int ITCM_ADDR_WIDTH = 16
) (
  input logic clk,
  input logic rst_n,
  input logic itcm_nohold,
  input logic ifu_req_valid,
  output logic ifu_req_ready,
  input logic [32-1:0] ifu_req_pc,
  input logic ifu_req_seq,
  input logic ifu_req_seq_rv32,
  input logic [32-1:0] ifu_req_last_pc,
  output logic ifu_rsp_valid,
  input logic ifu_rsp_ready,
  output logic ifu_rsp_err,
  output logic [32-1:0] ifu_rsp_instr,
  input logic [32-1:0] itcm_region_indic,
  output logic ifu2itcm_icb_cmd_valid,
  input logic ifu2itcm_icb_cmd_ready,
  output logic [16-1:0] ifu2itcm_icb_cmd_addr,
  input logic ifu2itcm_icb_rsp_valid,
  output logic ifu2itcm_icb_rsp_ready,
  input logic ifu2itcm_icb_rsp_err,
  input logic [64-1:0] ifu2itcm_icb_rsp_rdata,
  output logic ifu2biu_icb_cmd_valid,
  input logic ifu2biu_icb_cmd_ready,
  output logic [32-1:0] ifu2biu_icb_cmd_addr,
  input logic ifu2biu_icb_rsp_valid,
  output logic ifu2biu_icb_rsp_ready,
  input logic ifu2biu_icb_rsp_err,
  input logic [32-1:0] ifu2biu_icb_rsp_rdata,
  input logic ifu2itcm_holdup
);

  // ITCM hold control
  // IFU fetch request
  // IFU fetch response
  // ITCM region indicator
  // ITCM ICB master interface
  // BIU ICB master interface
  // ITCM holdup signal
  // ── Region decode ──────────────────────────────────────────────────────
  logic [16-1:0] pc_region;
  assign pc_region = ifu_req_pc[31:16];
  logic [16-1:0] itcm_region;
  assign itcm_region = itcm_region_indic[31:16];
  logic is_itcm_region;
  assign is_itcm_region = pc_region == itcm_region;
  logic is_biu_region;
  assign is_biu_region = ~is_itcm_region;
  // ── Sequential PC calculation for sequential fetch ─────────────────────
  // For sequential access: last_pc + 4 (rv32) or last_pc + 2
  logic [32-1:0] seq_pc;
  assign seq_pc = ifu_req_seq_rv32 ? 32'(ifu_req_last_pc + 4) : 32'(ifu_req_last_pc + 2);
  // Use seq_pc when sequential, otherwise use ifu_req_pc
  logic [32-1:0] fetch_pc;
  assign fetch_pc = ifu_req_seq ? seq_pc : ifu_req_pc;
  // ── Response pipeline register ─────────────────────────────────────────
  logic rsp_valid_r = 0;
  logic rsp_err_r = 0;
  logic [32-1:0] rsp_instr_r = 0;
  // Backpressure
  logic stall_pipe;
  assign stall_pipe = rsp_valid_r & ~ifu_rsp_ready;
  // ── ITCM request path ──────────────────────────────────────────────────
  logic itcm_cmd_fire;
  assign itcm_cmd_fire = ifu2itcm_icb_cmd_valid & ifu2itcm_icb_cmd_ready;
  logic biu_cmd_fire;
  assign biu_cmd_fire = ifu2biu_icb_cmd_valid & ifu2biu_icb_cmd_ready;
  // ── Select response data from ITCM (64-bit) based on PC alignment ─────
  logic [32-1:0] itcm_rsp_data_sel;
  assign itcm_rsp_data_sel = fetch_pc[2:2] != 0 ? ifu2itcm_icb_rsp_rdata[63:32] : ifu2itcm_icb_rsp_rdata[31:0];
  // ── ITCM or BIU response mux ──────────────────────────────────────────
  // Track which path is active for response routing
  logic itcm_active_r = 0;
  assign ifu2itcm_icb_cmd_valid = ifu_req_valid & is_itcm_region & ~stall_pipe;
  assign ifu2itcm_icb_cmd_addr = fetch_pc[15:0];
  assign ifu2biu_icb_cmd_valid = ifu_req_valid & is_biu_region & ~stall_pipe;
  assign ifu2biu_icb_cmd_addr = fetch_pc;
  assign ifu_req_ready = (is_itcm_region ? ifu2itcm_icb_cmd_ready : ifu2biu_icb_cmd_ready) & ~stall_pipe;
  assign ifu2itcm_icb_rsp_ready = ~stall_pipe;
  assign ifu2biu_icb_rsp_ready = ~stall_pipe;
  assign ifu_rsp_valid = rsp_valid_r;
  assign ifu_rsp_err = rsp_err_r;
  assign ifu_rsp_instr = rsp_instr_r;
  // Command routing
  // Ready back to IFU
  // Response ready
  // IFU response
  // ── Response pipeline register ─────────────────────────────────────────
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      itcm_active_r <= 0;
      rsp_err_r <= 0;
      rsp_instr_r <= 0;
      rsp_valid_r <= 0;
    end else begin
      if (~stall_pipe) begin
        if (itcm_active_r & ifu2itcm_icb_rsp_valid) begin
          rsp_valid_r <= 1'b1;
          rsp_err_r <= ifu2itcm_icb_rsp_err;
          rsp_instr_r <= itcm_rsp_data_sel;
        end else if (~itcm_active_r & ifu2biu_icb_rsp_valid) begin
          rsp_valid_r <= 1'b1;
          rsp_err_r <= ifu2biu_icb_rsp_err;
          rsp_instr_r <= ifu2biu_icb_rsp_rdata;
        end else begin
          rsp_valid_r <= 1'b0;
          rsp_err_r <= 1'b0;
          rsp_instr_r <= 0;
        end
      end
      // Track which path was requested
      if (itcm_cmd_fire) begin
        itcm_active_r <= 1'b1;
      end else if (biu_cmd_fire) begin
        itcm_active_r <= 1'b0;
      end
    end
  end

endmodule

