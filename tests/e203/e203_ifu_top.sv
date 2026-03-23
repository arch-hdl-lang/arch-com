// E203 HBirdv2 IFU Top — wires IfuIfetch, IfuMinidec, LiteBpu, Ift2Icb
// Simplified: no RVC (16-bit) support, ITCM-only fetch path.
module IfuTop (
  input logic clk,
  input logic rst_n,
  output logic o_valid,
  input logic o_ready,
  output logic [32-1:0] o_instr,
  output logic [32-1:0] o_pc,
  output logic o_bus_err,
  input logic exu_redirect,
  input logic [32-1:0] exu_redirect_pc,
  output logic itcm_cmd_valid,
  output logic [14-1:0] itcm_cmd_addr,
  input logic itcm_cmd_ready,
  input logic itcm_rsp_valid,
  input logic [32-1:0] itcm_rsp_data,
  output logic itcm_rsp_ready,
  input logic oitf_empty,
  input logic ir_empty,
  input logic ir_rs1en,
  input logic jalr_rs1idx_cam_irrdidx,
  input logic ir_valid_clr,
  input logic [32-1:0] rf2bpu_x1,
  input logic [32-1:0] rf2bpu_rs1,
  output logic bpu_wait,
  output logic bpu2rf_rs1_ena,
  output logic prdt_taken,
  output logic [32-1:0] prdt_pc_add_op1,
  output logic [32-1:0] prdt_pc_add_op2,
  output logic dec_is_bjp,
  output logic dec_is_lui,
  output logic dec_is_auipc
);

  // Output to EXU: decoded instruction
  // Branch redirect from EXU
  // ITCM interface
  // OITF/regfile inputs for BPU
  // BPU outputs
  // Mini-decoder classification (used by EXU for decode hints)
  // ── IfuIfetch: PC generation + fetch state machine ──────────────────
  logic ifetch_req_valid;
  logic [32-1:0] ifetch_req_addr;
  logic ifetch_rsp_ready;
  IfuIfetch ifetch (
    .clk(clk),
    .rst(rst_n),
    .req_ready(icb_req_ready),
    .rsp_valid(icb_rsp_valid),
    .rsp_instr(icb_rsp_instr),
    .rsp_err(1'b0),
    .o_ready(o_ready),
    .redirect(exu_redirect),
    .redirect_pc(exu_redirect_pc),
    .req_valid(ifetch_req_valid),
    .req_addr(ifetch_req_addr),
    .rsp_ready(ifetch_rsp_ready),
    .o_valid(o_valid),
    .o_instr(o_instr),
    .o_pc(o_pc),
    .o_bus_err(o_bus_err)
  );
  // ── Ift2Icb: fetch-to-ITCM bridge ──────────────────────────────────
  logic icb_req_ready;
  logic icb_rsp_valid;
  logic [32-1:0] icb_rsp_instr;
  Ift2Icb icb (
    .clk(clk),
    .rst_n(rst_n),
    .ifu_req_valid(ifetch_req_valid),
    .ifu_req_pc(ifetch_req_addr),
    .ifu_rsp_ready(ifetch_rsp_ready),
    .itcm_cmd_ready(itcm_cmd_ready),
    .itcm_rsp_valid(itcm_rsp_valid),
    .itcm_rsp_data(itcm_rsp_data),
    .ifu_req_ready(icb_req_ready),
    .ifu_rsp_valid(icb_rsp_valid),
    .ifu_rsp_instr(icb_rsp_instr),
    .itcm_cmd_valid(itcm_cmd_valid),
    .itcm_cmd_addr(itcm_cmd_addr),
    .itcm_rsp_ready(itcm_rsp_ready)
  );
  // ── IfuMinidec: classify fetched instruction ────────────────────────
  logic mdec_is_jal;
  logic mdec_is_jalr;
  logic mdec_is_bxx;
  logic signed [21-1:0] mdec_bjp_imm;
  logic [5-1:0] mdec_rs1_idx;
  IfuMinidec mdec (
    .instr(o_instr),
    .o_is_bjp(dec_is_bjp),
    .o_is_jal(mdec_is_jal),
    .o_is_jalr(mdec_is_jalr),
    .o_is_bxx(mdec_is_bxx),
    .o_is_lui(dec_is_lui),
    .o_is_auipc(dec_is_auipc),
    .o_bjp_imm(mdec_bjp_imm),
    .o_rs1_idx(mdec_rs1_idx)
  );
  // ── LiteBpu: static branch prediction ──────────────────────────────
  LiteBpu bpu (
    .clk(clk),
    .rst_n(rst_n),
    .pc(o_pc),
    .dec_jal(mdec_is_jal),
    .dec_jalr(mdec_is_jalr),
    .dec_bxx(mdec_is_bxx),
    .dec_bjp_imm(32'($unsigned(mdec_bjp_imm))),
    .dec_jalr_rs1idx(mdec_rs1_idx),
    .oitf_empty(oitf_empty),
    .ir_empty(ir_empty),
    .ir_rs1en(ir_rs1en),
    .jalr_rs1idx_cam_irrdidx(jalr_rs1idx_cam_irrdidx),
    .dec_i_valid(o_valid),
    .ir_valid_clr(ir_valid_clr),
    .rf2bpu_x1(rf2bpu_x1),
    .rf2bpu_rs1(rf2bpu_rs1),
    .prdt_taken(prdt_taken),
    .prdt_pc_add_op1(prdt_pc_add_op1),
    .prdt_pc_add_op2(prdt_pc_add_op2),
    .bpu_wait(bpu_wait),
    .bpu2rf_rs1_ena(bpu2rf_rs1_ena)
  );

endmodule

