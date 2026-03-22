// E203 Execution Unit Top-Level
// Integrates: Decode → Dispatch → ALU/MulDiv → Commit → Regfile
// Single-issue, in-order pipeline with valid/ready handshake.
module ExuTop #(
  parameter int XLEN = 32
) (
  input logic clk,
  input logic rst_n,
  input logic ifu_valid,
  output logic ifu_ready,
  input logic [32-1:0] ifu_instr,
  input logic [32-1:0] ifu_pc,
  output logic o_bjp_valid,
  output logic o_bjp_taken,
  output logic [32-1:0] o_bjp_tgt,
  output logic lsu_valid,
  input logic lsu_ready,
  output logic [32-1:0] lsu_addr,
  output logic [32-1:0] lsu_wdata,
  output logic lsu_load,
  output logic lsu_store,
  input logic lsu_resp_valid,
  input logic [32-1:0] lsu_resp_data,
  output logic o_commit_valid
);

  // ── IFU interface ──────────────────────────────────────────────────
  // ── Branch feedback to IFU ─────────────────────────────────────────
  // ── LSU interface ──────────────────────────────────────────────────
  // ── Commit status ──────────────────────────────────────────────────
  // ── Decode ─────────────────────────────────────────────────────────
  logic [5-1:0] dec_rs1_idx;
  logic [5-1:0] dec_rs2_idx;
  logic [5-1:0] dec_rd_idx;
  logic [32-1:0] dec_imm;
  logic dec_alu;
  logic dec_bjp;
  logic dec_agu;
  logic dec_alu_add;
  logic dec_alu_sub;
  logic dec_alu_xor;
  logic dec_alu_sll;
  logic dec_alu_srl;
  logic dec_alu_sra;
  logic dec_alu_or;
  logic dec_alu_and;
  logic dec_alu_slt;
  logic dec_alu_sltu;
  logic dec_alu_lui;
  logic dec_beq;
  logic dec_bne;
  logic dec_blt;
  logic dec_bge;
  logic dec_bltu;
  logic dec_bgeu;
  logic dec_jump;
  logic dec_load;
  logic dec_store;
  logic dec_rs1_en;
  logic dec_rs2_en;
  logic dec_rd_en;
  ExuDecode dec (
    .instr(ifu_instr),
    .o_rs1_idx(dec_rs1_idx),
    .o_rs2_idx(dec_rs2_idx),
    .o_rd_idx(dec_rd_idx),
    .o_imm(dec_imm),
    .o_alu(dec_alu),
    .o_bjp(dec_bjp),
    .o_agu(dec_agu),
    .o_alu_add(dec_alu_add),
    .o_alu_sub(dec_alu_sub),
    .o_alu_xor(dec_alu_xor),
    .o_alu_sll(dec_alu_sll),
    .o_alu_srl(dec_alu_srl),
    .o_alu_sra(dec_alu_sra),
    .o_alu_or(dec_alu_or),
    .o_alu_and(dec_alu_and),
    .o_alu_slt(dec_alu_slt),
    .o_alu_sltu(dec_alu_sltu),
    .o_alu_lui(dec_alu_lui),
    .o_beq(dec_beq),
    .o_bne(dec_bne),
    .o_blt(dec_blt),
    .o_bge(dec_bge),
    .o_bltu(dec_bltu),
    .o_bgeu(dec_bgeu),
    .o_jump(dec_jump),
    .o_load(dec_load),
    .o_store(dec_store),
    .o_rs1_en(dec_rs1_en),
    .o_rs2_en(dec_rs2_en),
    .o_rd_en(dec_rd_en)
  );
  // ── Regfile (2R1W) — flattened port names ──────────────────────────
  ExuRegfile rf (
    .clk(clk),
    .rst_n(rst_n),
    .test_mode(1'b0),
    .read0_addr(dec_rs1_idx),
    .read0_data(rf_rs1_data),
    .read1_addr(dec_rs2_idx),
    .read1_data(rf_rs2_data),
    .write_en(wbck_fire),
    .write_addr(wbck_rd_idx),
    .write_data(wbck_wdat)
  );
  // ── Dispatch ───────────────────────────────────────────────────────
  logic disp_rdy;
  logic disp_alu_valid;
  logic [32-1:0] disp_alu_rs1;
  logic [32-1:0] disp_alu_rs2;
  logic [32-1:0] disp_alu_pc;
  logic [32-1:0] disp_alu_imm;
  logic [5-1:0] disp_alu_rdidx;
  logic disp_alu_op_add;
  logic disp_alu_op_sub;
  logic disp_alu_op_xor;
  logic disp_alu_op_sll;
  logic disp_alu_op_srl;
  logic disp_alu_op_sra;
  logic disp_alu_op_or;
  logic disp_alu_op_and;
  logic disp_alu_op_slt;
  logic disp_alu_op_sltu;
  logic disp_alu_op_lui;
  logic disp_alu_is_bjp;
  logic disp_alu_beq;
  logic disp_alu_bne;
  logic disp_alu_blt;
  logic disp_alu_bge;
  logic disp_alu_bltu;
  logic disp_alu_bgeu;
  logic disp_alu_is_jump;
  logic disp_alu_is_agu;
  logic disp_mdv_valid;
  logic [32-1:0] disp_mdv_rs1;
  logic [32-1:0] disp_mdv_rs2;
  logic disp_mdv_mul;
  logic disp_mdv_div;
  logic disp_lsu_valid;
  logic [32-1:0] disp_lsu_rs1;
  logic [32-1:0] disp_lsu_rs2;
  logic [32-1:0] disp_lsu_imm;
  logic disp_lsu_load;
  logic disp_lsu_store;
  ExuDisp disp (
    .disp_valid(ifu_valid),
    .disp_ready(disp_rdy),
    .i_rs1(rf_rs1_data),
    .i_rs2(rf_rs2_data),
    .i_pc(ifu_pc),
    .i_imm(dec_imm),
    .i_rd_idx(dec_rd_idx),
    .i_rd_en(dec_rd_en),
    .i_alu(dec_alu),
    .i_bjp(dec_bjp),
    .i_agu(dec_agu),
    .i_load(dec_load),
    .i_store(dec_store),
    .i_mul(1'b0),
    .i_div(1'b0),
    .i_alu_add(dec_alu_add),
    .i_alu_sub(dec_alu_sub),
    .i_alu_xor(dec_alu_xor),
    .i_alu_sll(dec_alu_sll),
    .i_alu_srl(dec_alu_srl),
    .i_alu_sra(dec_alu_sra),
    .i_alu_or(dec_alu_or),
    .i_alu_and(dec_alu_and),
    .i_alu_slt(dec_alu_slt),
    .i_alu_sltu(dec_alu_sltu),
    .i_alu_lui(dec_alu_lui),
    .i_beq(dec_beq),
    .i_bne(dec_bne),
    .i_blt(dec_blt),
    .i_bge(dec_bge),
    .i_bltu(dec_bltu),
    .i_bgeu(dec_bgeu),
    .i_jump(dec_jump),
    .alu_valid(disp_alu_valid),
    .alu_ready(alu_o_ready),
    .alu_rs1(disp_alu_rs1),
    .alu_rs2(disp_alu_rs2),
    .alu_pc(disp_alu_pc),
    .alu_imm(disp_alu_imm),
    .alu_rdidx(disp_alu_rdidx),
    .alu_op_add(disp_alu_op_add),
    .alu_op_sub(disp_alu_op_sub),
    .alu_op_xor(disp_alu_op_xor),
    .alu_op_sll(disp_alu_op_sll),
    .alu_op_srl(disp_alu_op_srl),
    .alu_op_sra(disp_alu_op_sra),
    .alu_op_or(disp_alu_op_or),
    .alu_op_and(disp_alu_op_and),
    .alu_op_slt(disp_alu_op_slt),
    .alu_op_sltu(disp_alu_op_sltu),
    .alu_op_lui(disp_alu_op_lui),
    .alu_is_bjp(disp_alu_is_bjp),
    .alu_beq(disp_alu_beq),
    .alu_bne(disp_alu_bne),
    .alu_blt(disp_alu_blt),
    .alu_bge(disp_alu_bge),
    .alu_bltu(disp_alu_bltu),
    .alu_bgeu(disp_alu_bgeu),
    .alu_is_jump(disp_alu_is_jump),
    .alu_is_agu(disp_alu_is_agu),
    .mdv_valid(disp_mdv_valid),
    .mdv_ready(1'b1),
    .mdv_rs1(disp_mdv_rs1),
    .mdv_rs2(disp_mdv_rs2),
    .mdv_mul(disp_mdv_mul),
    .mdv_div(disp_mdv_div),
    .lsu_valid(disp_lsu_valid),
    .lsu_ready(lsu_ready),
    .lsu_rs1(disp_lsu_rs1),
    .lsu_rs2(disp_lsu_rs2),
    .lsu_imm(disp_lsu_imm),
    .lsu_load(disp_lsu_load),
    .lsu_store(disp_lsu_store)
  );
  // ── ALU ────────────────────────────────────────────────────────────
  logic alu_o_ready;
  logic alu_done_valid;
  logic [32-1:0] alu_wdat;
  logic [5-1:0] alu_rdidx;
  logic alu_bjp_taken;
  logic [32-1:0] alu_bjp_tgt;
  logic [32-1:0] alu_bjp_lnk;
  ExuAlu alu (
    .clk(clk),
    .rst_n(rst_n),
    .i_valid(disp_alu_valid),
    .i_ready(alu_o_ready),
    .i_rs1(disp_alu_rs1),
    .i_rs2(disp_alu_rs2),
    .i_pc(disp_alu_pc),
    .i_imm(disp_alu_imm),
    .i_rdidx(disp_alu_rdidx),
    .i_alu(1'b1),
    .i_alu_add(disp_alu_op_add),
    .i_alu_sub(disp_alu_op_sub),
    .i_alu_xor(disp_alu_op_xor),
    .i_alu_sll(disp_alu_op_sll),
    .i_alu_srl(disp_alu_op_srl),
    .i_alu_sra(disp_alu_op_sra),
    .i_alu_or(disp_alu_op_or),
    .i_alu_and(disp_alu_op_and),
    .i_alu_slt(disp_alu_op_slt),
    .i_alu_sltu(disp_alu_op_sltu),
    .i_alu_lui(disp_alu_op_lui),
    .i_bjp(disp_alu_is_bjp),
    .i_beq(disp_alu_beq),
    .i_bne(disp_alu_bne),
    .i_blt(disp_alu_blt),
    .i_bge(disp_alu_bge),
    .i_bltu(disp_alu_bltu),
    .i_bgeu(disp_alu_bgeu),
    .i_jump(disp_alu_is_jump),
    .i_agu(disp_alu_is_agu),
    .i_agu_swap(1'b0),
    .i_agu_add(1'b0),
    .i_agu_and(1'b0),
    .i_agu_or(1'b0),
    .i_agu_xor(1'b0),
    .i_agu_max(1'b0),
    .i_agu_min(1'b0),
    .i_agu_maxu(1'b0),
    .i_agu_minu(1'b0),
    .i_agu_sbf_0_ena(1'b0),
    .i_agu_sbf_0_nxt(0),
    .i_agu_sbf_1_ena(1'b0),
    .i_agu_sbf_1_nxt(0),
    .o_valid(alu_done_valid),
    .o_ready(commit_alu_ready),
    .o_wdat(alu_wdat),
    .o_rdidx(alu_rdidx),
    .o_bjp_taken(alu_bjp_taken),
    .o_bjp_tgt(alu_bjp_tgt),
    .o_bjp_lnk(alu_bjp_lnk)
  );
  // ── Commit arbiter ─────────────────────────────────────────────────
  logic commit_alu_ready;
  logic cmt_long_ready;
  logic wbck_valid_w;
  logic [32-1:0] wbck_wdat;
  logic [5-1:0] wbck_rd_idx;
  logic wbck_rd_en;
  logic cmt_valid_out;
  logic [2-1:0] cmt_src_out;
  ExuCommit cmt (
    .clk(clk),
    .rst(1'b0),
    .alu_valid(alu_done_valid),
    .alu_ready(commit_alu_ready),
    .alu_wdat(alu_wdat),
    .alu_rd_idx(alu_rdidx),
    .alu_rd_en(1'b1),
    .long_valid(1'b0),
    .long_ready(cmt_long_ready),
    .long_wdat(0),
    .long_rd_idx(0),
    .long_rd_en(1'b0),
    .wbck_valid(wbck_valid_w),
    .wbck_ready(1'b1),
    .wbck_wdat(wbck_wdat),
    .wbck_rd_idx(wbck_rd_idx),
    .wbck_rd_en(wbck_rd_en),
    .commit_valid(cmt_valid_out),
    .commit_src(cmt_src_out)
  );
  // ── Output logic ───────────────────────────────────────────────────
  assign ifu_ready = disp_rdy;
  assign wbck_fire = (wbck_valid_w & wbck_rd_en);
  assign o_bjp_valid = (alu_done_valid & alu_bjp_taken);
  assign o_bjp_taken = alu_bjp_taken;
  assign o_bjp_tgt = alu_bjp_tgt;
  assign lsu_valid = disp_lsu_valid;
  assign lsu_addr = (disp_lsu_rs1 + disp_lsu_imm);
  assign lsu_wdata = disp_lsu_rs2;
  assign lsu_load = disp_lsu_load;
  assign lsu_store = disp_lsu_store;
  assign o_commit_valid = cmt_valid_out;

endmodule

// BJP feedback
// LSU interface
// Commit status
