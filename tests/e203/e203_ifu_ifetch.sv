// E203 HBirdv2 Instruction Fetch Unit
// Manages PC generation, instruction fetch requests, branch prediction,
// pipeline flush, and output to decode stage.
// Matches RealBench port interface.
module e203_ifu_ifetch #(
  parameter int XLEN = 32
) (
  input logic clk,
  input logic rst_n,
  output logic [32-1:0] inspect_pc,
  input logic [32-1:0] pc_rtvec,
  output logic ifu_req_valid,
  input logic ifu_req_ready,
  output logic [32-1:0] ifu_req_pc,
  output logic ifu_req_seq,
  output logic ifu_req_seq_rv32,
  output logic [32-1:0] ifu_req_last_pc,
  input logic ifu_rsp_valid,
  output logic ifu_rsp_ready,
  input logic ifu_rsp_err,
  input logic [32-1:0] ifu_rsp_instr,
  output logic [32-1:0] ifu_o_ir,
  output logic [32-1:0] ifu_o_pc,
  output logic ifu_o_pc_vld,
  output logic [5-1:0] ifu_o_rs1idx,
  output logic [5-1:0] ifu_o_rs2idx,
  output logic ifu_o_prdt_taken,
  output logic ifu_o_misalgn,
  output logic ifu_o_buserr,
  output logic ifu_o_muldiv_b2b,
  output logic ifu_o_valid,
  input logic ifu_o_ready,
  output logic pipe_flush_ack,
  input logic pipe_flush_req,
  input logic [32-1:0] pipe_flush_add_op1,
  input logic [32-1:0] pipe_flush_add_op2,
  input logic [32-1:0] pipe_flush_pc,
  input logic ifu_halt_req,
  output logic ifu_halt_ack,
  input logic oitf_empty,
  input logic [32-1:0] rf2ifu_x1,
  input logic [32-1:0] rf2ifu_rs1,
  input logic dec2ifu_rs1en,
  input logic dec2ifu_rden,
  input logic [5-1:0] dec2ifu_rdidx,
  input logic dec2ifu_mulhsu,
  input logic dec2ifu_div,
  input logic dec2ifu_rem,
  input logic dec2ifu_divu,
  input logic dec2ifu_remu
);

  // ── PC inspect output ─────────────────────────────────────────────
  // ── Instruction memory request ────────────────────────────────────
  // ── Instruction memory response ───────────────────────────────────
  // ── Output to decode stage ────────────────────────────────────────
  // ── Pipeline flush interface ──────────────────────────────────────
  // ── Halt interface ────────────────────────────────────────────────
  // ── OITF and register file ────────────────────────────────────────
  // ── Decode feedback for branch prediction ─────────────────────────
  // ── PC registers ──────────────────────────────────────────────────
  logic [32-1:0] pc_r = 0;
  logic [32-1:0] ir_r = 0;
  logic [32-1:0] pc_out_r = 0;
  logic pc_vld_r = 0;
  logic buserr_r = 0;
  logic out_valid_r = 0;
  logic req_pending_r = 0;
  logic [32-1:0] last_pc_r = 0;
  logic last_rv32_r = 0;
  // ── Flush target address ──────────────────────────────────────────
  logic [33-1:0] flush_target;
  assign flush_target = 33'(33'($unsigned(pipe_flush_add_op1)) + 33'($unsigned(pipe_flush_add_op2)));
  logic [32-1:0] flush_pc;
  assign flush_pc = 32'(flush_target);
  // ── Sequential PC: PC+4 ──────────────────────────────────────────
  logic [32-1:0] pc_plus4;
  assign pc_plus4 = 32'(pc_r + 4);
  // ── MulDiv back-to-back detection ─────────────────────────────────
  logic muldiv_b2b;
  assign muldiv_b2b = dec2ifu_rden & (dec2ifu_mulhsu | dec2ifu_div | dec2ifu_rem | dec2ifu_divu | dec2ifu_remu);
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      buserr_r <= 0;
      ir_r <= 0;
      last_pc_r <= 0;
      last_rv32_r <= 0;
      out_valid_r <= 0;
      pc_out_r <= 0;
      pc_r <= 0;
      pc_vld_r <= 0;
      req_pending_r <= 0;
    end else begin
      // Pipeline flush: reload PC
      if (pipe_flush_req) begin
        pc_r <= flush_pc;
        out_valid_r <= 1'b0;
        req_pending_r <= 1'b0;
      end else if (ifu_halt_req) begin
        out_valid_r <= 1'b0;
        req_pending_r <= 1'b0;
      end else if (ifu_req_valid & ifu_req_ready) begin
        // Request accepted
        last_pc_r <= pc_r;
        last_rv32_r <= pc_r[1:0] == 0;
        req_pending_r <= 1'b1;
      end
      if (req_pending_r & ifu_rsp_valid) begin
        // Response received
        ir_r <= ifu_rsp_instr;
        pc_out_r <= pc_r;
        pc_vld_r <= 1'b1;
        buserr_r <= ifu_rsp_err;
        out_valid_r <= 1'b1;
        pc_r <= pc_plus4;
        req_pending_r <= 1'b0;
      end else if (ifu_o_valid & ifu_o_ready) begin
        out_valid_r <= 1'b0;
      end
    end
  end
  assign inspect_pc = pc_r;
  assign ifu_req_valid = ~pipe_flush_req & ~ifu_halt_req & ~req_pending_r & ~out_valid_r;
  assign ifu_req_pc = pc_r;
  assign ifu_req_seq = ~pipe_flush_req;
  assign ifu_req_seq_rv32 = last_rv32_r;
  assign ifu_req_last_pc = last_pc_r;
  assign ifu_rsp_ready = req_pending_r;
  assign ifu_o_valid = out_valid_r;
  assign ifu_o_ir = ir_r;
  assign ifu_o_pc = pc_out_r;
  assign ifu_o_pc_vld = pc_vld_r;
  assign ifu_o_rs1idx = ir_r[19:15];
  assign ifu_o_rs2idx = ir_r[24:20];
  assign ifu_o_prdt_taken = 1'b0;
  assign ifu_o_misalgn = 1'b0;
  assign ifu_o_buserr = buserr_r;
  assign ifu_o_muldiv_b2b = muldiv_b2b;
  assign pipe_flush_ack = pipe_flush_req;
  assign ifu_halt_ack = ifu_halt_req & ~req_pending_r;

endmodule

// PC inspection
// Fetch request
// Response accept
// Output to decode
// Flush/halt ack
