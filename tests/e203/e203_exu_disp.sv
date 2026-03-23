// E203 Execution Dispatch Unit
// Routes decoded instructions to ALU, MulDiv, or LSU.
// Valid/ready handshake: backpressure from any unit stalls dispatch.
module ExuDisp (
  input logic disp_valid,
  output logic disp_ready,
  input logic [32-1:0] i_rs1,
  input logic [32-1:0] i_rs2,
  input logic [32-1:0] i_pc,
  input logic [32-1:0] i_imm,
  input logic [5-1:0] i_rd_idx,
  input logic i_rd_en,
  input logic i_rs2_en,
  input logic i_alu,
  input logic i_bjp,
  input logic i_agu,
  input logic i_load,
  input logic i_store,
  input logic i_mul,
  input logic i_mulh,
  input logic i_mulhsu,
  input logic i_mulhu,
  input logic i_div,
  input logic i_divu,
  input logic i_rem,
  input logic i_remu,
  input logic i_alu_add,
  input logic i_alu_sub,
  input logic i_alu_xor,
  input logic i_alu_sll,
  input logic i_alu_srl,
  input logic i_alu_sra,
  input logic i_alu_or,
  input logic i_alu_and,
  input logic i_alu_slt,
  input logic i_alu_sltu,
  input logic i_alu_lui,
  input logic i_beq,
  input logic i_bne,
  input logic i_blt,
  input logic i_bge,
  input logic i_bltu,
  input logic i_bgeu,
  input logic i_jump,
  output logic alu_valid,
  input logic alu_ready,
  output logic [32-1:0] alu_rs1,
  output logic [32-1:0] alu_rs2,
  output logic [32-1:0] alu_pc,
  output logic [32-1:0] alu_imm,
  output logic [5-1:0] alu_rdidx,
  output logic alu_op_add,
  output logic alu_op_sub,
  output logic alu_op_xor,
  output logic alu_op_sll,
  output logic alu_op_srl,
  output logic alu_op_sra,
  output logic alu_op_or,
  output logic alu_op_and,
  output logic alu_op_slt,
  output logic alu_op_sltu,
  output logic alu_op_lui,
  output logic alu_is_bjp,
  output logic alu_beq,
  output logic alu_bne,
  output logic alu_blt,
  output logic alu_bge,
  output logic alu_bltu,
  output logic alu_bgeu,
  output logic alu_is_jump,
  output logic alu_is_agu,
  output logic mdv_valid,
  input logic mdv_ready,
  output logic [32-1:0] mdv_rs1,
  output logic [32-1:0] mdv_rs2,
  output logic [5-1:0] mdv_rdidx,
  output logic mdv_rd_en,
  output logic mdv_mul,
  output logic mdv_mulh,
  output logic mdv_mulhsu,
  output logic mdv_mulhu,
  output logic mdv_div,
  output logic mdv_divu,
  output logic mdv_rem,
  output logic mdv_remu,
  output logic lsu_valid,
  input logic lsu_ready,
  output logic [32-1:0] lsu_rs1,
  output logic [32-1:0] lsu_rs2,
  output logic [32-1:0] lsu_imm,
  output logic lsu_load,
  output logic lsu_store
);

  // Decoded instruction info
  // Decode flags — which unit
  // ALU op flags (pass-through)
  // BJP flags (pass-through)
  // ALU interface
  // MulDiv interface
  // LSU interface
  always_comb begin
    alu_valid = (disp_valid & ((i_alu | i_bjp) | i_agu));
    mdv_valid = (disp_valid & (((((((i_mul | i_mulh) | i_mulhsu) | i_mulhu) | i_div) | i_divu) | i_rem) | i_remu));
    lsu_valid = (disp_valid & (i_load | i_store));
    disp_ready = (((((i_alu | i_bjp) | i_agu) & alu_ready) | ((((((((i_mul | i_mulh) | i_mulhsu) | i_mulhu) | i_div) | i_divu) | i_rem) | i_remu) & mdv_ready)) | ((i_load | i_store) & lsu_ready));
    alu_rs1 = i_rs1;
    if (i_rs2_en) begin
      alu_rs2 = i_rs2;
    end else begin
      alu_rs2 = i_imm;
    end
    alu_pc = i_pc;
    alu_imm = i_imm;
    alu_rdidx = i_rd_idx;
    alu_op_add = i_alu_add;
    alu_op_sub = i_alu_sub;
    alu_op_xor = i_alu_xor;
    alu_op_sll = i_alu_sll;
    alu_op_srl = i_alu_srl;
    alu_op_sra = i_alu_sra;
    alu_op_or = i_alu_or;
    alu_op_and = i_alu_and;
    alu_op_slt = i_alu_slt;
    alu_op_sltu = i_alu_sltu;
    alu_op_lui = i_alu_lui;
    alu_is_bjp = i_bjp;
    alu_beq = i_beq;
    alu_bne = i_bne;
    alu_blt = i_blt;
    alu_bge = i_bge;
    alu_bltu = i_bltu;
    alu_bgeu = i_bgeu;
    alu_is_jump = i_jump;
    alu_is_agu = i_agu;
    mdv_rs1 = i_rs1;
    mdv_rs2 = i_rs2;
    mdv_rdidx = i_rd_idx;
    mdv_rd_en = i_rd_en;
    mdv_mul = i_mul;
    mdv_mulh = i_mulh;
    mdv_mulhsu = i_mulhsu;
    mdv_mulhu = i_mulhu;
    mdv_div = i_div;
    mdv_divu = i_divu;
    mdv_rem = i_rem;
    mdv_remu = i_remu;
    lsu_rs1 = i_rs1;
    lsu_rs2 = i_rs2;
    lsu_imm = i_imm;
    lsu_load = i_load;
    lsu_store = i_store;
  end

endmodule

// Valid signals — only one unit gets valid
// Ready: accept when the targeted unit is ready
// ALU operands — mux immediate into rs2 for I-type ALU/LUI (rs2_en=0)
// MulDiv operands
// LSU operands
