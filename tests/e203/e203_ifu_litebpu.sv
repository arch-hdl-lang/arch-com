// E203 HBirdv2 static branch prediction unit (LiteBPU)
// Handles JAL (seq taken), JALR (seq taken), and Bxx (taken if backward).
// Generates PC-adder operands and stall signal for data-dependent JALR.
module LiteBpu (
  input logic clk,
  input logic rst_n,
  input logic [32-1:0] pc,
  input logic dec_jal,
  input logic dec_jalr,
  input logic dec_bxx,
  input logic [32-1:0] dec_bjp_imm,
  input logic [5-1:0] dec_jalr_rs1idx,
  input logic oitf_empty,
  input logic ir_empty,
  input logic ir_rs1en,
  input logic jalr_rs1idx_cam_irrdidx,
  input logic dec_i_valid,
  input logic ir_valid_clr,
  input logic [32-1:0] rf2bpu_x1,
  input logic [32-1:0] rf2bpu_rs1,
  output logic prdt_taken,
  output logic [32-1:0] prdt_pc_add_op1,
  output logic [32-1:0] prdt_pc_add_op2,
  output logic bpu_wait,
  output logic bpu2rf_rs1_ena
);

  // Decode signals
  // Pipeline hazard state
  // Register file read-back
  // Outputs
  // State: tracks whether an xN regfile read is pending
  logic rs1xn_rdrf_r = 0;
  // ── Combinational intermediates ──────────────────────────────────────────
  // rs1 classification for JALR
  logic dec_jalr_rs1x0;
  assign dec_jalr_rs1x0 = dec_jalr_rs1idx == 0;
  logic dec_jalr_rs1x1;
  assign dec_jalr_rs1x1 = dec_jalr_rs1idx == 1;
  logic dec_jalr_rs1xn;
  assign dec_jalr_rs1xn = ~dec_jalr_rs1x0 & ~dec_jalr_rs1x1;
  // Immediate sign bit (negative = backward branch)
  logic bjp_imm_neg;
  assign bjp_imm_neg = dec_bjp_imm >> 31 != 0;
  // x1 dependency: OITF not empty, or IR target matches x1
  logic jalr_rs1x1_dep;
  assign jalr_rs1x1_dep = ~oitf_empty | jalr_rs1idx_cam_irrdidx;
  // xn dependency: OITF not empty, or IR has active rs1 match (not being cleared)
  logic jalr_rs1xn_dep;
  assign jalr_rs1xn_dep = ~oitf_empty | ~ir_empty & ir_rs1en & jalr_rs1idx_cam_irrdidx & ~ir_valid_clr;
  // xn dep being cleared this cycle (IR match + ir_valid_clr)
  logic jalr_rs1xn_dep_ir_clr;
  assign jalr_rs1xn_dep_ir_clr = ~ir_empty & ir_rs1en & jalr_rs1idx_cam_irrdidx & ir_valid_clr;
  // Regfile read request: issued when dep is clear (or clearing), and not already pending
  logic rs1xn_rdrf_set;
  assign rs1xn_rdrf_set = ~rs1xn_rdrf_r & dec_i_valid & dec_jalr & dec_jalr_rs1xn & (~jalr_rs1xn_dep | jalr_rs1xn_dep_ir_clr);
  // ── State machine ────────────────────────────────────────────────────────
  // rs1xn_rdrf_r is set by rs1xn_rdrf_set and self-clears after one cycle.
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      rs1xn_rdrf_r <= 0;
    end else begin
      rs1xn_rdrf_r <= rs1xn_rdrf_set;
    end
  end
  // ── Combinational outputs ─────────────────────────────────────────────────
  always_comb begin
    prdt_taken = dec_jal | dec_jalr | dec_bxx & bjp_imm_neg;
    prdt_pc_add_op2 = dec_bjp_imm;
    bpu_wait = dec_jalr & dec_jalr_rs1x1 & jalr_rs1x1_dep | dec_jalr & dec_jalr_rs1xn & jalr_rs1xn_dep & ~rs1xn_rdrf_r;
    bpu2rf_rs1_ena = rs1xn_rdrf_set;
    if (dec_jalr) begin
      if (dec_jalr_rs1x0) begin
        prdt_pc_add_op1 = 0;
      end else if (dec_jalr_rs1x1) begin
        prdt_pc_add_op1 = rf2bpu_x1;
      end else begin
        prdt_pc_add_op1 = rf2bpu_rs1;
      end
    end else begin
      prdt_pc_add_op1 = pc;
    end
  end

endmodule

// JAL/JALR: seq taken; Bxx: taken if backward (negative offset)
// PC-adder op2 is seq the branch immediate (truncated to PC_SIZE bits)
// BPU wait: JALR x1 dep unresolved, or JALR xN dep unresolved and read not issued
// Issue regfile read for xN
// PC-adder op1: rs1 value for JALR, PC for JAL/Bxx
