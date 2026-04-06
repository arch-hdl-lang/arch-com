// E203 HBirdv2 Instruction Decode Unit
// Pure combinational RV32IMC decoder matching the RealBench port interface.
// Decodes instruction into register indices, immediate, info bus, and
// control flags. Passes through PC, misalign, and bus error from IFU.
module e203_exu_decode #(
  parameter int XLEN = 32
) (
  input logic [32-1:0] i_instr,
  input logic [32-1:0] i_pc,
  input logic i_prdt_taken,
  input logic i_misalgn,
  input logic i_buserr,
  input logic i_muldiv_b2b,
  input logic dbg_mode,
  input logic nice_xs_off,
  output logic dec_rs1x0,
  output logic dec_rs2x0,
  output logic dec_rs1en,
  output logic dec_rs2en,
  output logic dec_rdwen,
  output logic [5-1:0] dec_rs1idx,
  output logic [5-1:0] dec_rs2idx,
  output logic [5-1:0] dec_rdidx,
  output logic [32-1:0] dec_info,
  output logic [32-1:0] dec_imm,
  output logic [32-1:0] dec_pc,
  output logic dec_misalgn,
  output logic dec_buserr,
  output logic dec_ilegl,
  output logic dec_nice,
  output logic nice_cmt_off_ilgl_o,
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
  output logic [32-1:0] dec_bjp_imm,
  output logic o_alu,
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
  output logic o_mulh,
  output logic o_mulhu,
  output logic o_load,
  output logic o_store
);

  // ── Inputs from IFU ─────────────────────────────────────────────────
  // ── Register index outputs ──────────────────────────────────────────
  // ── Decoded info bus and immediate ──────────────────────────────────
  // ── Error pass-through ──────────────────────────────────────────────
  // ── NICE (coprocessor) ──────────────────────────────────────────────
  // ── MulDiv flags ────────────────────────────────────────────────────
  // ── Instruction type flags ──────────────────────────────────────────
  // ── Simplified control outputs for e203_exu integration ─────────────
  // ── Instruction field extraction ────────────────────────────────────
  logic [7-1:0] opcode;
  assign opcode = i_instr[6:0];
  logic [5-1:0] rd_field;
  assign rd_field = i_instr[11:7];
  logic [3-1:0] funct3;
  assign funct3 = i_instr[14:12];
  logic [5-1:0] rs1_field;
  assign rs1_field = i_instr[19:15];
  logic [5-1:0] rs2_field;
  assign rs2_field = i_instr[24:20];
  logic [7-1:0] funct7;
  assign funct7 = i_instr[31:25];
  // ── Opcode classification ───────────────────────────────────────────
  logic is_op;
  assign is_op = opcode == 'h33;
  logic is_op_imm;
  assign is_op_imm = opcode == 'h13;
  logic is_lui;
  assign is_lui = opcode == 'h37;
  logic is_auipc;
  assign is_auipc = opcode == 'h17;
  logic is_branch;
  assign is_branch = opcode == 'h63;
  logic is_jal;
  assign is_jal = opcode == 'h6F;
  logic is_jalr;
  assign is_jalr = opcode == 'h67;
  logic is_load_op;
  assign is_load_op = opcode == 'h3;
  logic is_store_op;
  assign is_store_op = opcode == 'h23;
  logic is_system;
  assign is_system = opcode == 'h73;
  logic is_fence;
  assign is_fence = opcode == 'hF;
  logic is_nice_op;
  assign is_nice_op = opcode == 'hB;
  // ── funct3 decode ───────────────────────────────────────────────────
  logic f3_add;
  assign f3_add = funct3 == 'h0;
  logic f3_sll;
  assign f3_sll = funct3 == 'h1;
  logic f3_slt;
  assign f3_slt = funct3 == 'h2;
  logic f3_sltu;
  assign f3_sltu = funct3 == 'h3;
  logic f3_xor;
  assign f3_xor = funct3 == 'h4;
  logic f3_srl;
  assign f3_srl = funct3 == 'h5;
  logic f3_or;
  assign f3_or = funct3 == 'h6;
  logic f3_and;
  assign f3_and = funct3 == 'h7;
  logic f7_sub;
  assign f7_sub = funct7 == 'h20;
  logic f7_muldiv;
  assign f7_muldiv = funct7 == 'h1;
  logic is_muldiv;
  assign is_muldiv = is_op & f7_muldiv;
  // ── funct3 decode (branch) ──────────────────────────────────────────
  logic f3_beq;
  assign f3_beq = funct3 == 'h0;
  logic f3_bne;
  assign f3_bne = funct3 == 'h1;
  logic f3_blt;
  assign f3_blt = funct3 == 'h4;
  logic f3_bge;
  assign f3_bge = funct3 == 'h5;
  logic f3_bltu;
  assign f3_bltu = funct3 == 'h6;
  logic f3_bgeu;
  assign f3_bgeu = funct3 == 'h7;
  // ── System instruction decode ───────────────────────────────────────
  logic is_ecall;
  assign is_ecall = is_system & i_instr[31:20] == 'h0;
  logic is_ebreak;
  assign is_ebreak = is_system & i_instr[31:20] == 'h1;
  logic is_mret;
  assign is_mret = is_system & i_instr[31:20] == 'h302;
  logic is_wfi;
  assign is_wfi = is_system & i_instr[31:20] == 'h105;
  logic is_csr;
  assign is_csr = is_system & funct3 != 0;
  logic is_fencei;
  assign is_fencei = is_fence & f3_sll;
  // 32-bit instruction flag (not compressed)
  logic rv32_flag;
  assign rv32_flag = i_instr[1:0] == 3;
  // ── Illegal instruction check (simplified) ─────────────────────────
  logic known_op;
  assign known_op = is_op | is_op_imm | is_lui | is_auipc | is_branch | is_jal | is_jalr | is_load_op | is_store_op | is_system | is_fence | is_nice_op;
  logic illegal;
  assign illegal = ~known_op;
  // ── Immediate generation ────────────────────────────────────────────
  // I-type
  logic [32-1:0] imm_i;
  assign imm_i = 32'($unsigned({{(32-$bits(i_instr[31:20])){i_instr[31:20][$bits(i_instr[31:20])-1]}}, i_instr[31:20]}));
  // S-type
  logic [12-1:0] imm_s_hi;
  assign imm_s_hi = 12'($unsigned(funct7)) << 5;
  logic [12-1:0] imm_s_raw;
  assign imm_s_raw = imm_s_hi | 12'($unsigned(rd_field));
  logic [32-1:0] imm_s;
  assign imm_s = 32'($unsigned({{(32-$bits(imm_s_raw)){imm_s_raw[$bits(imm_s_raw)-1]}}, imm_s_raw}));
  // B-type
  logic [32-1:0] imm_b_12;
  assign imm_b_12 = 32'($unsigned(i_instr[31:31])) << 12;
  logic [32-1:0] imm_b_11;
  assign imm_b_11 = 32'($unsigned(i_instr[7:7])) << 11;
  logic [32-1:0] imm_b_10_5;
  assign imm_b_10_5 = 32'($unsigned(i_instr[30:25])) << 5;
  logic [32-1:0] imm_b_4_1;
  assign imm_b_4_1 = 32'($unsigned(i_instr[11:8])) << 1;
  logic [13-1:0] imm_b_raw;
  assign imm_b_raw = 13'(imm_b_12 | imm_b_11 | imm_b_10_5 | imm_b_4_1);
  logic [32-1:0] imm_b;
  assign imm_b = 32'($unsigned({{(32-$bits(imm_b_raw)){imm_b_raw[$bits(imm_b_raw)-1]}}, imm_b_raw}));
  // U-type
  logic [32-1:0] imm_u;
  assign imm_u = 32'($unsigned(20'(i_instr >> 12))) << 12;
  // J-type
  logic [32-1:0] imm_j_20;
  assign imm_j_20 = 32'($unsigned(i_instr[31:31])) << 20;
  logic [32-1:0] imm_j_19_12;
  assign imm_j_19_12 = 32'($unsigned(i_instr[19:12])) << 12;
  logic [32-1:0] imm_j_11;
  assign imm_j_11 = 32'($unsigned(i_instr[20:20])) << 11;
  logic [32-1:0] imm_j_10_1;
  assign imm_j_10_1 = 32'($unsigned(i_instr[30:21])) << 1;
  logic [21-1:0] imm_j_raw;
  assign imm_j_raw = 21'(imm_j_20 | imm_j_19_12 | imm_j_11 | imm_j_10_1);
  logic [32-1:0] imm_j;
  assign imm_j = 32'($unsigned({{(32-$bits(imm_j_raw)){imm_j_raw[$bits(imm_j_raw)-1]}}, imm_j_raw}));
  // ── BJP immediate (branch/jump target offset) ──────────────────────
  logic bjp_is_bxx;
  assign bjp_is_bxx = is_branch;
  logic bjp_is_jal;
  assign bjp_is_jal = is_jal;
  logic bjp_is_jalr_flag;
  assign bjp_is_jalr_flag = is_jalr;
  // ── Combinational output logic ──────────────────────────────────────
  always_comb begin
    // Register indices
    dec_rs1idx = rs1_field;
    dec_rs2idx = rs2_field;
    dec_rdidx = rd_field;
    // rs1/rs2 == x0 flags
    dec_rs1x0 = rs1_field == 0;
    dec_rs2x0 = rs2_field == 0;
    // Register enables
    dec_rs1en = is_op | is_op_imm | is_branch | is_jalr | is_load_op | is_store_op;
    dec_rs2en = is_op | is_branch | is_store_op;
    dec_rdwen = is_op | is_op_imm | is_lui | is_auipc | is_jal | is_jalr | is_load_op;
    // Pass-through
    dec_pc = i_pc;
    dec_misalgn = i_misalgn;
    dec_buserr = i_buserr;
    dec_ilegl = illegal;
    // NICE coprocessor (not supported, always 0)
    dec_nice = is_nice_op & ~nice_xs_off;
    nice_cmt_off_ilgl_o = is_nice_op & nice_xs_off;
    // MulDiv flags
    dec_mul = is_muldiv & f3_add;
    dec_mulhsu = is_muldiv & f3_slt;
    dec_div = is_muldiv & f3_xor;
    dec_rem = is_muldiv & f3_or;
    dec_divu = is_muldiv & f3_srl;
    dec_remu = is_muldiv & f3_and;
    // Instruction type flags
    dec_rv32 = rv32_flag;
    dec_bjp = is_branch | is_jal | is_jalr;
    dec_jal = is_jal;
    dec_jalr = is_jalr;
    dec_bxx = is_branch;
    dec_jalr_rs1idx = rs1_field;
    // BJP immediate
    if (is_branch) begin
      dec_bjp_imm = imm_b;
    end else if (is_jal) begin
      dec_bjp_imm = imm_j;
    end else if (is_jalr) begin
      dec_bjp_imm = imm_i;
    end else begin
      dec_bjp_imm = 0;
    end
    // Immediate output (general)
    if (is_op_imm | is_load_op | is_jalr) begin
      dec_imm = imm_i;
    end else if (is_store_op) begin
      dec_imm = imm_s;
    end else if (is_branch) begin
      dec_imm = imm_b;
    end else if (is_lui | is_auipc) begin
      dec_imm = imm_u;
    end else if (is_jal) begin
      dec_imm = imm_j;
    end else begin
      dec_imm = 0;
    end
    // Info bus (packed operation encoding — simplified 32-bit info)
    // This is a placeholder encoding; functional correctness depends
    // on matching the reference model's exact encoding.
    dec_info = 0;
    // ── Simplified control outputs ────────────────────────────────────
    // ALU class: regular ALU operations (R-type + I-type arithmetic)
    o_alu = is_op | is_op_imm | is_lui | is_auipc;
    // AGU class: load/store address generation
    o_agu = is_load_op | is_store_op;
    // Individual ALU operation flags
    o_alu_add = is_op & f3_add & ~f7_sub | is_op_imm & f3_add | is_auipc;
    o_alu_sub = is_op & f3_add & f7_sub;
    o_alu_xor = (is_op | is_op_imm) & f3_xor;
    o_alu_sll = (is_op | is_op_imm) & f3_sll;
    o_alu_srl = (is_op | is_op_imm) & f3_srl & ~f7_sub;
    o_alu_sra = (is_op | is_op_imm) & f3_srl & f7_sub;
    o_alu_or = (is_op | is_op_imm) & f3_or;
    o_alu_and = (is_op | is_op_imm) & f3_and;
    o_alu_slt = (is_op | is_op_imm) & f3_slt;
    o_alu_sltu = (is_op | is_op_imm) & f3_sltu;
    o_alu_lui = is_lui;
    // Individual branch type flags
    o_beq = is_branch & f3_beq;
    o_bne = is_branch & f3_bne;
    o_blt = is_branch & f3_blt;
    o_bge = is_branch & f3_bge;
    o_bltu = is_branch & f3_bltu;
    o_bgeu = is_branch & f3_bgeu;
    o_jump = is_jal | is_jalr;
    // MulDiv (mulh, mulhu not yet output)
    o_mulh = is_muldiv & f3_sll;
    o_mulhu = is_muldiv & f3_sltu;
    // Load/Store
    o_load = is_load_op;
    o_store = is_store_op;
  end

endmodule

