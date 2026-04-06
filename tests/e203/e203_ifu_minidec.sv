// E203 HBirdv2 IFU Mini Decoder
// Lightweight combinational decoder used inside the IFU to quickly classify
// fetched instructions before full decode in the EXU. Extracts just enough
// info for branch/jump handling.
module e203_ifu_minidec (
  input logic [32-1:0] instr,
  output logic dec_rs1en,
  output logic dec_rs2en,
  output logic [5-1:0] dec_rs1idx,
  output logic [5-1:0] dec_rs2idx,
  output logic dec_mulhsu,
  output logic dec_mul,
  output logic dec_div,
  output logic dec_rem,
  output logic dec_divu,
  output logic dec_remu,
  output logic dec_rv32,
  output logic dec_bjp,
  output logic dec_jal,
  output logic dec_jalr,
  output logic dec_bxx,
  output logic [5-1:0] dec_jalr_rs1idx,
  output logic [32-1:0] dec_bjp_imm
);

  // Register source enables and indices
  // M-extension decode
  // RV32 flag
  // Branch/jump decode
  // JALR rs1 index
  // Branch/jump immediate
  // ── Instruction field extraction ─────────────────────────────────────
  logic [7-1:0] opcode;
  assign opcode = instr[6:0];
  logic [3-1:0] funct3;
  assign funct3 = instr[14:12];
  logic [7-1:0] funct7;
  assign funct7 = instr[31:25];
  logic [5-1:0] rs1_idx;
  assign rs1_idx = instr[19:15];
  logic [5-1:0] rs2_idx;
  assign rs2_idx = instr[24:20];
  // ── Opcode classification ────────────────────────────────────────────
  logic is_bxx;
  assign is_bxx = opcode == 'h63;
  logic is_jal;
  assign is_jal = opcode == 'h6F;
  logic is_jalr;
  assign is_jalr = opcode == 'h67;
  logic is_op;
  assign is_op = opcode == 'h33;
  // R-type ALU (includes M-extension)
  logic is_op_imm;
  assign is_op_imm = opcode == 'h13;
  // I-type ALU
  logic is_load;
  assign is_load = opcode == 'h3;
  logic is_store;
  assign is_store = opcode == 'h23;
  logic is_lui;
  assign is_lui = opcode == 'h37;
  logic is_auipc;
  assign is_auipc = opcode == 'h17;
  logic is_system;
  assign is_system = opcode == 'h73;
  // ── M-extension decode (funct7 == 0x01 under OP) ────────────────────
  logic is_muldiv;
  assign is_muldiv = is_op & funct7 == 'h1;
  logic is_mul_op;
  assign is_mul_op = is_muldiv & funct3 == 0;
  logic is_mulh_op;
  assign is_mulh_op = is_muldiv & funct3 == 1;
  logic is_mulhsu_op;
  assign is_mulhsu_op = is_muldiv & funct3 == 2;
  logic is_mulhu_op;
  assign is_mulhu_op = is_muldiv & funct3 == 3;
  logic is_div_op;
  assign is_div_op = is_muldiv & funct3 == 4;
  logic is_divu_op;
  assign is_divu_op = is_muldiv & funct3 == 5;
  logic is_rem_op;
  assign is_rem_op = is_muldiv & funct3 == 6;
  logic is_remu_op;
  assign is_remu_op = is_muldiv & funct3 == 7;
  // ── Register source enables ─────────────────────────────────────────
  // For BPU purposes: only instructions that read rs1 and could affect
  // branch prediction need rs1_en. Simplified to standard R/I/S/B/JALR.
  logic rs1_en;
  assign rs1_en = is_op | is_op_imm | is_load | is_store | is_bxx | is_jalr;
  // rs2 used by: R-type, stores, branches
  logic rs2_en;
  assign rs2_en = is_op | is_store | is_bxx;
  // ── RV32 detection (all 32-bit instructions have [1:0] == 2'b11) ────
  logic is_rv32;
  assign is_rv32 = instr[1:0] == 3;
  // ── B-type immediate: {instr[31], instr[7], instr[30:25], instr[11:8], 1'b0} ──
  logic [32-1:0] bimm_12;
  assign bimm_12 = 32'($unsigned(instr[31:31])) << 12;
  logic [32-1:0] bimm_11;
  assign bimm_11 = 32'($unsigned(instr[7:7])) << 11;
  logic [32-1:0] bimm_10_5;
  assign bimm_10_5 = 32'($unsigned(instr[30:25])) << 5;
  logic [32-1:0] bimm_4_1;
  assign bimm_4_1 = 32'($unsigned(instr[11:8])) << 1;
  logic [13-1:0] bimm_raw;
  assign bimm_raw = 13'(bimm_12 | bimm_11 | bimm_10_5 | bimm_4_1);
  logic [32-1:0] bimm_se;
  assign bimm_se = $unsigned({{(32-$bits(bimm_raw)){bimm_raw[$bits(bimm_raw)-1]}}, bimm_raw});
  // ── J-type immediate: {instr[31], instr[19:12], instr[20], instr[30:21], 1'b0} ──
  logic [32-1:0] jimm_20;
  assign jimm_20 = 32'($unsigned(instr[31:31])) << 20;
  logic [32-1:0] jimm_19_12;
  assign jimm_19_12 = 32'($unsigned(instr[19:12])) << 12;
  logic [32-1:0] jimm_11;
  assign jimm_11 = 32'($unsigned(instr[20:20])) << 11;
  logic [32-1:0] jimm_10_1;
  assign jimm_10_1 = 32'($unsigned(instr[30:21])) << 1;
  logic [21-1:0] jimm_raw;
  assign jimm_raw = 21'(jimm_20 | jimm_19_12 | jimm_11 | jimm_10_1);
  logic [32-1:0] jimm_se;
  assign jimm_se = $unsigned({{(32-$bits(jimm_raw)){jimm_raw[$bits(jimm_raw)-1]}}, jimm_raw});
  // ── I-type immediate (JALR): sign-extend instr[31:20] ──
  logic [12-1:0] iimm_raw;
  assign iimm_raw = instr[31:20];
  logic [32-1:0] iimm_se;
  assign iimm_se = $unsigned({{(32-$bits(iimm_raw)){iimm_raw[$bits(iimm_raw)-1]}}, iimm_raw});
  // ── Output logic ─────────────────────────────────────────────────────
  always_comb begin
    dec_bxx = is_bxx;
    dec_jal = is_jal;
    dec_jalr = is_jalr;
    dec_bjp = is_bxx | is_jal | is_jalr;
    dec_rs1en = rs1_en;
    dec_rs2en = rs2_en;
    dec_rs1idx = rs1_idx;
    dec_rs2idx = rs2_idx;
    dec_mulhsu = is_mulhsu_op;
    dec_mul = is_mul_op | is_mulh_op | is_mulhsu_op | is_mulhu_op;
    dec_div = is_div_op;
    dec_rem = is_rem_op;
    dec_divu = is_divu_op;
    dec_remu = is_remu_op;
    dec_rv32 = is_rv32;
    dec_jalr_rs1idx = rs1_idx;
    if (is_bxx) begin
      dec_bjp_imm = bimm_se;
    end else if (is_jal) begin
      dec_bjp_imm = jimm_se;
    end else if (is_jalr) begin
      dec_bjp_imm = iimm_se;
    end else begin
      dec_bjp_imm = 0;
    end
  end

endmodule

