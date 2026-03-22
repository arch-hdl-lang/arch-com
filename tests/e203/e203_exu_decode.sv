// E203 HBirdv2 Instruction Decode Unit
// Pure combinational RV32I decoder. Takes a 32-bit instruction word and
// produces one-hot operation signals, immediate value, register indices,
// and register read/write enables.
//
// Feeds directly into ExuAlu (ALU/BJP ops) and the AGU (load/store).
// Supports: R-type ALU, I-type ALU, LUI, AUIPC, branches, JAL, JALR,
// loads, and stores.
module ExuDecode #(
  parameter int XLEN = 32
) (
  input logic [32-1:0] instr,
  output logic [5-1:0] o_rs1_idx,
  output logic [5-1:0] o_rs2_idx,
  output logic [5-1:0] o_rd_idx,
  output logic [32-1:0] o_imm,
  output logic o_alu,
  output logic o_bjp,
  output logic o_agu,
  output logic o_alu_add,
  output logic o_alu_sub,
  output logic o_alu_xor,
  output logic o_alu_sll,
  output logic o_alu_srl,
  output logic o_alu_sra,
  output logic o_alu_or,
  output logic o_alu_and,
  output logic o_alu_slt,
  output logic o_alu_sltu,
  output logic o_alu_lui,
  output logic o_beq,
  output logic o_bne,
  output logic o_blt,
  output logic o_bge,
  output logic o_bltu,
  output logic o_bgeu,
  output logic o_jump,
  output logic o_load,
  output logic o_store,
  output logic o_rs1_en,
  output logic o_rs2_en,
  output logic o_rd_en
);

  // ── Register indices ─────────────────────────────────────────────────
  // ── Unit select (one-hot) ────────────────────────────────────────────
  // ── ALU operation one-hot ────────────────────────────────────────────
  // ── BJP operation one-hot ────────────────────────────────────────────
  // ── Memory ───────────────────────────────────────────────────────────
  // ── Register enables ─────────────────────────────────────────────────
  // ── Instruction field extraction ─────────────────────────────────────
  logic [7-1:0] opcode;
  assign opcode = instr[6:0];
  logic [5-1:0] rd_field;
  assign rd_field = instr[11:7];
  logic [3-1:0] funct3;
  assign funct3 = instr[14:12];
  logic [5-1:0] rs1_field;
  assign rs1_field = instr[19:15];
  logic [5-1:0] rs2_field;
  assign rs2_field = instr[24:20];
  logic [7-1:0] funct7;
  assign funct7 = instr[31:25];
  // ── Opcode classification ────────────────────────────────────────────
  logic is_op;
  assign is_op = (opcode == 'h33);
  // R-type ALU
  logic is_op_imm;
  assign is_op_imm = (opcode == 'h13);
  // I-type ALU
  logic is_lui;
  assign is_lui = (opcode == 'h37);
  // LUI
  logic is_auipc;
  assign is_auipc = (opcode == 'h17);
  // AUIPC
  logic is_branch;
  assign is_branch = (opcode == 'h63);
  // Branch
  logic is_jal;
  assign is_jal = (opcode == 'h6F);
  // JAL
  logic is_jalr;
  assign is_jalr = (opcode == 'h67);
  // JALR
  logic is_load_op;
  assign is_load_op = (opcode == 'h3);
  // Load
  logic is_store_op;
  assign is_store_op = (opcode == 'h23);
  // Store
  // ── funct3 decode (ALU) ──────────────────────────────────────────────
  logic f3_add;
  assign f3_add = (funct3 == 'h0);
  logic f3_sll;
  assign f3_sll = (funct3 == 'h1);
  logic f3_slt;
  assign f3_slt = (funct3 == 'h2);
  logic f3_sltu;
  assign f3_sltu = (funct3 == 'h3);
  logic f3_xor;
  assign f3_xor = (funct3 == 'h4);
  logic f3_srl;
  assign f3_srl = (funct3 == 'h5);
  logic f3_or;
  assign f3_or = (funct3 == 'h6);
  logic f3_and;
  assign f3_and = (funct3 == 'h7);
  // funct7 bit 5 distinguishes ADD/SUB, SRL/SRA
  logic f7_sub;
  assign f7_sub = (funct7 == 'h20);
  // ── funct3 decode (branch) ───────────────────────────────────────────
  logic f3_beq;
  assign f3_beq = (funct3 == 'h0);
  logic f3_bne;
  assign f3_bne = (funct3 == 'h1);
  logic f3_blt;
  assign f3_blt = (funct3 == 'h4);
  logic f3_bge;
  assign f3_bge = (funct3 == 'h5);
  logic f3_bltu;
  assign f3_bltu = (funct3 == 'h6);
  logic f3_bgeu;
  assign f3_bgeu = (funct3 == 'h7);
  // ── Immediate generation ─────────────────────────────────────────────
  // I-type: instr[31:20] sign-extended
  logic [32-1:0] imm_i;
  assign imm_i = 32'($unsigned({{(32-$bits(instr[31:20])){instr[31:20][$bits(instr[31:20])-1]}}, instr[31:20]}));
  // S-type: {instr[31:25], instr[11:7]} sign-extended
  logic [12-1:0] imm_s_hi;
  assign imm_s_hi = (12'($unsigned(funct7)) << 5);
  logic [12-1:0] imm_s_raw;
  assign imm_s_raw = 12'((imm_s_hi | 12'($unsigned(rd_field))));
  logic [32-1:0] imm_s;
  assign imm_s = 32'($unsigned({{(32-$bits(imm_s_raw)){imm_s_raw[$bits(imm_s_raw)-1]}}, imm_s_raw}));
  // B-type: {instr[31], instr[7], instr[30:25], instr[11:8], 0} sign-extended
  logic [32-1:0] imm_b_12;
  assign imm_b_12 = (32'($unsigned(instr[31:31])) << 12);
  logic [32-1:0] imm_b_11;
  assign imm_b_11 = (32'($unsigned(instr[7:7])) << 11);
  logic [32-1:0] imm_b_10_5;
  assign imm_b_10_5 = (32'($unsigned(instr[30:25])) << 5);
  logic [32-1:0] imm_b_4_1;
  assign imm_b_4_1 = (32'($unsigned(instr[11:8])) << 1);
  logic [13-1:0] imm_b_raw;
  assign imm_b_raw = 13'((((imm_b_12 | imm_b_11) | imm_b_10_5) | imm_b_4_1));
  logic [32-1:0] imm_b;
  assign imm_b = 32'($unsigned({{(32-$bits(imm_b_raw)){imm_b_raw[$bits(imm_b_raw)-1]}}, imm_b_raw}));
  // U-type: {instr[31:12], 12'b0}
  logic [32-1:0] imm_u;
  assign imm_u = (32'($unsigned(20'((instr >> 12)))) << 12);
  // J-type: {instr[31], instr[19:12], instr[20], instr[30:21], 0} sign-extended
  logic [32-1:0] imm_j_20;
  assign imm_j_20 = (32'($unsigned(instr[31:31])) << 20);
  logic [32-1:0] imm_j_19_12;
  assign imm_j_19_12 = (32'($unsigned(instr[19:12])) << 12);
  logic [32-1:0] imm_j_11;
  assign imm_j_11 = (32'($unsigned(instr[20:20])) << 11);
  logic [32-1:0] imm_j_10_1;
  assign imm_j_10_1 = (32'($unsigned(instr[30:21])) << 1);
  logic [21-1:0] imm_j_raw;
  assign imm_j_raw = 21'((((imm_j_20 | imm_j_19_12) | imm_j_11) | imm_j_10_1));
  logic [32-1:0] imm_j;
  assign imm_j = 32'($unsigned({{(32-$bits(imm_j_raw)){imm_j_raw[$bits(imm_j_raw)-1]}}, imm_j_raw}));
  // ── Combinational output logic ───────────────────────────────────────
  always_comb begin
    o_rs1_idx = rs1_field;
    o_rs2_idx = rs2_field;
    o_rd_idx = rd_field;
    o_alu = (((is_op | is_op_imm) | is_lui) | is_auipc);
    o_bjp = ((is_branch | is_jal) | is_jalr);
    o_agu = (is_load_op | is_store_op);
    o_alu_add = ((((is_op & f3_add) & (~f7_sub)) | (is_op_imm & f3_add)) | is_auipc);
    o_alu_sub = ((is_op & f3_add) & f7_sub);
    o_alu_xor = ((is_op | is_op_imm) & f3_xor);
    o_alu_sll = ((is_op | is_op_imm) & f3_sll);
    o_alu_srl = (((is_op | is_op_imm) & f3_srl) & (~f7_sub));
    o_alu_sra = (((is_op | is_op_imm) & f3_srl) & f7_sub);
    o_alu_or = ((is_op | is_op_imm) & f3_or);
    o_alu_and = ((is_op | is_op_imm) & f3_and);
    o_alu_slt = ((is_op | is_op_imm) & f3_slt);
    o_alu_sltu = ((is_op | is_op_imm) & f3_sltu);
    o_alu_lui = is_lui;
    o_beq = (is_branch & f3_beq);
    o_bne = (is_branch & f3_bne);
    o_blt = (is_branch & f3_blt);
    o_bge = (is_branch & f3_bge);
    o_bltu = (is_branch & f3_bltu);
    o_bgeu = (is_branch & f3_bgeu);
    o_jump = (is_jal | is_jalr);
    o_load = is_load_op;
    o_store = is_store_op;
    o_rs1_en = (((((is_op | is_op_imm) | is_branch) | is_jalr) | is_load_op) | is_store_op);
    o_rs2_en = ((is_op | is_branch) | is_store_op);
    o_rd_en = ((((((is_op | is_op_imm) | is_lui) | is_auipc) | is_jal) | is_jalr) | is_load_op);
    if (((is_op_imm | is_load_op) | is_jalr)) begin
      o_imm = imm_i;
    end else if (is_store_op) begin
      o_imm = imm_s;
    end else if (is_branch) begin
      o_imm = imm_b;
    end else if ((is_lui | is_auipc)) begin
      o_imm = imm_u;
    end else if (is_jal) begin
      o_imm = imm_j;
    end else begin
      o_imm = 0;
    end
  end

endmodule

// Register indices
// Unit select
// ALU operation one-hot
// BJP operation one-hot
// Memory
// Register enables
// Immediate mux
