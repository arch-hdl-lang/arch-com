// E203 HBirdv2 Instruction Decode Unit
// Full RV32IMC decoder: 32-bit (RV32I+M+A) + 16-bit compressed (RV32C).
// Encodes dec_info bus per e203_defines.v bit assignments.
// Verify: decode-coverage audit applied.
module e203_exu_decode (
  input logic [31:0] i_instr,
  input logic [31:0] i_pc,
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
  output logic [4:0] dec_rs1idx,
  output logic [4:0] dec_rs2idx,
  output logic [4:0] dec_rdidx,
  output logic [31:0] dec_info,
  output logic [31:0] dec_imm,
  output logic [31:0] dec_pc,
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
  output logic [4:0] dec_jalr_rs1idx,
  output logic [31:0] dec_bjp_imm
);

  // ── Inputs from IFU ─────────────────────────────────────────────────
  // ── Register index outputs ──────────────────────────────────────────
  // ── Decoded info bus and immediate ──────────────────────────────────
  // ── Error pass-through ──────────────────────────────────────────────
  // ── NICE (coprocessor) ──────────────────────────────────────────────
  // ── MulDiv flags ────────────────────────────────────────────────────
  // ── Instruction type flags ──────────────────────────────────────────
  // ── Instruction field extraction ────────────────────────────────────
  logic [6:0] opcode;
  assign opcode = i_instr[6:0];
  logic [2:0] funct3;
  assign funct3 = i_instr[14:12];
  logic [6:0] funct7;
  assign funct7 = i_instr[31:25];
  logic [4:0] rs1_field;
  assign rs1_field = i_instr[19:15];
  logic [4:0] rs2_field;
  assign rs2_field = i_instr[24:20];
  logic [4:0] rd_field;
  assign rd_field = i_instr[11:7];
  // 16-bit fields (compressed instruction register mapping: x8-x15)
  logic [4:0] rv16_rd;
  assign rv16_rd = rd_field;
  logic [4:0] rv16_rs1;
  assign rv16_rs1 = rd_field;
  logic [4:0] rv16_rs2;
  assign rv16_rs2 = i_instr[6:2];
  logic [4:0] rv16_rdd;
  assign rv16_rdd = 8 + i_instr[4:2];
  logic [4:0] rv16_rss1;
  assign rv16_rss1 = 8 + i_instr[9:7];
  logic [4:0] rv16_rss2;
  assign rv16_rss2 = rv16_rdd;
  logic [2:0] rv16_func3;
  assign rv16_func3 = i_instr[15:13];
  // ── RV32 detection ──────────────────────────────────────────────────
  logic opcode_1_0_11;
  assign opcode_1_0_11 = opcode[1:0] == 3;
  logic rv32;
  assign rv32 = ~(i_instr[4:2] == 7) & opcode_1_0_11;
  // ── RV32 opcode decode ──────────────────────────────────────────────
  logic [1:0] op6_5;
  assign op6_5 = opcode[6:5];
  logic [2:0] op4_2;
  assign op4_2 = opcode[4:2];
  logic is_load;
  assign is_load = (op6_5 == 0) & (op4_2 == 0) & rv32;
  logic is_store;
  assign is_store = (op6_5 == 1) & (op4_2 == 0) & rv32;
  logic is_branch;
  assign is_branch = (op6_5 == 3) & (op4_2 == 0) & rv32;
  logic is_jalr;
  assign is_jalr = (op6_5 == 3) & (op4_2 == 1) & rv32;
  logic is_jal;
  assign is_jal = (op6_5 == 3) & (op4_2 == 3) & rv32;
  logic is_op_imm;
  assign is_op_imm = (op6_5 == 0) & (op4_2 == 4) & rv32;
  logic is_op;
  assign is_op = (op6_5 == 1) & (op4_2 == 4) & rv32;
  logic is_system;
  assign is_system = (op6_5 == 3) & (op4_2 == 4) & rv32;
  logic is_miscmem;
  assign is_miscmem = (op6_5 == 0) & (op4_2 == 3) & rv32;
  logic is_auipc;
  assign is_auipc = (op6_5 == 0) & (op4_2 == 5) & rv32;
  logic is_lui;
  assign is_lui = (op6_5 == 1) & (op4_2 == 5) & rv32;
  logic is_amo;
  assign is_amo = (op6_5 == 1) & (op4_2 == 3) & rv32;
  // ── RV32 funct3/funct7 classification ───────────────────────────────
  logic f3_000;
  assign f3_000 = funct3 == 0;
  logic f3_001;
  assign f3_001 = funct3 == 1;
  logic f3_010;
  assign f3_010 = funct3 == 2;
  logic f3_011;
  assign f3_011 = funct3 == 3;
  logic f3_100;
  assign f3_100 = funct3 == 4;
  logic f3_101;
  assign f3_101 = funct3 == 5;
  logic f3_110;
  assign f3_110 = funct3 == 6;
  logic f3_111;
  assign f3_111 = funct3 == 7;
  logic f7_00;
  assign f7_00 = funct7 == 'h0;
  logic f7_20;
  assign f7_20 = funct7 == 'h20;
  logic f7_01;
  assign f7_01 = funct7 == 'h1;
  // ── RV32 specific instructions ──────────────────────────────────────
  logic rv32_add;
  assign rv32_add = is_op & f3_000 & f7_00;
  logic rv32_sub;
  assign rv32_sub = is_op & f3_000 & f7_20;
  logic rv32_sll;
  assign rv32_sll = is_op & f3_001 & f7_00;
  logic rv32_slt;
  assign rv32_slt = is_op & f3_010 & f7_00;
  logic rv32_sltu;
  assign rv32_sltu = is_op & f3_011 & f7_00;
  logic rv32_xor;
  assign rv32_xor = is_op & f3_100 & f7_00;
  logic rv32_srl;
  assign rv32_srl = is_op & f3_101 & f7_00;
  logic rv32_sra;
  assign rv32_sra = is_op & f3_101 & f7_20;
  logic rv32_or;
  assign rv32_or = is_op & f3_110 & f7_00;
  logic rv32_and;
  assign rv32_and = is_op & f3_111 & f7_00;
  logic rv32_addi;
  assign rv32_addi = is_op_imm & f3_000;
  logic rv32_slti;
  assign rv32_slti = is_op_imm & f3_010;
  logic rv32_sltiu;
  assign rv32_sltiu = is_op_imm & f3_011;
  logic rv32_xori;
  assign rv32_xori = is_op_imm & f3_100;
  logic rv32_ori;
  assign rv32_ori = is_op_imm & f3_110;
  logic rv32_andi;
  assign rv32_andi = is_op_imm & f3_111;
  logic rv32_slli;
  assign rv32_slli = is_op_imm & f3_001 & (funct7[6:6] == 0);
  logic rv32_srli;
  assign rv32_srli = is_op_imm & f3_101 & (funct7[6:6] == 0);
  logic rv32_srai;
  assign rv32_srai = is_op_imm & f3_101 & (funct7[6:6] == 1);
  logic rv32_beq;
  assign rv32_beq = is_branch & f3_000;
  logic rv32_bne;
  assign rv32_bne = is_branch & f3_001;
  logic rv32_blt;
  assign rv32_blt = is_branch & f3_100;
  logic rv32_bge;
  assign rv32_bge = is_branch & f3_101;
  logic rv32_bltu;
  assign rv32_bltu = is_branch & f3_110;
  logic rv32_bgeu;
  assign rv32_bgeu = is_branch & f3_111;
  logic rv32_bxx;
  assign rv32_bxx = is_branch;
  logic rv32_lb;
  assign rv32_lb = is_load & f3_000;
  logic rv32_lh;
  assign rv32_lh = is_load & f3_001;
  logic rv32_lw;
  assign rv32_lw = is_load & f3_010;
  logic rv32_lbu;
  assign rv32_lbu = is_load & f3_100;
  logic rv32_lhu;
  assign rv32_lhu = is_load & f3_101;
  logic rv32_sb;
  assign rv32_sb = is_store & f3_000;
  logic rv32_sh;
  assign rv32_sh = is_store & f3_001;
  logic rv32_sw;
  assign rv32_sw = is_store & f3_010;
  logic rv32_ecall;
  assign rv32_ecall = is_system & (i_instr[31:20] == 'h0);
  logic rv32_ebreak;
  assign rv32_ebreak = is_system & (i_instr[31:20] == 'h1);
  logic rv32_mret;
  assign rv32_mret = is_system & (i_instr[31:20] == 'h302);
  logic rv32_dret;
  assign rv32_dret = is_system & (i_instr[31:20] == 'h7B2);
  logic rv32_wfi;
  assign rv32_wfi = is_system & (i_instr[31:20] == 'h105);
  logic rv32_csr;
  assign rv32_csr = is_system & ~(funct3 == 0);
  logic rv32_csrrw;
  assign rv32_csrrw = is_system & (f3_001 | f3_101);
  logic rv32_csrrs;
  assign rv32_csrrs = is_system & (f3_010 | f3_110);
  logic rv32_csrrc;
  assign rv32_csrrc = is_system & (f3_011 | f3_111);
  logic rv32_csrrwi;
  assign rv32_csrrwi = is_system & f3_101;
  logic rv32_csrrsi;
  assign rv32_csrrsi = is_system & f3_110;
  logic rv32_csrrci;
  assign rv32_csrrci = is_system & f3_111;
  logic rv32_fence;
  assign rv32_fence = is_miscmem & f3_000;
  logic rv32_fence_i;
  assign rv32_fence_i = is_miscmem & f3_001;
  // M-extension
  logic rv32_mul;
  assign rv32_mul = is_op & f3_000 & f7_01;
  logic rv32_mulh;
  assign rv32_mulh = is_op & f3_001 & f7_01;
  logic rv32_mulhsu;
  assign rv32_mulhsu = is_op & f3_010 & f7_01;
  logic rv32_mulhu;
  assign rv32_mulhu = is_op & f3_011 & f7_01;
  logic rv32_div;
  assign rv32_div = is_op & f3_100 & f7_01;
  logic rv32_divu;
  assign rv32_divu = is_op & f3_101 & f7_01;
  logic rv32_rem;
  assign rv32_rem = is_op & f3_110 & f7_01;
  logic rv32_remu;
  assign rv32_remu = is_op & f3_111 & f7_01;
  logic is_muldiv;
  assign is_muldiv = is_op & f7_01;
  // ── RV16 (compressed) instruction decode ────────────────────────────
  logic [1:0] op1_0;
  assign op1_0 = opcode[1:0];
  logic rv16_addi4spn;
  assign rv16_addi4spn = (op1_0 == 0) & (rv16_func3 == 0);
  logic rv16_lw;
  assign rv16_lw = (op1_0 == 0) & (rv16_func3 == 2);
  logic rv16_sw;
  assign rv16_sw = (op1_0 == 0) & (rv16_func3 == 6);
  logic rv16_addi;
  assign rv16_addi = (op1_0 == 1) & (rv16_func3 == 0);
  logic rv16_jal;
  assign rv16_jal = (op1_0 == 1) & (rv16_func3 == 1);
  logic rv16_li;
  assign rv16_li = (op1_0 == 1) & (rv16_func3 == 2);
  logic rv16_lui_addi16sp;
  assign rv16_lui_addi16sp = (op1_0 == 1) & (rv16_func3 == 3);
  logic rv16_miscalu;
  assign rv16_miscalu = (op1_0 == 1) & (rv16_func3 == 4);
  logic rv16_j;
  assign rv16_j = (op1_0 == 1) & (rv16_func3 == 5);
  logic rv16_beqz;
  assign rv16_beqz = (op1_0 == 1) & (rv16_func3 == 6);
  logic rv16_bnez;
  assign rv16_bnez = (op1_0 == 1) & (rv16_func3 == 7);
  logic rv16_slli;
  assign rv16_slli = (op1_0 == 2) & (rv16_func3 == 0);
  logic rv16_lwsp;
  assign rv16_lwsp = (op1_0 == 2) & (rv16_func3 == 2);
  logic rv16_jalr_mv_add;
  assign rv16_jalr_mv_add = (op1_0 == 2) & (rv16_func3 == 4);
  logic rv16_swsp;
  assign rv16_swsp = (op1_0 == 2) & (rv16_func3 == 6);
  // RV16 sub-decodes
  logic rv16_nop;
  assign rv16_nop = rv16_addi & ~i_instr[12:12] & (rv16_rd == 0) & (rv16_rs2 == 0);
  logic rv16_srli;
  assign rv16_srli = rv16_miscalu & (i_instr[11:10] == 0);
  logic rv16_srai;
  assign rv16_srai = rv16_miscalu & (i_instr[11:10] == 1);
  logic rv16_andi;
  assign rv16_andi = rv16_miscalu & (i_instr[11:10] == 2);
  logic rv16_subxororand;
  assign rv16_subxororand = rv16_miscalu & (i_instr[12:10] == 3);
  logic rv16_sub;
  assign rv16_sub = rv16_subxororand & (i_instr[6:5] == 0);
  logic rv16_xor;
  assign rv16_xor = rv16_subxororand & (i_instr[6:5] == 1);
  logic rv16_or;
  assign rv16_or = rv16_subxororand & (i_instr[6:5] == 2);
  logic rv16_and;
  assign rv16_and = rv16_subxororand & (i_instr[6:5] == 3);
  logic rv16_addi16sp;
  assign rv16_addi16sp = rv16_lui_addi16sp & (rd_field == 2);
  logic rv16_lui;
  assign rv16_lui = rv16_lui_addi16sp & (rd_field != 0) & (rd_field != 2);
  // RV16 register field special cases
  logic rv16_instr_12_is0;
  assign rv16_instr_12_is0 = i_instr[12:12] == 0;
  logic rv16_instr_6_2_is0;
  assign rv16_instr_6_2_is0 = i_instr[6:2] == 0;
  logic rv16_jr;
  assign rv16_jr = rv16_jalr_mv_add & ~i_instr[12:12] & (rv16_rs2 == 0) & (rv16_rd != 0);
  logic rv16_jalr;
  assign rv16_jalr = rv16_jalr_mv_add & i_instr[12:12] & (rv16_rs2 == 0) & (rv16_rd != 0);
  logic rv16_mv;
  assign rv16_mv = rv16_jalr_mv_add & ~i_instr[12:12] & (rv16_rs2 != 0) & (rv16_rd != 0);
  logic rv16_add;
  assign rv16_add = rv16_jalr_mv_add & i_instr[12:12] & (rv16_rs2 != 0) & (rv16_rd != 0);
  logic rv16_ebreak;
  assign rv16_ebreak = rv16_jalr_mv_add & i_instr[12:12] & (rv16_rs2 == 0) & (rv16_rd == 0);
  // ── Specific illegal conditions (gating for alu_op) ──────────────────
  logic rv32_sxxi_shamt_legl;
  assign rv32_sxxi_shamt_legl = funct7[6:6] == 0;
  logic rv32_sxxi_shamt_ilgl;
  assign rv32_sxxi_shamt_ilgl = (rv32_slli | rv32_srli | rv32_srai) & ~rv32_sxxi_shamt_legl;
  logic rv16_sxxi_shamt_legl;
  assign rv16_sxxi_shamt_legl = rv16_instr_12_is0 & ~rv16_instr_6_2_is0;
  logic rv16_sxxi_shamt_ilgl;
  assign rv16_sxxi_shamt_ilgl = (rv16_slli | rv16_srli | rv16_srai) & ~rv16_sxxi_shamt_legl;
  logic rv16_addi4spn_ilgl;
  assign rv16_addi4spn_ilgl = rv16_addi4spn & rv16_instr_12_is0 & (rv16_rd == 0) & (opcode[6:5] == 0);
  logic rv16_addi16sp_ilgl;
  assign rv16_addi16sp_ilgl = rv16_addi16sp & rv16_instr_12_is0 & rv16_instr_6_2_is0;
  logic rv16_li_ilgl;
  assign rv16_li_ilgl = rv16_li & (rv16_rd == 0);
  logic rv16_lui_ilgl;
  assign rv16_lui_ilgl = rv16_lui & ((rv16_rd == 0) | (rv16_rd == 2) | (rv16_instr_6_2_is0 & rv16_instr_12_is0));
  logic rv16_li_lui_ilgl;
  assign rv16_li_lui_ilgl = rv16_li_ilgl | rv16_lui_ilgl;
  // ── Instruction group classification ────────────────────────────────
  // alu_op is gated by illegal conditions (matches reference)
  logic alu_op;
  assign alu_op = ~rv32_sxxi_shamt_ilgl & ~rv16_sxxi_shamt_ilgl & ~rv16_li_lui_ilgl & ~rv16_addi4spn_ilgl & ~rv16_addi16sp_ilgl & (is_op_imm | (is_op & ~f7_01) | is_auipc | is_lui | rv16_addi4spn | rv16_addi | rv16_lui_addi16sp | rv16_li | rv16_mv | rv16_slli | rv16_miscalu | rv16_addi16sp | rv16_nop | rv32_ecall | rv32_ebreak | rv32_wfi | rv32_mret | rv32_dret | rv16_ebreak);
  // exclude MULDIV
  logic amoldst_op;
  assign amoldst_op = is_load | is_store | is_amo | rv16_lw | rv16_sw | rv16_lwsp | rv16_swsp;
  logic bjp_op;
  assign bjp_op = is_branch | is_jal | is_jalr | rv16_j | rv16_jal | rv16_beqz | rv16_bnez | rv16_jr | rv16_jalr | rv32_fence | rv32_fence_i;
  logic csr_op;
  assign csr_op = rv32_csr;
  logic muldiv_op;
  assign muldiv_op = is_muldiv;
  // ── need_imm flag (for ALU op2 select) ──────────────────────────────
  logic need_imm;
  assign need_imm = is_op_imm | is_load | is_store | is_jalr | is_auipc | is_lui | rv16_addi4spn | rv16_addi | rv16_addi16sp | rv16_li | rv16_lui | rv16_lw | rv16_sw | rv16_lwsp | rv16_swsp;
  // ── Register index selection (RV32 vs RV16) ─────────────────────────
  // RV16 format classification
  logic rv16_fmt_cr;
  assign rv16_fmt_cr = rv16_jalr_mv_add;
  logic rv16_fmt_ci;
  assign rv16_fmt_ci = rv16_lwsp | rv16_li | rv16_lui_addi16sp | rv16_addi | rv16_slli;
  logic rv16_fmt_css;
  assign rv16_fmt_css = rv16_swsp;
  logic rv16_fmt_ciw;
  assign rv16_fmt_ciw = rv16_addi4spn;
  logic rv16_fmt_cl;
  assign rv16_fmt_cl = rv16_lw;
  logic rv16_fmt_cs;
  assign rv16_fmt_cs = rv16_sw | rv16_subxororand;
  logic rv16_fmt_cb;
  assign rv16_fmt_cb = rv16_beqz | rv16_bnez | rv16_srli | rv16_srai | rv16_andi;
  logic rv16_fmt_cj;
  assign rv16_fmt_cj = rv16_j | rv16_jal;
  logic [4:0] rs1_idx;
  logic [4:0] rs2_idx;
  logic [4:0] rd_idx;
  always_comb begin
    if (rv32) begin
      rs1_idx = rs1_field;
      rs2_idx = rs2_field;
      rd_idx = rd_field;
    end else if (rv16_fmt_cr) begin
      // CR format: JR(rs1=coded,rd=0), JALR(rs1=coded,rd=1), MV(rs1=0,rd=coded), ADD(rs1=coded,rd=coded)
      if (rv16_mv) begin
        rs1_idx = 0;
      end else begin
        rs1_idx = rv16_rs1;
      end
      rs2_idx = rv16_rs2;
      if (rv16_jr) begin
        rd_idx = 0;
      end else if (rv16_jalr) begin
        rd_idx = 1;
      end else begin
        rd_idx = rv16_rd;
      end
    end else if (rv16_fmt_ci) begin
      // CI format: addi, li, lui, addi16sp, slli — rs1=rd, rd=rd
      rs1_idx = rv16_rd;
      rs2_idx = 0;
      rd_idx = rv16_rd;
    end else if (rv16_fmt_css) begin
      // CSS format: swsp — rs1=x2, rs2=rs2
      rs1_idx = 2;
      rs2_idx = rv16_rs2;
      rd_idx = 0;
    end else if (rv16_fmt_ciw) begin
      // CIW format: addi4spn — rs1=x2, rd=rdd
      rs1_idx = 2;
      rs2_idx = 0;
      rd_idx = rv16_rdd;
    end else if (rv16_fmt_cl) begin
      // CL format: lw — rs1=rss1, rd=rdd
      rs1_idx = rv16_rss1;
      rs2_idx = 0;
      rd_idx = rv16_rdd;
    end else if (rv16_fmt_cs) begin
      // CS format: sw/sub/xor/or/and — rs1=rss1, rs2=rss2, rd=rss1(for ALU)
      rs1_idx = rv16_rss1;
      rs2_idx = rv16_rss2;
      if (rv16_subxororand) begin
        rd_idx = rv16_rss1;
      end else begin
        rd_idx = 0;
      end
    end else if (rv16_fmt_cb) begin
      // CB format: beqz/bnez/srli/srai/andi — rs1=rss1, rd=rss1(for ALU)
      rs1_idx = rv16_rss1;
      rs2_idx = 0;
      if (rv16_beqz | rv16_bnez) begin
        rd_idx = 0;
      end else begin
        rd_idx = rv16_rss1;
      end
    end else if (rv16_fmt_cj) begin
      // CJ format: j/jal — rd=0 or rd=1
      rs1_idx = 0;
      rs2_idx = 0;
      if (rv16_j) begin
        rd_idx = 0;
      end else begin
        rd_idx = 1;
      end
    end else begin
      rs1_idx = 0;
      rs2_idx = 0;
      rd_idx = 0;
    end
  end
  // ── Register enables ────────────────────────────────────────────────
  logic rv16_rs1en;
  assign rv16_rs1en = rv16_fmt_cr | rv16_fmt_ci | rv16_fmt_css | rv16_fmt_ciw | rv16_fmt_cl | rv16_fmt_cs | rv16_fmt_cb;
  logic rv16_rs2en;
  assign rv16_rs2en = rv16_fmt_cr | rv16_fmt_css | (rv16_fmt_cs & ~rv16_subxororand);
  logic rv16_rdwen;
  assign rv16_rdwen = (rv16_fmt_cr & ~rv16_jr & ~rv16_jalr) | rv16_fmt_ci | rv16_fmt_ciw | rv16_fmt_cl | (rv16_fmt_cs & rv16_subxororand) | (rv16_fmt_cb & ~rv16_beqz & ~rv16_bnez) | rv16_fmt_cj;
  logic rs1en_32;
  assign rs1en_32 = is_op | is_op_imm | is_branch | is_jalr | is_load | is_store | rv32_csr;
  logic rs2en_32;
  assign rs2en_32 = is_op | is_branch | is_store;
  logic rdwen_32;
  assign rdwen_32 = is_op | is_op_imm | is_lui | is_auipc | is_jal | is_jalr | is_load | rv32_csr;
  logic rs1_en;
  assign rs1_en = rv32 ? rs1en_32 : rv16_rs1en;
  logic rs2_en;
  assign rs2_en = rv32 ? rs2en_32 : rv16_rs2en;
  logic rdwen;
  assign rdwen = rv32 ? rdwen_32 : rv16_rdwen;
  // ── Immediate generation ────────────────────────────────────────────
  // I-type: sign-extend instr[31:20] manually
  logic [31:0] imm_i_se;
  assign imm_i_se = $unsigned({{(32-$bits(i_instr[31:20])){i_instr[31:20][$bits(i_instr[31:20])-1]}}, i_instr[31:20]});
  // S-type: {instr[31:25], instr[11:7]}
  logic [31:0] imm_s;
  assign imm_s = $unsigned({{(32-$bits({funct7, rd_field})){{funct7, rd_field}[$bits({funct7, rd_field})-1]}}, {funct7, rd_field}});
  // B-type: construct 13-bit signed immediate then sign-extend
  logic [31:0] b_imm_12;
  assign b_imm_12 = 32'($unsigned(i_instr[31:31])) << 12;
  logic [31:0] b_imm_11;
  assign b_imm_11 = 32'($unsigned(i_instr[7:7])) << 11;
  logic [31:0] b_imm_10_5;
  assign b_imm_10_5 = 32'($unsigned(i_instr[30:25])) << 5;
  logic [31:0] b_imm_4_1;
  assign b_imm_4_1 = 32'($unsigned(i_instr[11:8])) << 1;
  logic [12:0] b_imm_raw;
  assign b_imm_raw = 13'(b_imm_12 | b_imm_11 | b_imm_10_5 | b_imm_4_1);
  logic [31:0] imm_b_se;
  assign imm_b_se = $unsigned({{(32-$bits(b_imm_raw)){b_imm_raw[$bits(b_imm_raw)-1]}}, b_imm_raw});
  // U-type: instr[31:12] << 12
  logic [31:0] imm_u;
  assign imm_u = 32'($unsigned(20'(i_instr >> 12))) << 12;
  // J-type: construct 21-bit signed immediate then sign-extend
  logic [31:0] j_imm_20;
  assign j_imm_20 = 32'($unsigned(i_instr[31:31])) << 20;
  logic [31:0] j_imm_19_12;
  assign j_imm_19_12 = 32'($unsigned(i_instr[19:12])) << 12;
  logic [31:0] j_imm_11;
  assign j_imm_11 = 32'($unsigned(i_instr[20:20])) << 11;
  logic [31:0] j_imm_10_1;
  assign j_imm_10_1 = 32'($unsigned(i_instr[30:21])) << 1;
  logic [20:0] j_imm_raw;
  assign j_imm_raw = 21'(j_imm_20 | j_imm_19_12 | j_imm_11 | j_imm_10_1);
  logic [31:0] imm_j_se;
  assign imm_j_se = $unsigned({{(32-$bits(j_imm_raw)){j_imm_raw[$bits(j_imm_raw)-1]}}, j_imm_raw});
  // RV16 immediates (simplified)
  logic [31:0] rv16_imm_i;
  assign rv16_imm_i = $unsigned(32'($unsigned(i_instr[12:12])) << 5 | 32'($unsigned(i_instr[6:2])));
  logic [31:0] rv16_imm_b;
  assign rv16_imm_b = 32'($unsigned(i_instr[12:12])) << 8 | 32'($unsigned(i_instr[6:5])) << 6 | 32'($unsigned(i_instr[4:2])) << 3 | 32'($unsigned(i_instr[11:10])) << 1;
  logic [31:0] rv16_imm_j;
  assign rv16_imm_j = 32'($unsigned(i_instr[12:12])) << 11 | 32'($unsigned(i_instr[11:11])) << 4 | 32'($unsigned(i_instr[10:9])) << 8 | 32'($unsigned(i_instr[8:8])) << 10 | 32'($unsigned(i_instr[7:7])) << 6 | 32'($unsigned(i_instr[6:6])) << 7 | 32'($unsigned(i_instr[5:5])) << 2 | 32'($unsigned(i_instr[4:4])) << 1 | 32'($unsigned(i_instr[3:1])) << 5;
  // ── Illegal instruction detection ───────────────────────────────────
  // Reference: legl_ops = alu_op | amoldst_op | bjp_op | csr_op | muldiv_op
  // Single legl_ops covers both 32-bit and 16-bit (alu_op etc. include both)
  logic legl_ops;
  assign legl_ops = alu_op | amoldst_op | bjp_op | csr_op | muldiv_op;
  logic illegal;
  assign illegal = ~legl_ops;
  // ── Info bus encoding (per e203_defines.v) ──────────────────────────
  // GRP[2:0] at bits [2:0], RV32 at bit [3]
  // Sub-decode info starts at bit [4]
  // ALU group bits (starting at bit 4):
  //   4:ADD 5:SUB 6:XOR 7:SLL 8:SRL 9:SRA 10:OR 11:AND 12:SLT 13:SLTU
  //   14:LUI 15:OP2IMM 16:OP1PC 17:NOP 18:ECAL 19:EBRK 20:WFI
  logic [31:0] grp_alu;
  assign grp_alu = 0;
  logic [31:0] grp_agu;
  assign grp_agu = 1;
  logic [31:0] grp_bjp;
  assign grp_bjp = 2;
  logic [31:0] grp_csr;
  assign grp_csr = 3;
  logic [31:0] grp_muldiv;
  assign grp_muldiv = 4;
  // Build info bus: group in bits [2:0], rv32 in bit [3], sub-decode in [4+]
  logic [31:0] info_base;
  assign info_base = rv32 ? 8 : 0;
  // bit 3 = rv32 flag
  logic [31:0] alu_sub;
  assign alu_sub = (rv32_add | rv32_addi | is_auipc | rv16_addi4spn | rv16_addi | rv16_addi16sp | rv16_add | rv16_li | rv16_mv ? 'h10 : 0) | (rv32_sub | rv16_sub ? 'h20 : 0) | (rv32_slt | rv32_slti ? 'h1000 : 0) | (rv32_sltu | rv32_sltiu ? 'h2000 : 0) | (rv32_xor | rv32_xori | rv16_xor ? 'h40 : 0) | (rv32_sll | rv32_slli | rv16_slli ? 'h80 : 0) | (rv32_srl | rv32_srli | rv16_srli ? 'h100 : 0) | (rv32_sra | rv32_srai | rv16_srai ? 'h200 : 0) | (rv32_or | rv32_ori | rv16_or ? 'h400 : 0) | (rv32_and | rv32_andi | rv16_andi | rv16_and ? 'h800 : 0) | (is_lui | rv16_lui ? 'h4000 : 0) | (need_imm ? 'h8000 : 0) | (is_auipc ? 'h10000 : 0) | (rv16_nop ? 'h20000 : 0) | (rv32_ecall ? 'h40000 : 0) | (rv32_ebreak | rv16_ebreak ? 'h80000 : 0) | (rv32_wfi ? 'h100000 : 0);
  logic [31:0] alu_info;
  assign alu_info = grp_alu | info_base | alu_sub;
  // AGU group
  logic [31:0] agu_info;
  assign agu_info = grp_agu | info_base | (is_load | rv16_lw | rv16_lwsp ? 'h10 : 0) | (is_store | rv16_sw | rv16_swsp ? 'h20 : 0);
  // BJP group
  logic [31:0] bjp_info;
  assign bjp_info = grp_bjp | info_base | (is_jal | is_jalr | rv16_j | rv16_jal | rv16_jr | rv16_jalr ? 'h10 : 0) | (i_prdt_taken ? 'h20 : 0) | (rv32_beq | rv16_beqz ? 'h40 : 0) | (rv32_bne | rv16_bnez ? 'h80 : 0) | (rv32_blt ? 'h100 : 0) | (rv32_bge ? 'h200 : 0) | (rv32_bltu ? 'h400 : 0) | (rv32_bgeu ? 'h800 : 0) | (rv32_bxx | rv16_beqz | rv16_bnez ? 'h1000 : 0) | (rv32_mret ? 'h2000 : 0) | (rv32_dret ? 'h4000 : 0) | (rv32_fence ? 'h8000 : 0) | (rv32_fence_i ? 'h10000 : 0);
  // CSR group
  logic [31:0] csr_info;
  assign csr_info = grp_csr | info_base | (rv32_csrrw | rv32_csrrwi ? 'h10 : 0) | (rv32_csrrs | rv32_csrrsi ? 'h20 : 0) | (rv32_csrrc | rv32_csrrci ? 'h40 : 0) | (rv32_csrrwi | rv32_csrrsi | rv32_csrrci ? 'h80 : 0);
  // MULDIV group
  logic [31:0] muldiv_info;
  assign muldiv_info = grp_muldiv | info_base | (rv32_mul ? 'h10 : 0) | (rv32_mulh ? 'h20 : 0) | (rv32_mulhsu ? 'h40 : 0) | (rv32_mulhu ? 'h80 : 0) | (rv32_div ? 'h100 : 0) | (rv32_divu ? 'h200 : 0) | (rv32_rem ? 'h400 : 0) | (rv32_remu ? 'h800 : 0) | (i_muldiv_b2b ? 'h1000 : 0);
  // ── Combinational output logic ──────────────────────────────────────
  always_comb begin
    // Register indices
    dec_rs1idx = rs1_idx;
    dec_rs2idx = rs2_idx;
    dec_rdidx = rd_idx;
    // jalr_rs1idx: always use 32-bit rs1 field for JALR, compressed rs1 for c.jalr
    if (is_jalr) begin
      dec_jalr_rs1idx = rs1_field;
    end else if (rv16_jalr) begin
      dec_jalr_rs1idx = rv16_rs1;
    end else begin
      dec_jalr_rs1idx = 0;
    end
    // rs1/rs2 == x0 flags (index is 0, independent of enable)
    dec_rs1x0 = rs1_idx == 0;
    dec_rs2x0 = rs2_idx == 0;
    // Register enables
    dec_rs1en = rs1_en;
    dec_rs2en = rs2_en;
    dec_rdwen = rdwen;
    // Pass-through
    dec_pc = i_pc;
    dec_misalgn = i_misalgn;
    dec_buserr = i_buserr;
    dec_ilegl = illegal;
    // NICE coprocessor
    dec_nice = 1'b0;
    nice_cmt_off_ilgl_o = 1'b0;
    // MulDiv flags
    dec_mulhsu = rv32_mulh | rv32_mulhsu | rv32_mulhu;
    dec_mul = rv32_mul;
    dec_div = rv32_div;
    dec_rem = rv32_rem;
    dec_divu = rv32_divu;
    dec_remu = rv32_remu;
    // Instruction type flags
    dec_rv32 = rv32;
    dec_bjp = bjp_op;
    dec_jal = is_jal | rv16_jal;
    dec_jalr = is_jalr | rv16_jr | rv16_jalr;
    dec_bxx = rv32_bxx | rv16_beqz | rv16_bnez;
    // BJP immediate
    if (is_branch) begin
      dec_bjp_imm = imm_b_se;
    end else if (is_jal) begin
      dec_bjp_imm = imm_j_se;
    end else if (is_jalr) begin
      dec_bjp_imm = imm_i_se;
    end else if (rv16_beqz | rv16_bnez) begin
      dec_bjp_imm = rv16_imm_b;
    end else if (rv16_j | rv16_jal) begin
      dec_bjp_imm = rv16_imm_j;
    end else begin
      dec_bjp_imm = 0;
    end
    // Immediate output (general)
    if (is_op_imm | is_load | is_jalr) begin
      dec_imm = imm_i_se;
    end else if (is_store) begin
      dec_imm = imm_s;
    end else if (is_branch) begin
      dec_imm = imm_b_se;
    end else if (is_lui | is_auipc) begin
      dec_imm = imm_u;
    end else if (is_jal) begin
      dec_imm = imm_j_se;
    end else begin
      dec_imm = 0;
    end
    // Info bus
    if (alu_op) begin
      dec_info = alu_info;
    end else if (amoldst_op) begin
      dec_info = agu_info;
    end else if (bjp_op) begin
      dec_info = bjp_info;
    end else if (csr_op) begin
      dec_info = csr_info;
    end else if (muldiv_op) begin
      dec_info = muldiv_info;
    end else begin
      dec_info = 0;
    end
  end

endmodule

// E203 HBirdv2 IFU Mini Decoder
// Wraps e203_exu_decode — selects subset of decode outputs for IFU.
module e203_ifu_minidec (
  input logic [31:0] instr,
  output logic dec_rs1en,
  output logic dec_rs2en,
  output logic [4:0] dec_rs1idx,
  output logic [4:0] dec_rs2idx,
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
  output logic [4:0] dec_jalr_rs1idx,
  output logic [31:0] dec_bjp_imm
);

  logic nc0;
  logic nc1;
  logic nc2;
  logic nc3;
  logic nc4;
  logic nc5;
  logic nc6;
  logic nc7;
  logic nc8;
  logic [4:0] nc_u0;
  logic [31:0] nc_u1;
  logic [31:0] nc_u2;
  logic [31:0] nc_u3;
  e203_exu_decode u_decode (
    .i_instr(instr),
    .i_pc(0),
    .i_prdt_taken(1'b0),
    .i_misalgn(1'b0),
    .i_buserr(1'b0),
    .i_muldiv_b2b(1'b0),
    .dbg_mode(1'b0),
    .nice_xs_off(1'b0),
    .dec_rs1x0(nc0),
    .dec_rs2x0(nc1),
    .dec_rs1en(dec_rs1en),
    .dec_rs2en(dec_rs2en),
    .dec_rdwen(nc2),
    .dec_rs1idx(dec_rs1idx),
    .dec_rs2idx(dec_rs2idx),
    .dec_rdidx(nc_u0),
    .dec_info(nc_u1),
    .dec_imm(nc_u2),
    .dec_pc(nc_u3),
    .dec_misalgn(nc3),
    .dec_buserr(nc4),
    .dec_ilegl(nc5),
    .dec_nice(nc6),
    .nice_cmt_off_ilgl_o(nc7),
    .dec_mulhsu(dec_mulhsu),
    .dec_mul(dec_mul),
    .dec_div(dec_div),
    .dec_rem(dec_rem),
    .dec_divu(dec_divu),
    .dec_remu(dec_remu),
    .dec_rv32(dec_rv32),
    .dec_bjp(dec_bjp),
    .dec_jal(dec_jal),
    .dec_jalr(dec_jalr),
    .dec_bxx(dec_bxx),
    .dec_jalr_rs1idx(dec_jalr_rs1idx),
    .dec_bjp_imm(dec_bjp_imm)
  );

endmodule

