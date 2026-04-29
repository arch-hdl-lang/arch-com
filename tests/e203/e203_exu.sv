// E203 HBirdv2 Instruction Decode Unit
// Full RV32IMC decoder matching RealBench e203_exu_decode_ref.sv exactly.
// Supports RV32 and RV16C (compressed) with info bus packing per group.
module e203_exu_decode #(
  parameter int XLEN = 32
) (
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
  output logic [31:0] dec_bjp_imm,
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
  // ── Per-group control outputs (for integration module) ─────────────
  // ── RV32/RV16 instruction extraction ────────────────────────────────
  logic [31:0] rv32_instr;
  assign rv32_instr = i_instr;
  logic [15:0] rv16_instr;
  assign rv16_instr = i_instr[15:0];
  logic [6:0] opcode;
  assign opcode = rv32_instr[6:0];
  logic [4:0] rv32_rd;
  assign rv32_rd = rv32_instr[11:7];
  logic [2:0] rv32_f3;
  assign rv32_f3 = rv32_instr[14:12];
  logic [4:0] rv32_rs1;
  assign rv32_rs1 = rv32_instr[19:15];
  logic [4:0] rv32_rs2;
  assign rv32_rs2 = rv32_instr[24:20];
  logic [6:0] rv32_f7;
  assign rv32_f7 = rv32_instr[31:25];
  logic [4:0] rv16_rd;
  assign rv16_rd = rv32_rd;
  logic [4:0] rv16_rs1;
  assign rv16_rs1 = rv16_rd;
  logic [4:0] rv16_rs2;
  assign rv16_rs2 = rv32_instr[6:2];
  logic [4:0] rv16_rdd;
  assign rv16_rdd = {2'd1, rv32_instr[4:2]};
  logic [4:0] rv16_rss1;
  assign rv16_rss1 = {2'd1, rv32_instr[9:7]};
  logic [4:0] rv16_rss2;
  assign rv16_rss2 = rv16_rdd;
  logic [2:0] rv16_f3;
  assign rv16_f3 = rv32_instr[15:13];
  // ── RV32 detection ──────────────────────────────────────────────────
  logic opcode_1_0_00;
  assign opcode_1_0_00 = opcode[1:0] == 0;
  logic opcode_1_0_01;
  assign opcode_1_0_01 = opcode[1:0] == 1;
  logic opcode_1_0_10;
  assign opcode_1_0_10 = opcode[1:0] == 2;
  logic opcode_1_0_11;
  assign opcode_1_0_11 = opcode[1:0] == 3;
  logic rv32;
  assign rv32 = ~(i_instr[4:2] == 7) & opcode_1_0_11;
  // ── opcode[4:2] groups ──────────────────────────────────────────────
  logic opcode_4_2_000;
  assign opcode_4_2_000 = opcode[4:2] == 0;
  logic opcode_4_2_001;
  assign opcode_4_2_001 = opcode[4:2] == 1;
  logic opcode_4_2_010;
  assign opcode_4_2_010 = opcode[4:2] == 2;
  logic opcode_4_2_011;
  assign opcode_4_2_011 = opcode[4:2] == 3;
  logic opcode_4_2_100;
  assign opcode_4_2_100 = opcode[4:2] == 4;
  logic opcode_4_2_101;
  assign opcode_4_2_101 = opcode[4:2] == 5;
  logic opcode_4_2_110;
  assign opcode_4_2_110 = opcode[4:2] == 6;
  logic opcode_4_2_111;
  assign opcode_4_2_111 = opcode[4:2] == 7;
  logic opcode_6_5_00;
  assign opcode_6_5_00 = opcode[6:5] == 0;
  logic opcode_6_5_01;
  assign opcode_6_5_01 = opcode[6:5] == 1;
  logic opcode_6_5_10;
  assign opcode_6_5_10 = opcode[6:5] == 2;
  logic opcode_6_5_11;
  assign opcode_6_5_11 = opcode[6:5] == 3;
  // ── RV32 instruction classes ────────────────────────────────────────
  logic rv32_load;
  assign rv32_load = opcode_6_5_00 & opcode_4_2_000 & opcode_1_0_11;
  logic rv32_store;
  assign rv32_store = opcode_6_5_01 & opcode_4_2_000 & opcode_1_0_11;
  logic rv32_branch;
  assign rv32_branch = opcode_6_5_11 & opcode_4_2_000 & opcode_1_0_11;
  logic rv32_jalr;
  assign rv32_jalr = opcode_6_5_11 & opcode_4_2_001 & opcode_1_0_11;
  logic rv32_miscmem;
  assign rv32_miscmem = opcode_6_5_00 & opcode_4_2_011 & opcode_1_0_11;
  logic rv32_amo;
  assign rv32_amo = opcode_6_5_01 & opcode_4_2_011 & opcode_1_0_11;
  logic rv32_jal;
  assign rv32_jal = opcode_6_5_11 & opcode_4_2_011 & opcode_1_0_11;
  logic rv32_op_imm;
  assign rv32_op_imm = opcode_6_5_00 & opcode_4_2_100 & opcode_1_0_11;
  logic rv32_op;
  assign rv32_op = opcode_6_5_01 & opcode_4_2_100 & opcode_1_0_11;
  logic rv32_system;
  assign rv32_system = opcode_6_5_11 & opcode_4_2_100 & opcode_1_0_11;
  logic rv32_auipc;
  assign rv32_auipc = opcode_6_5_00 & opcode_4_2_101 & opcode_1_0_11;
  logic rv32_lui;
  assign rv32_lui = opcode_6_5_01 & opcode_4_2_101 & opcode_1_0_11;
  logic rv32_custom0;
  assign rv32_custom0 = opcode_6_5_00 & opcode_4_2_010 & opcode_1_0_11;
  logic rv32_custom1;
  assign rv32_custom1 = opcode_6_5_01 & opcode_4_2_010 & opcode_1_0_11;
  logic rv32_custom2;
  assign rv32_custom2 = opcode_6_5_10 & opcode_4_2_110 & opcode_1_0_11;
  logic rv32_custom3;
  assign rv32_custom3 = opcode_6_5_11 & opcode_4_2_110 & opcode_1_0_11;
  // ── funct3 decode ───────────────────────────────────────────────────
  logic rv32_f3_000;
  assign rv32_f3_000 = rv32_f3 == 0;
  logic rv32_f3_001;
  assign rv32_f3_001 = rv32_f3 == 1;
  logic rv32_f3_010;
  assign rv32_f3_010 = rv32_f3 == 2;
  logic rv32_f3_011;
  assign rv32_f3_011 = rv32_f3 == 3;
  logic rv32_f3_100;
  assign rv32_f3_100 = rv32_f3 == 4;
  logic rv32_f3_101;
  assign rv32_f3_101 = rv32_f3 == 5;
  logic rv32_f3_110;
  assign rv32_f3_110 = rv32_f3 == 6;
  logic rv32_f3_111;
  assign rv32_f3_111 = rv32_f3 == 7;
  logic rv16_f3_000;
  assign rv16_f3_000 = rv16_f3 == 0;
  logic rv16_f3_001;
  assign rv16_f3_001 = rv16_f3 == 1;
  logic rv16_f3_010;
  assign rv16_f3_010 = rv16_f3 == 2;
  logic rv16_f3_011;
  assign rv16_f3_011 = rv16_f3 == 3;
  logic rv16_f3_100;
  assign rv16_f3_100 = rv16_f3 == 4;
  logic rv16_f3_101;
  assign rv16_f3_101 = rv16_f3 == 5;
  logic rv16_f3_110;
  assign rv16_f3_110 = rv16_f3 == 6;
  logic rv16_f3_111;
  assign rv16_f3_111 = rv16_f3 == 7;
  // ── funct7 decode ───────────────────────────────────────────────────
  logic rv32_f7_0000000;
  assign rv32_f7_0000000 = rv32_f7 == 'h0;
  logic rv32_f7_0100000;
  assign rv32_f7_0100000 = rv32_f7 == 'h20;
  logic rv32_f7_0000001;
  assign rv32_f7_0000001 = rv32_f7 == 'h1;
  // ── Register zero/x31 flags ─────────────────────────────────────────
  logic rv32_rs1_x0;
  assign rv32_rs1_x0 = rv32_rs1 == 0;
  logic rv32_rs2_x0;
  assign rv32_rs2_x0 = rv32_rs2 == 0;
  logic rv32_rd_x0;
  assign rv32_rd_x0 = rv32_rd == 0;
  logic rv32_rd_x2;
  assign rv32_rd_x2 = rv32_rd == 2;
  logic rv16_rs1_x0;
  assign rv16_rs1_x0 = rv16_rs1 == 0;
  logic rv16_rs2_x0;
  assign rv16_rs2_x0 = rv16_rs2 == 0;
  logic rv16_rd_x0;
  assign rv16_rd_x0 = rv16_rd == 0;
  logic rv16_rd_x2;
  assign rv16_rd_x2 = rv16_rd == 2;
  logic rv32_rs1_x31;
  assign rv32_rs1_x31 = rv32_rs1 == 31;
  logic rv32_rs2_x31;
  assign rv32_rs2_x31 = rv32_rs2 == 31;
  logic rv32_rd_x31;
  assign rv32_rd_x31 = rv32_rd == 31;
  // ── RV16C instruction classes ───────────────────────────────────────
  logic rv16_addi4spn;
  assign rv16_addi4spn = opcode_1_0_00 & rv16_f3_000;
  logic rv16_lw;
  assign rv16_lw = opcode_1_0_00 & rv16_f3_010;
  logic rv16_sw;
  assign rv16_sw = opcode_1_0_00 & rv16_f3_110;
  logic rv16_addi;
  assign rv16_addi = opcode_1_0_01 & rv16_f3_000;
  logic rv16_jal;
  assign rv16_jal = opcode_1_0_01 & rv16_f3_001;
  logic rv16_li;
  assign rv16_li = opcode_1_0_01 & rv16_f3_010;
  logic rv16_lui_addi16sp;
  assign rv16_lui_addi16sp = opcode_1_0_01 & rv16_f3_011;
  logic rv16_miscalu;
  assign rv16_miscalu = opcode_1_0_01 & rv16_f3_100;
  logic rv16_j;
  assign rv16_j = opcode_1_0_01 & rv16_f3_101;
  logic rv16_beqz;
  assign rv16_beqz = opcode_1_0_01 & rv16_f3_110;
  logic rv16_bnez;
  assign rv16_bnez = opcode_1_0_01 & rv16_f3_111;
  logic rv16_slli;
  assign rv16_slli = opcode_1_0_10 & rv16_f3_000;
  logic rv16_lwsp;
  assign rv16_lwsp = opcode_1_0_10 & rv16_f3_010;
  logic rv16_jalr_mv_add;
  assign rv16_jalr_mv_add = opcode_1_0_10 & rv16_f3_100;
  logic rv16_swsp;
  assign rv16_swsp = opcode_1_0_10 & rv16_f3_110;
  // ── RV16C sub-decodes ───────────────────────────────────────────────
  logic rv16_lwsp_ilgl;
  assign rv16_lwsp_ilgl = rv16_lwsp & rv16_rd_x0;
  logic rv16_nop;
  assign rv16_nop = rv16_addi & ~rv16_instr[12:12] & rv16_rd_x0 & rv16_rs2_x0;
  logic rv16_srli;
  assign rv16_srli = rv16_miscalu & (rv16_instr[11:10] == 0);
  logic rv16_srai;
  assign rv16_srai = rv16_miscalu & (rv16_instr[11:10] == 1);
  logic rv16_andi;
  assign rv16_andi = rv16_miscalu & (rv16_instr[11:10] == 2);
  logic rv16_instr_12_is0;
  assign rv16_instr_12_is0 = rv16_instr[12:12] == 0;
  logic rv16_instr_6_2_is0s;
  assign rv16_instr_6_2_is0s = rv16_instr[6:2] == 0;
  logic rv16_sxxi_shamt_legl;
  assign rv16_sxxi_shamt_legl = rv16_instr_12_is0 & ~rv16_instr_6_2_is0s;
  logic rv16_sxxi_shamt_ilgl;
  assign rv16_sxxi_shamt_ilgl = (rv16_slli | rv16_srli | rv16_srai) & ~rv16_sxxi_shamt_legl;
  logic rv16_addi16sp;
  assign rv16_addi16sp = rv16_lui_addi16sp & rv32_rd_x2;
  logic rv16_lui;
  assign rv16_lui = rv16_lui_addi16sp & ~rv32_rd_x0 & ~rv32_rd_x2;
  logic rv16_li_ilgl;
  assign rv16_li_ilgl = rv16_li & rv16_rd_x0;
  logic rv16_lui_ilgl;
  assign rv16_lui_ilgl = rv16_lui & (rv16_rd_x0 | rv16_rd_x2 | (rv16_instr_6_2_is0s & rv16_instr_12_is0));
  logic rv16_li_lui_ilgl;
  assign rv16_li_lui_ilgl = rv16_li_ilgl | rv16_lui_ilgl;
  logic rv16_addi4spn_ilgl;
  assign rv16_addi4spn_ilgl = rv16_addi4spn & rv16_instr_12_is0 & rv16_rd_x0 & opcode_6_5_00;
  logic rv16_addi16sp_ilgl;
  assign rv16_addi16sp_ilgl = rv16_addi16sp & rv16_instr_12_is0 & rv16_instr_6_2_is0s;
  logic rv16_subxororand;
  assign rv16_subxororand = rv16_miscalu & (rv16_instr[12:10] == 3);
  logic rv16_sub;
  assign rv16_sub = rv16_subxororand & (rv16_instr[6:5] == 0);
  logic rv16_xor;
  assign rv16_xor = rv16_subxororand & (rv16_instr[6:5] == 1);
  logic rv16_or;
  assign rv16_or = rv16_subxororand & (rv16_instr[6:5] == 2);
  logic rv16_and;
  assign rv16_and = rv16_subxororand & (rv16_instr[6:5] == 3);
  logic rv16_jr;
  assign rv16_jr = rv16_jalr_mv_add & ~rv16_instr[12:12] & ~rv16_rs1_x0 & rv16_rs2_x0;
  logic rv16_mv;
  assign rv16_mv = rv16_jalr_mv_add & ~rv16_instr[12:12] & ~rv16_rd_x0 & ~rv16_rs2_x0;
  logic rv16_ebreak;
  assign rv16_ebreak = rv16_jalr_mv_add & rv16_instr[12:12] & rv16_rd_x0 & rv16_rs2_x0;
  logic rv16_jalr;
  assign rv16_jalr = rv16_jalr_mv_add & rv16_instr[12:12] & ~rv16_rs1_x0 & rv16_rs2_x0;
  logic rv16_add;
  assign rv16_add = rv16_jalr_mv_add & rv16_instr[12:12] & ~rv16_rd_x0 & ~rv16_rs2_x0;
  // ── NICE decode ─────────────────────────────────────────────────────
  logic nice_need_rs1;
  assign nice_need_rs1 = rv32_instr[13:13];
  logic nice_need_rs2;
  assign nice_need_rs2 = rv32_instr[12:12];
  logic nice_need_rd;
  assign nice_need_rd = rv32_instr[14:14];
  logic [26:0] nice_instr;
  assign nice_instr = rv32_instr[31:5];
  logic nice_op;
  assign nice_op = rv32_custom0 | rv32_custom1 | rv32_custom2 | rv32_custom3;
  // ── Branch instructions ─────────────────────────────────────────────
  logic rv32_beq;
  assign rv32_beq = rv32_branch & rv32_f3_000;
  logic rv32_bne;
  assign rv32_bne = rv32_branch & rv32_f3_001;
  logic rv32_blt;
  assign rv32_blt = rv32_branch & rv32_f3_100;
  logic rv32_bgt;
  assign rv32_bgt = rv32_branch & rv32_f3_101;
  logic rv32_bltu;
  assign rv32_bltu = rv32_branch & rv32_f3_110;
  logic rv32_bgtu;
  assign rv32_bgtu = rv32_branch & rv32_f3_111;
  // ── System instructions ─────────────────────────────────────────────
  logic rv32_ecall;
  assign rv32_ecall = rv32_system & rv32_f3_000 & (rv32_instr[31:20] == 'h0);
  logic rv32_ebreak;
  assign rv32_ebreak = rv32_system & rv32_f3_000 & (rv32_instr[31:20] == 'h1);
  logic rv32_mret;
  assign rv32_mret = rv32_system & rv32_f3_000 & (rv32_instr[31:20] == 'h302);
  logic rv32_dret;
  assign rv32_dret = rv32_system & rv32_f3_000 & (rv32_instr[31:20] == 'h7B2);
  logic rv32_wfi;
  assign rv32_wfi = rv32_system & rv32_f3_000 & (rv32_instr[31:20] == 'h105);
  logic rv32_ecall_ebreak_ret_wfi;
  assign rv32_ecall_ebreak_ret_wfi = rv32_system & rv32_f3_000;
  logic rv32_csrrw;
  assign rv32_csrrw = rv32_system & rv32_f3_001;
  logic rv32_csrrs;
  assign rv32_csrrs = rv32_system & rv32_f3_010;
  logic rv32_csrrc;
  assign rv32_csrrc = rv32_system & rv32_f3_011;
  logic rv32_csrrwi;
  assign rv32_csrrwi = rv32_system & rv32_f3_101;
  logic rv32_csrrsi;
  assign rv32_csrrsi = rv32_system & rv32_f3_110;
  logic rv32_csrrci;
  assign rv32_csrrci = rv32_system & rv32_f3_111;
  logic rv32_csr;
  assign rv32_csr = rv32_system & ~rv32_f3_000;
  logic rv32_dret_ilgl;
  assign rv32_dret_ilgl = rv32_dret & ~dbg_mode;
  // ── BJP group ───────────────────────────────────────────────────────
  logic dec_jal_b;
  assign dec_jal_b = rv32_jal | rv16_jal | rv16_j;
  logic dec_jalr_b;
  assign dec_jalr_b = rv32_jalr | rv16_jalr | rv16_jr;
  logic dec_bxx_b;
  assign dec_bxx_b = rv32_branch | rv16_beqz | rv16_bnez;
  logic dec_bjp_b;
  assign dec_bjp_b = dec_jal_b | dec_jalr_b | dec_bxx_b;
  // ── Fence ───────────────────────────────────────────────────────────
  logic rv32_fence;
  assign rv32_fence = rv32_miscmem & rv32_f3_000;
  logic rv32_fence_i;
  assign rv32_fence_i = rv32_miscmem & rv32_f3_001;
  logic rv32_fence_fencei;
  assign rv32_fence_fencei = rv32_miscmem;
  logic bjp_op;
  assign bjp_op = dec_bjp_b | rv32_mret | (rv32_dret & ~rv32_dret_ilgl) | rv32_fence_fencei;
  // ── ALU instructions ────────────────────────────────────────────────
  logic rv32_addi;
  assign rv32_addi = rv32_op_imm & rv32_f3_000;
  logic rv32_slti;
  assign rv32_slti = rv32_op_imm & rv32_f3_010;
  logic rv32_sltiu;
  assign rv32_sltiu = rv32_op_imm & rv32_f3_011;
  logic rv32_xori;
  assign rv32_xori = rv32_op_imm & rv32_f3_100;
  logic rv32_ori;
  assign rv32_ori = rv32_op_imm & rv32_f3_110;
  logic rv32_andi_b;
  assign rv32_andi_b = rv32_op_imm & rv32_f3_111;
  logic rv32_slli;
  assign rv32_slli = rv32_op_imm & rv32_f3_001 & (rv32_instr[31:26] == 0);
  logic rv32_srli;
  assign rv32_srli = rv32_op_imm & rv32_f3_101 & (rv32_instr[31:26] == 0);
  logic rv32_srai;
  assign rv32_srai = rv32_op_imm & rv32_f3_101 & (rv32_instr[31:26] == 'h10);
  logic rv32_sxxi_shamt_legl;
  assign rv32_sxxi_shamt_legl = rv32_instr[25:25] == 0;
  logic rv32_sxxi_shamt_ilgl;
  assign rv32_sxxi_shamt_ilgl = (rv32_slli | rv32_srli | rv32_srai) & ~rv32_sxxi_shamt_legl;
  logic rv32_add;
  assign rv32_add = rv32_op & rv32_f3_000 & rv32_f7_0000000;
  logic rv32_sub;
  assign rv32_sub = rv32_op & rv32_f3_000 & rv32_f7_0100000;
  logic rv32_sll;
  assign rv32_sll = rv32_op & rv32_f3_001 & rv32_f7_0000000;
  logic rv32_slt;
  assign rv32_slt = rv32_op & rv32_f3_010 & rv32_f7_0000000;
  logic rv32_sltu;
  assign rv32_sltu = rv32_op & rv32_f3_011 & rv32_f7_0000000;
  logic rv32_xor;
  assign rv32_xor = rv32_op & rv32_f3_100 & rv32_f7_0000000;
  logic rv32_srl;
  assign rv32_srl = rv32_op & rv32_f3_101 & rv32_f7_0000000;
  logic rv32_sra;
  assign rv32_sra = rv32_op & rv32_f3_101 & rv32_f7_0100000;
  logic rv32_or;
  assign rv32_or = rv32_op & rv32_f3_110 & rv32_f7_0000000;
  logic rv32_and;
  assign rv32_and = rv32_op & rv32_f3_111 & rv32_f7_0000000;
  logic rv32_nop;
  assign rv32_nop = rv32_addi & rv32_rs1_x0 & rv32_rd_x0 & ~|rv32_instr[31:20];
  logic ecall_ebreak;
  assign ecall_ebreak = rv32_ecall | rv32_ebreak | rv16_ebreak;
  logic alu_op;
  assign alu_op = ~rv32_sxxi_shamt_ilgl & ~rv16_sxxi_shamt_ilgl & ~rv16_li_lui_ilgl & ~rv16_addi4spn_ilgl & ~rv16_addi16sp_ilgl & (rv32_op_imm | (rv32_op & ~rv32_f7_0000001) | rv32_auipc | rv32_lui | rv16_addi4spn | rv16_addi | rv16_lui_addi16sp | rv16_li | rv16_mv | rv16_slli | rv16_miscalu | rv16_add | rv16_nop | rv32_nop | rv32_wfi | ecall_ebreak);
  // ── MulDiv ──────────────────────────────────────────────────────────
  logic rv32_mul;
  assign rv32_mul = rv32_op & rv32_f3_000 & rv32_f7_0000001;
  logic rv32_mulh;
  assign rv32_mulh = rv32_op & rv32_f3_001 & rv32_f7_0000001;
  logic rv32_mulhsu;
  assign rv32_mulhsu = rv32_op & rv32_f3_010 & rv32_f7_0000001;
  logic rv32_mulhu;
  assign rv32_mulhu = rv32_op & rv32_f3_011 & rv32_f7_0000001;
  logic rv32_div;
  assign rv32_div = rv32_op & rv32_f3_100 & rv32_f7_0000001;
  logic rv32_divu;
  assign rv32_divu = rv32_op & rv32_f3_101 & rv32_f7_0000001;
  logic rv32_rem;
  assign rv32_rem = rv32_op & rv32_f3_110 & rv32_f7_0000001;
  logic rv32_remu;
  assign rv32_remu = rv32_op & rv32_f3_111 & rv32_f7_0000001;
  logic muldiv_op;
  assign muldiv_op = rv32_op & rv32_f7_0000001;
  // ── Load/Store ──────────────────────────────────────────────────────
  logic rv32_lb;
  assign rv32_lb = rv32_load & rv32_f3_000;
  logic rv32_lh;
  assign rv32_lh = rv32_load & rv32_f3_001;
  logic rv32_lw;
  assign rv32_lw = rv32_load & rv32_f3_010;
  logic rv32_lbu;
  assign rv32_lbu = rv32_load & rv32_f3_100;
  logic rv32_lhu;
  assign rv32_lhu = rv32_load & rv32_f3_101;
  logic rv32_sb;
  assign rv32_sb = rv32_store & rv32_f3_000;
  logic rv32_sh;
  assign rv32_sh = rv32_store & rv32_f3_001;
  logic rv32_sw;
  assign rv32_sw = rv32_store & rv32_f3_010;
  // ── AMO ─────────────────────────────────────────────────────────────
  logic rv32_lr_w;
  assign rv32_lr_w = rv32_amo & rv32_f3_010 & (rv32_f7[6:2] == 2);
  logic rv32_sc_w;
  assign rv32_sc_w = rv32_amo & rv32_f3_010 & (rv32_f7[6:2] == 3);
  logic rv32_amoswap_w;
  assign rv32_amoswap_w = rv32_amo & rv32_f3_010 & (rv32_f7[6:2] == 1);
  logic rv32_amoadd_w;
  assign rv32_amoadd_w = rv32_amo & rv32_f3_010 & (rv32_f7[6:2] == 0);
  logic rv32_amoxor_w;
  assign rv32_amoxor_w = rv32_amo & rv32_f3_010 & (rv32_f7[6:2] == 4);
  logic rv32_amoand_w;
  assign rv32_amoand_w = rv32_amo & rv32_f3_010 & (rv32_f7[6:2] == 12);
  logic rv32_amoor_w;
  assign rv32_amoor_w = rv32_amo & rv32_f3_010 & (rv32_f7[6:2] == 8);
  logic rv32_amomin_w;
  assign rv32_amomin_w = rv32_amo & rv32_f3_010 & (rv32_f7[6:2] == 16);
  logic rv32_amomax_w;
  assign rv32_amomax_w = rv32_amo & rv32_f3_010 & (rv32_f7[6:2] == 20);
  logic rv32_amominu_w;
  assign rv32_amominu_w = rv32_amo & rv32_f3_010 & (rv32_f7[6:2] == 24);
  logic rv32_amomaxu_w;
  assign rv32_amomaxu_w = rv32_amo & rv32_f3_010 & (rv32_f7[6:2] == 28);
  logic amoldst_op;
  assign amoldst_op = rv32_amo | rv32_load | rv32_store | rv16_lw | rv16_sw | (rv16_lwsp & ~rv16_lwsp_ilgl) | rv16_swsp;
  logic [1:0] lsu_info_size;
  assign lsu_info_size = rv32 ? rv32_f3[1:0] : 2;
  logic lsu_info_usign;
  assign lsu_info_usign = rv32 ? rv32_f3[2:2] : 1'b0;
  // ── Register need flags ─────────────────────────────────────────────
  logic rv32_need_rd;
  assign rv32_need_rd = ~rv32_rd_x0 & (nice_op ? nice_need_rd : ~rv32_branch & ~rv32_store & ~rv32_fence_fencei & ~rv32_ecall_ebreak_ret_wfi);
  logic rv32_need_rs1;
  assign rv32_need_rs1 = ~rv32_rs1_x0 & (nice_op ? nice_need_rs1 : ~rv32_lui & ~rv32_auipc & ~rv32_jal & ~rv32_fence_fencei & ~rv32_ecall_ebreak_ret_wfi & ~rv32_csrrwi & ~rv32_csrrsi & ~rv32_csrrci);
  logic rv32_need_rs2;
  assign rv32_need_rs2 = ~rv32_rs2_x0 & (nice_op ? nice_need_rs2 : rv32_branch | rv32_store | rv32_op | (rv32_amo & ~rv32_lr_w));
  // ── Immediates ──────────────────────────────────────────────────────
  // RV32 I-type
  logic [31:0] rv32_i_imm;
  assign rv32_i_imm = {{20{rv32_instr[31:31]}}, rv32_instr[31:20]};
  // RV32 S-type
  logic [31:0] rv32_s_imm;
  assign rv32_s_imm = {{20{rv32_instr[31:31]}}, rv32_instr[31:25], rv32_instr[11:7]};
  // RV32 B-type
  logic [31:0] rv32_b_imm;
  assign rv32_b_imm = {{19{rv32_instr[31:31]}}, rv32_instr[31:31], rv32_instr[7:7], rv32_instr[30:25], rv32_instr[11:8], 1'd0};
  // RV32 U-type
  logic [31:0] rv32_u_imm;
  assign rv32_u_imm = {rv32_instr[31:12], 12'd0};
  // RV32 J-type
  logic [31:0] rv32_j_imm;
  assign rv32_j_imm = {{11{rv32_instr[31:31]}}, rv32_instr[31:31], rv32_instr[19:12], rv32_instr[20:20], rv32_instr[30:21], 1'd0};
  // immediate select
  logic rv32_imm_sel_i;
  assign rv32_imm_sel_i = rv32_op_imm | rv32_jalr | rv32_load;
  logic rv32_imm_sel_u;
  assign rv32_imm_sel_u = rv32_lui | rv32_auipc;
  logic rv32_imm_sel_j;
  assign rv32_imm_sel_j = rv32_jal;
  logic rv32_imm_sel_b;
  assign rv32_imm_sel_b = rv32_branch;
  logic rv32_imm_sel_s;
  assign rv32_imm_sel_s = rv32_store;
  logic rv32_need_imm;
  assign rv32_need_imm = rv32_imm_sel_i | rv32_imm_sel_s | rv32_imm_sel_b | rv32_imm_sel_u | rv32_imm_sel_j;
  logic [31:0] rv32_imm;
  assign rv32_imm = (rv32_imm_sel_i ? rv32_i_imm : 0) | (rv32_imm_sel_s ? rv32_s_imm : 0) | (rv32_imm_sel_b ? rv32_b_imm : 0) | (rv32_imm_sel_u ? rv32_u_imm : 0) | (rv32_imm_sel_j ? rv32_j_imm : 0);
  // ── RV16 immediates ─────────────────────────────────────────────────
  logic [31:0] rv16_cis_imm;
  assign rv16_cis_imm = {24'd0, rv16_instr[3:2], rv16_instr[12:12], rv16_instr[6:4], 2'd0};
  logic [31:0] rv16_cili_imm;
  assign rv16_cili_imm = {{26{rv16_instr[12:12]}}, rv16_instr[12:12], rv16_instr[6:2]};
  logic [31:0] rv16_cilui_imm;
  assign rv16_cilui_imm = {{14{rv16_instr[12:12]}}, rv16_instr[12:12], rv16_instr[6:2], 12'd0};
  logic [31:0] rv16_ci16sp_imm;
  assign rv16_ci16sp_imm = {{22{rv16_instr[12:12]}}, rv16_instr[12:12], rv16_instr[4:4], rv16_instr[3:3], rv16_instr[5:5], rv16_instr[2:2], rv16_instr[6:6], 4'd0};
  logic [31:0] rv16_css_imm;
  assign rv16_css_imm = {24'd0, rv16_instr[8:7], rv16_instr[12:9], 2'd0};
  logic [31:0] rv16_ciw_imm;
  assign rv16_ciw_imm = {22'd0, rv16_instr[10:7], rv16_instr[12:12], rv16_instr[11:11], rv16_instr[5:5], rv16_instr[6:6], 2'd0};
  logic [31:0] rv16_cl_imm;
  assign rv16_cl_imm = {25'd0, rv16_instr[5:5], rv16_instr[12:12], rv16_instr[11:11], rv16_instr[10:10], rv16_instr[6:6], 2'd0};
  logic [31:0] rv16_cs_imm;
  assign rv16_cs_imm = {25'd0, rv16_instr[5:5], rv16_instr[12:12], rv16_instr[11:11], rv16_instr[10:10], rv16_instr[6:6], 2'd0};
  logic [31:0] rv16_cb_imm;
  assign rv16_cb_imm = {{23{rv16_instr[12:12]}}, rv16_instr[12:12], rv16_instr[6:5], rv16_instr[2:2], rv16_instr[11:10], rv16_instr[4:3], 1'd0};
  logic [31:0] rv16_cj_imm;
  assign rv16_cj_imm = {{20{rv16_instr[12:12]}}, rv16_instr[12:12], rv16_instr[8:8], rv16_instr[10:9], rv16_instr[6:6], rv16_instr[7:7], rv16_instr[2:2], rv16_instr[11:11], rv16_instr[5:3], 1'd0};
  // RV16 immediate selects
  logic rv16_imm_sel_cis;
  assign rv16_imm_sel_cis = rv16_lwsp;
  logic rv16_imm_sel_cili;
  assign rv16_imm_sel_cili = rv16_li | rv16_addi | rv16_slli | rv16_srai | rv16_srli | rv16_andi;
  logic rv16_imm_sel_cilui;
  assign rv16_imm_sel_cilui = rv16_lui;
  logic rv16_imm_sel_ci16sp;
  assign rv16_imm_sel_ci16sp = rv16_addi16sp;
  logic rv16_imm_sel_css;
  assign rv16_imm_sel_css = rv16_swsp;
  logic rv16_imm_sel_ciw;
  assign rv16_imm_sel_ciw = rv16_addi4spn;
  logic rv16_imm_sel_cl;
  assign rv16_imm_sel_cl = rv16_lw;
  logic rv16_imm_sel_cs;
  assign rv16_imm_sel_cs = rv16_sw;
  logic rv16_imm_sel_cb;
  assign rv16_imm_sel_cb = rv16_beqz | rv16_bnez;
  logic rv16_imm_sel_cj;
  assign rv16_imm_sel_cj = rv16_j | rv16_jal;
  logic rv16_need_imm;
  assign rv16_need_imm = rv16_imm_sel_cis | rv16_imm_sel_cili | rv16_imm_sel_cilui | rv16_imm_sel_ci16sp | rv16_imm_sel_css | rv16_imm_sel_ciw | rv16_imm_sel_cl | rv16_imm_sel_cs | rv16_imm_sel_cb | rv16_imm_sel_cj;
  logic [31:0] rv16_imm;
  assign rv16_imm = (rv16_imm_sel_cis ? rv16_cis_imm : 0) | (rv16_imm_sel_cili ? rv16_cili_imm : 0) | (rv16_imm_sel_cilui ? rv16_cilui_imm : 0) | (rv16_imm_sel_ci16sp ? rv16_ci16sp_imm : 0) | (rv16_imm_sel_css ? rv16_css_imm : 0) | (rv16_imm_sel_ciw ? rv16_ciw_imm : 0) | (rv16_imm_sel_cl ? rv16_cl_imm : 0) | (rv16_imm_sel_cs ? rv16_cs_imm : 0) | (rv16_imm_sel_cb ? rv16_cb_imm : 0) | (rv16_imm_sel_cj ? rv16_cj_imm : 0);
  // ── need_imm (shared by ALU and AGU info buses) ─────────────────────
  logic need_imm;
  assign need_imm = rv32 ? rv32_need_imm : rv16_need_imm;
  // ── RV16C register file format groups ───────────────────────────────
  logic rv16_format_cr;
  assign rv16_format_cr = rv16_jalr_mv_add;
  logic rv16_format_ci;
  assign rv16_format_ci = rv16_lwsp | rv16_li | rv16_lui_addi16sp | rv16_addi | rv16_slli;
  logic rv16_format_css;
  assign rv16_format_css = rv16_swsp;
  logic rv16_format_ciw;
  assign rv16_format_ciw = rv16_addi4spn;
  logic rv16_format_cl;
  assign rv16_format_cl = rv16_lw;
  logic rv16_format_cs;
  assign rv16_format_cs = rv16_sw | rv16_subxororand;
  logic rv16_format_cb;
  assign rv16_format_cb = rv16_beqz | rv16_bnez | rv16_srli | rv16_srai | rv16_andi;
  logic rv16_format_cj;
  assign rv16_format_cj = rv16_j | rv16_jal;
  // CR format: JR/JALR/MV/ADD/EBREAK
  logic rv16_need_cr_rs1;
  assign rv16_need_cr_rs1 = rv16_format_cr & 1'b1;
  logic rv16_need_cr_rs2;
  assign rv16_need_cr_rs2 = rv16_format_cr & 1'b1;
  logic rv16_need_cr_rd;
  assign rv16_need_cr_rd = rv16_format_cr & 1'b1;
  logic [4:0] rv16_cr_rs1;
  assign rv16_cr_rs1 = rv16_mv ? 0 : rv16_rs1;
  logic [4:0] rv16_cr_rs2;
  assign rv16_cr_rs2 = rv16_rs2;
  logic [4:0] rv16_cr_rd;
  assign rv16_cr_rd = rv16_jalr | rv16_jr ? {4'd0, rv16_instr[12:12]} : rv16_rd;
  // CI format
  logic rv16_need_ci_rs1;
  assign rv16_need_ci_rs1 = rv16_format_ci & 1'b1;
  logic rv16_need_ci_rs2;
  assign rv16_need_ci_rs2 = rv16_format_ci & 1'b0;
  logic rv16_need_ci_rd;
  assign rv16_need_ci_rd = rv16_format_ci & 1'b1;
  logic [4:0] rv16_ci_rs1;
  assign rv16_ci_rs1 = rv16_lwsp ? 2 : rv16_li | rv16_lui ? 0 : rv16_rs1;
  logic [4:0] rv16_ci_rs2;
  assign rv16_ci_rs2 = 0;
  logic [4:0] rv16_ci_rd;
  assign rv16_ci_rd = rv16_rd;
  // CSS format
  logic rv16_need_css_rs1;
  assign rv16_need_css_rs1 = rv16_format_css & 1'b1;
  logic rv16_need_css_rs2;
  assign rv16_need_css_rs2 = rv16_format_css & 1'b1;
  logic rv16_need_css_rd;
  assign rv16_need_css_rd = rv16_format_css & 1'b0;
  logic [4:0] rv16_css_rs1;
  assign rv16_css_rs1 = 2;
  logic [4:0] rv16_css_rs2;
  assign rv16_css_rs2 = rv16_rs2;
  logic [4:0] rv16_css_rd;
  assign rv16_css_rd = 0;
  // CIW format
  logic rv16_need_ciw_rss1;
  assign rv16_need_ciw_rss1 = rv16_format_ciw & 1'b1;
  logic rv16_need_ciw_rss2;
  assign rv16_need_ciw_rss2 = rv16_format_ciw & 1'b0;
  logic rv16_need_ciw_rdd;
  assign rv16_need_ciw_rdd = rv16_format_ciw & 1'b1;
  logic [4:0] rv16_ciw_rss1;
  assign rv16_ciw_rss1 = 2;
  logic [4:0] rv16_ciw_rss2;
  assign rv16_ciw_rss2 = 0;
  logic [4:0] rv16_ciw_rdd;
  assign rv16_ciw_rdd = rv16_rdd;
  // CL format
  logic rv16_need_cl_rss1;
  assign rv16_need_cl_rss1 = rv16_format_cl & 1'b1;
  logic rv16_need_cl_rss2;
  assign rv16_need_cl_rss2 = rv16_format_cl & 1'b0;
  logic rv16_need_cl_rdd;
  assign rv16_need_cl_rdd = rv16_format_cl & 1'b1;
  logic [4:0] rv16_cl_rss1;
  assign rv16_cl_rss1 = rv16_rss1;
  logic [4:0] rv16_cl_rss2;
  assign rv16_cl_rss2 = 0;
  logic [4:0] rv16_cl_rdd;
  assign rv16_cl_rdd = rv16_rdd;
  // CS format
  logic rv16_need_cs_rss1;
  assign rv16_need_cs_rss1 = rv16_format_cs & 1'b1;
  logic rv16_need_cs_rss2;
  assign rv16_need_cs_rss2 = rv16_format_cs & 1'b1;
  logic rv16_need_cs_rdd;
  assign rv16_need_cs_rdd = rv16_format_cs & rv16_subxororand;
  logic [4:0] rv16_cs_rss1;
  assign rv16_cs_rss1 = rv16_rss1;
  logic [4:0] rv16_cs_rss2;
  assign rv16_cs_rss2 = rv16_rss2;
  logic [4:0] rv16_cs_rdd;
  assign rv16_cs_rdd = rv16_rss1;
  // CB format
  logic rv16_need_cb_rss1;
  assign rv16_need_cb_rss1 = rv16_format_cb & 1'b1;
  logic rv16_need_cb_rss2;
  assign rv16_need_cb_rss2 = rv16_format_cb & (rv16_beqz | rv16_bnez);
  logic rv16_need_cb_rdd;
  assign rv16_need_cb_rdd = rv16_format_cb & ~(rv16_beqz | rv16_bnez);
  logic [4:0] rv16_cb_rss1;
  assign rv16_cb_rss1 = rv16_rss1;
  logic [4:0] rv16_cb_rss2;
  assign rv16_cb_rss2 = 0;
  logic [4:0] rv16_cb_rdd;
  assign rv16_cb_rdd = rv16_rss1;
  // CJ format
  logic rv16_need_cj_rss1;
  assign rv16_need_cj_rss1 = rv16_format_cj & 1'b0;
  logic rv16_need_cj_rss2;
  assign rv16_need_cj_rss2 = rv16_format_cj & 1'b0;
  logic rv16_need_cj_rdd;
  assign rv16_need_cj_rdd = rv16_format_cj & 1'b1;
  logic [4:0] rv16_cj_rss1;
  assign rv16_cj_rss1 = 0;
  logic [4:0] rv16_cj_rss2;
  assign rv16_cj_rss2 = 0;
  logic [4:0] rv16_cj_rdd;
  assign rv16_cj_rdd = rv16_j ? 0 : 1;
  // RV16 register need aggregation
  logic rv16_need_rs1;
  assign rv16_need_rs1 = rv16_need_cr_rs1 | rv16_need_ci_rs1 | rv16_need_css_rs1;
  logic rv16_need_rs2;
  assign rv16_need_rs2 = rv16_need_cr_rs2 | rv16_need_ci_rs2 | rv16_need_css_rs2;
  logic rv16_need_rd;
  assign rv16_need_rd = rv16_need_cr_rd | rv16_need_ci_rd | rv16_need_css_rd;
  logic rv16_need_rss1;
  assign rv16_need_rss1 = rv16_need_ciw_rss1 | rv16_need_cl_rss1 | rv16_need_cs_rss1 | rv16_need_cb_rss1 | rv16_need_cj_rss1;
  logic rv16_need_rss2;
  assign rv16_need_rss2 = rv16_need_ciw_rss2 | rv16_need_cl_rss2 | rv16_need_cs_rss2 | rv16_need_cb_rss2 | rv16_need_cj_rss2;
  logic rv16_need_rdd;
  assign rv16_need_rdd = rv16_need_ciw_rdd | rv16_need_cl_rdd | rv16_need_cs_rdd | rv16_need_cb_rdd | rv16_need_cj_rdd;
  logic rv16_rs1en;
  assign rv16_rs1en = rv16_need_rs1 | rv16_need_rss1;
  logic rv16_rs2en;
  assign rv16_rs2en = rv16_need_rs2 | rv16_need_rss2;
  logic rv16_rden;
  assign rv16_rden = rv16_need_rd | rv16_need_rdd;
  // RV16 register index mux (OR-based, matching reference)
  logic [4:0] rv16_rs1idx;
  assign rv16_rs1idx = (rv16_need_cr_rs1 ? rv16_cr_rs1 : 0) | (rv16_need_ci_rs1 ? rv16_ci_rs1 : 0) | (rv16_need_css_rs1 ? rv16_css_rs1 : 0) | (rv16_need_ciw_rss1 ? rv16_ciw_rss1 : 0) | (rv16_need_cl_rss1 ? rv16_cl_rss1 : 0) | (rv16_need_cs_rss1 ? rv16_cs_rss1 : 0) | (rv16_need_cb_rss1 ? rv16_cb_rss1 : 0) | (rv16_need_cj_rss1 ? rv16_cj_rss1 : 0);
  logic [4:0] rv16_rs2idx;
  assign rv16_rs2idx = (rv16_need_cr_rs2 ? rv16_cr_rs2 : 0) | (rv16_need_ci_rs2 ? rv16_ci_rs2 : 0) | (rv16_need_css_rs2 ? rv16_css_rs2 : 0) | (rv16_need_ciw_rss2 ? rv16_ciw_rss2 : 0) | (rv16_need_cl_rss2 ? rv16_cl_rss2 : 0) | (rv16_need_cs_rss2 ? rv16_cs_rss2 : 0) | (rv16_need_cb_rss2 ? rv16_cb_rss2 : 0) | (rv16_need_cj_rss2 ? rv16_cj_rss2 : 0);
  logic [4:0] rv16_rdidx;
  assign rv16_rdidx = (rv16_need_cr_rd ? rv16_cr_rd : 0) | (rv16_need_ci_rd ? rv16_ci_rd : 0) | (rv16_need_css_rd ? rv16_css_rd : 0) | (rv16_need_ciw_rdd ? rv16_ciw_rdd : 0) | (rv16_need_cl_rdd ? rv16_cl_rdd : 0) | (rv16_need_cs_rdd ? rv16_cs_rdd : 0) | (rv16_need_cb_rdd ? rv16_cb_rdd : 0) | (rv16_need_cj_rdd ? rv16_cj_rdd : 0);
  // ── Output let bindings ─────────────────────────────────────────────
  logic [4:0] dec_rs1idx_w;
  assign dec_rs1idx_w = rv32 ? rv32_rs1 : rv16_rs1idx;
  logic [4:0] dec_rs2idx_w;
  assign dec_rs2idx_w = rv32 ? rv32_rs2 : rv16_rs2idx;
  logic [4:0] dec_rdidx_w;
  assign dec_rdidx_w = rv32 ? rv32_rd : rv16_rdidx;
  logic dec_rs1en_w;
  assign dec_rs1en_w = rv32 ? rv32_need_rs1 : rv16_rs1en & ~(rv16_rs1idx == 0);
  logic dec_rs2en_w;
  assign dec_rs2en_w = rv32 ? rv32_need_rs2 : rv16_rs2en & ~(rv16_rs2idx == 0);
  logic dec_rdwen_w;
  assign dec_rdwen_w = rv32 ? rv32_need_rd : rv16_rden & ~(rv16_rdidx == 0);
  logic dec_rs1x0_w;
  assign dec_rs1x0_w = dec_rs1idx_w == 0;
  logic dec_rs2x0_w;
  assign dec_rs2x0_w = dec_rs2idx_w == 0;
  // ── Illegal instruction ─────────────────────────────────────────────
  logic rv32_all0s_ilgl;
  assign rv32_all0s_ilgl = rv32_f7_0000000 & rv32_rs2_x0 & rv32_rs1_x0 & rv32_f3_000 & rv32_rd_x0 & opcode_6_5_00 & opcode_4_2_000 & (opcode[1:0] == 0);
  logic rv32_all1s_ilgl;
  assign rv32_all1s_ilgl = (rv32_f7 == 'h7F) & rv32_rs2_x31 & rv32_rs1_x31 & rv32_f3_111 & rv32_rd_x31 & opcode_6_5_11 & opcode_4_2_111 & opcode_1_0_11;
  logic rv16_all0s_ilgl;
  assign rv16_all0s_ilgl = rv16_f3_000 & rv32_f3_000 & rv32_rd_x0 & opcode_6_5_00 & opcode_4_2_000 & (opcode[1:0] == 0);
  logic rv16_all1s_ilgl;
  assign rv16_all1s_ilgl = rv16_f3_111 & rv32_f3_111 & rv32_rd_x31 & opcode_6_5_11 & opcode_4_2_111 & opcode_1_0_11;
  logic rv_all0s1s_ilgl;
  assign rv_all0s1s_ilgl = rv32 ? rv32_all0s_ilgl | rv32_all1s_ilgl : rv16_all0s_ilgl | rv16_all1s_ilgl;
  logic rv_index_ilgl;
  assign rv_index_ilgl = 1'b0;
  // RFREG_NUM_IS_32 → never illegal
  logic csr_op_b;
  assign csr_op_b = rv32_csr;
  logic legl_ops;
  assign legl_ops = alu_op | amoldst_op | bjp_op | csr_op_b | muldiv_op | nice_op;
  // ── Per-group info buses (32-bit, OR-combined into dec_info) ────────
  // Each group packs: [2:0]=GRP, [3]=RV32, [31:4]=sub-decode
  // ALU info bus (GRP=0)
  logic [31:0] alu_info;
  assign alu_info = alu_op ? {11'd0, rv32_wfi, rv32_ebreak | rv16_ebreak, rv32_ecall, rv16_nop | rv32_nop, rv32_auipc, need_imm, rv32_lui | rv16_lui, rv32_sltu | rv32_sltiu, rv32_slt | rv32_slti, rv32_and | rv32_andi_b | rv16_andi | rv16_and, rv32_or | rv32_ori | rv16_or, rv32_sra | rv32_srai | rv16_srai, rv32_srl | rv32_srli | rv16_srli, rv32_sll | rv32_slli | rv16_slli, rv32_xor | rv32_xori | rv16_xor, rv32_sub | rv16_sub, rv32_add | rv32_addi | rv32_auipc | rv16_addi4spn | rv16_addi | rv16_addi16sp | rv16_add | rv16_li | rv16_mv, rv32, 3'd0} : 0;
  // [31:21] unused
  // [20] WFI
  // [19] EBRK
  // [18] ECAL
  // [17] NOP
  // [16] OP1PC
  // [15] OP2IMM
  // [14] LUI
  // [13] SLTU
  // [12] SLT
  // [11] AND
  // [10] OR
  // [9] SRA
  // [8] SRL
  // [7] SLL
  // [6] XOR
  // [5] SUB
  // [4] ADD
  // [3] RV32
  // [2:0] GRP=0
  // AGU info bus (GRP=1)
  logic [31:0] agu_info;
  assign agu_info = amoldst_op ? {11'd0, need_imm, rv32_amominu_w, rv32_amomaxu_w, rv32_amomin_w, rv32_amomax_w, rv32_amoxor_w, rv32_amoor_w, rv32_amoand_w, rv32_amoadd_w, rv32_amoswap_w, rv32_amo & ~(rv32_lr_w | rv32_sc_w), rv32_lr_w | rv32_sc_w, lsu_info_usign, lsu_info_size, rv32_store | rv32_sc_w | rv16_sw | rv16_swsp, rv32_load | rv32_lr_w | rv16_lw | rv16_lwsp, rv32, 3'd1} : 0;
  // [31:21] unused
  // [20] OP2IMM
  // [19] AMOMINU
  // [18] AMOMAXU
  // [17] AMOMIN
  // [16] AMOMAX
  // [15] AMOXOR
  // [14] AMOOR
  // [13] AMOAND
  // [12] AMOADD
  // [11] AMOSWAP
  // [10] AMO
  // [9] EXCL
  // [8] USIGN
  // [7:6] SIZE
  // [5] STORE
  // [4] LOAD
  // [3] RV32
  // [2:0] GRP=1
  // BJP info bus (GRP=2)
  logic [31:0] bjp_info;
  assign bjp_info = bjp_op ? {15'd0, rv32_fence_i, rv32_fence, rv32_dret, rv32_mret, dec_bxx_b, rv32_bgtu, rv32_bltu, rv32_bgt, rv32_blt, rv32_bne | rv16_bnez, rv32_beq | rv16_beqz, i_prdt_taken, dec_jal_b | dec_jalr_b, rv32, 3'd2} : 0;
  // [31:17] unused
  // [16] FENCEI
  // [15] FENCE
  // [14] DRET
  // [13] MRET
  // [12] BXX
  // [11] BGTU
  // [10] BLTU
  // [9] BGT
  // [8] BLT
  // [7] BNE
  // [6] BEQ
  // [5] BPRDT
  // [4] JUMP
  // [3] RV32
  // [2:0] GRP=2
  // CSR info bus (GRP=3)
  logic [31:0] csr_info;
  assign csr_info = csr_op_b ? {6'd0, rv32_instr[31:20], rv32_rs1_x0, rv32_rs1, rv32_csrrwi | rv32_csrrsi | rv32_csrrci, rv32_csrrc | rv32_csrrci, rv32_csrrs | rv32_csrrsi, rv32_csrrw | rv32_csrrwi, rv32, 3'd3} : 0;
  // [31:26] unused
  // [25:14] CSRIDX
  // [13] RS1IS0
  // [12:8] ZIMM
  // [7] RS1IMM
  // [6] CSRRC
  // [5] CSRRS
  // [4] CSRRW
  // [3] RV32
  // [2:0] GRP=3
  // MULDIV info bus (GRP=4)
  logic [31:0] muldiv_info;
  assign muldiv_info = muldiv_op ? {19'd0, i_muldiv_b2b, rv32_remu, rv32_rem, rv32_divu, rv32_div, rv32_mulhu, rv32_mulhsu, rv32_mulh, rv32_mul, rv32, 3'd4} : 0;
  // [31:13] unused
  // [12] B2B
  // [11] REMU
  // [10] REM
  // [9] DIVU
  // [8] DIV
  // [7] MULHU
  // [6] MULHSU
  // [5] MULH
  // [4] MUL
  // [3] RV32
  // [2:0] GRP=4
  // NICE info bus (GRP=5)
  logic [31:0] nice_info;
  assign nice_info = nice_op ? {1'd0, nice_instr, rv32, 3'd5} : 0;
  // [31] unused
  // [30:4] NICE_INSTR
  // [3] RV32
  // [2:0] GRP=5
  // ── Combinational output assignments ─────────────────────────────────
  assign dec_rs1idx = dec_rs1idx_w;
  assign dec_rs2idx = dec_rs2idx_w;
  assign dec_rdidx = dec_rdidx_w;
  assign dec_rs1x0 = dec_rs1x0_w;
  assign dec_rs2x0 = dec_rs2x0_w;
  assign dec_rs1en = dec_rs1en_w;
  assign dec_rs2en = dec_rs2en_w;
  assign dec_rdwen = dec_rdwen_w;
  assign dec_pc = i_pc;
  assign dec_misalgn = i_misalgn;
  assign dec_buserr = i_buserr;
  assign dec_ilegl = rv_all0s1s_ilgl | rv_index_ilgl | rv16_addi16sp_ilgl | rv16_addi4spn_ilgl | rv16_li_lui_ilgl | rv16_sxxi_shamt_ilgl | rv32_sxxi_shamt_ilgl | rv32_dret_ilgl | rv16_lwsp_ilgl | ~legl_ops;
  assign dec_nice = nice_op;
  assign nice_cmt_off_ilgl_o = nice_xs_off & nice_op;
  assign dec_mulhsu = rv32_mulh | rv32_mulhsu | rv32_mulhu;
  assign dec_mul = rv32_mul;
  assign dec_div = rv32_div;
  assign dec_rem = rv32_rem;
  assign dec_divu = rv32_divu;
  assign dec_remu = rv32_remu;
  assign dec_rv32 = rv32;
  assign dec_bjp = dec_bjp_b;
  assign dec_jal = dec_jal_b;
  assign dec_jalr = dec_jalr_b;
  assign dec_bxx = dec_bxx_b;
  assign dec_jalr_rs1idx = rv32 ? rv32_rs1 : rv16_rs1;
  assign dec_imm = rv32 ? rv32_imm : rv16_imm;
  assign dec_bjp_imm = (rv16_jal | rv16_j ? rv16_cj_imm : 0) | (rv16_jalr_mv_add ? 0 : 0) | (rv16_beqz | rv16_bnez ? rv16_cb_imm : 0) | (rv32_jal ? rv32_j_imm : 0) | (rv32_jalr ? rv32_i_imm : 0) | (rv32_branch ? rv32_b_imm : 0);
  assign dec_info = alu_info | agu_info | bjp_info | csr_info | muldiv_info | nice_info;
  assign o_alu = alu_op;
  assign o_agu = amoldst_op;
  assign o_alu_add = rv32_add | rv32_addi | rv32_auipc | rv16_addi4spn | rv16_addi | rv16_addi16sp | rv16_add | rv16_li | rv16_mv;
  assign o_alu_sub = rv32_sub | rv16_sub;
  assign o_alu_xor = rv32_xor | rv32_xori | rv16_xor;
  assign o_alu_sll = rv32_sll | rv32_slli | rv16_slli;
  assign o_alu_srl = rv32_srl | rv32_srli | rv16_srli;
  assign o_alu_sra = rv32_sra | rv32_srai | rv16_srai;
  assign o_alu_or = rv32_or | rv32_ori | rv16_or;
  assign o_alu_and = rv32_and | rv32_andi_b | rv16_andi | rv16_and;
  assign o_alu_slt = rv32_slt | rv32_slti;
  assign o_alu_sltu = rv32_sltu | rv32_sltiu;
  assign o_alu_lui = rv32_lui | rv16_lui;
  assign o_beq = rv32_beq | rv16_beqz;
  assign o_bne = rv32_bne | rv16_bnez;
  assign o_blt = rv32_blt;
  assign o_bge = rv32_bgt;
  assign o_bltu = rv32_bltu;
  assign o_bgeu = rv32_bgtu;
  assign o_jump = dec_jal_b | dec_jalr_b;
  assign o_mulh = rv32_mulh;
  assign o_mulhu = rv32_mulhu;
  assign o_load = rv32_load | rv32_lr_w | rv16_lw | rv16_lwsp;
  assign o_store = rv32_store | rv32_sc_w | rv16_sw | rv16_swsp;

endmodule

// Register indices
// Register x0 flags
// Register enables
// Pass-through
// Illegal
// NICE
// MulDiv flags
// Type flags
// Immediate output
// BJP immediate
// ── Info bus: OR per-group contributions (only one group active) ──
// ── Per-group control outputs (for integration module) ────────────
// E203 HBirdv2 Execution Dispatch Unit
// Routes decoded instructions to ALU pipeline, checks OITF hazards,
// manages WFI halt handshake. Matches RealBench port interface.
module e203_exu_disp (
  input logic clk,
  input logic rst_n,
  input logic wfi_halt_exu_req,
  output logic wfi_halt_exu_ack,
  input logic oitf_empty,
  input logic amo_wait,
  input logic disp_i_valid,
  output logic disp_i_ready,
  input logic disp_i_rs1x0,
  input logic disp_i_rs2x0,
  input logic disp_i_rs1en,
  input logic disp_i_rs2en,
  input logic [4:0] disp_i_rs1idx,
  input logic [4:0] disp_i_rs2idx,
  input logic [31:0] disp_i_rs1,
  input logic [31:0] disp_i_rs2,
  input logic disp_i_rdwen,
  input logic [4:0] disp_i_rdidx,
  input logic [31:0] disp_i_info,
  input logic [31:0] disp_i_imm,
  input logic [31:0] disp_i_pc,
  input logic disp_i_misalgn,
  input logic disp_i_buserr,
  input logic disp_i_ilegl,
  output logic disp_o_alu_valid,
  input logic disp_o_alu_ready,
  input logic disp_o_alu_longpipe,
  output logic [31:0] disp_o_alu_rs1,
  output logic [31:0] disp_o_alu_rs2,
  output logic disp_o_alu_rdwen,
  output logic [4:0] disp_o_alu_rdidx,
  output logic [31:0] disp_o_alu_info,
  output logic [31:0] disp_o_alu_imm,
  output logic [31:0] disp_o_alu_pc,
  output logic [0:0] disp_o_alu_itag,
  output logic disp_o_alu_misalgn,
  output logic disp_o_alu_buserr,
  output logic disp_o_alu_ilegl,
  input logic oitfrd_match_disprs1,
  input logic oitfrd_match_disprs2,
  input logic oitfrd_match_disprs3,
  input logic oitfrd_match_disprd,
  input logic [0:0] disp_oitf_ptr,
  output logic disp_oitf_ena,
  input logic disp_oitf_ready,
  output logic disp_oitf_rs1fpu,
  output logic disp_oitf_rs2fpu,
  output logic disp_oitf_rs3fpu,
  output logic disp_oitf_rdfpu,
  output logic disp_oitf_rs1en,
  output logic disp_oitf_rs2en,
  output logic disp_oitf_rs3en,
  output logic disp_oitf_rdwen,
  output logic [4:0] disp_oitf_rs1idx,
  output logic [4:0] disp_oitf_rs2idx,
  output logic [4:0] disp_oitf_rs3idx,
  output logic [4:0] disp_oitf_rdidx,
  output logic [31:0] disp_oitf_pc
);

  // ── WFI halt interface ────────────────────────────────────────────
  // ── OITF status ───────────────────────────────────────────────────
  // ── Dispatch input (from decode) ──────────────────────────────────
  // ── ALU dispatch output ───────────────────────────────────────────
  // ── OITF hazard check inputs ──────────────────────────────────────
  // ── OITF dispatch interface ───────────────────────────────────────
  // ── Info bus decoding ─────────────────────────────────────────────
  logic [2:0] info_grp;
  assign info_grp = disp_i_info[2:0];
  logic is_agu;
  assign is_agu = info_grp == 1;
  logic is_bjp;
  assign is_bjp = info_grp == 2;
  logic is_csr;
  assign is_csr = info_grp == 3;
  logic is_fence_fencei;
  assign is_fence_fencei = is_bjp & ((disp_i_info[15:15] != 0) | (disp_i_info[16:16] != 0));
  // ── Hazard detection (matching reference: no rs_en/rdwen gating) ──
  logic raw_dep;
  assign raw_dep = oitfrd_match_disprs1 | oitfrd_match_disprs2 | oitfrd_match_disprs3;
  logic waw_dep;
  assign waw_dep = oitfrd_match_disprd;
  logic dep_stall;
  assign dep_stall = raw_dep | waw_dep;
  // For predicted long-pipe (AGU grp), OITF must be ready
  logic oitf_stall;
  assign oitf_stall = is_agu & ~disp_oitf_ready;
  // Dispatch condition: CSR/fence need OITF empty, no WFI halt, no dep, oitf ready for AGU
  logic disp_condition;
  assign disp_condition = (is_csr ? oitf_empty : 1'b1) & (is_fence_fencei ? oitf_empty : 1'b1) & ~wfi_halt_exu_req & ~dep_stall & ~oitf_stall;
  assign disp_o_alu_valid = disp_i_valid & disp_condition;
  assign disp_i_ready = disp_o_alu_ready & disp_condition;
  assign disp_o_alu_rs1 = disp_i_rs1x0 ? 0 : disp_i_rs1;
  assign disp_o_alu_rs2 = disp_i_rs2x0 ? 0 : disp_i_rs2;
  assign disp_o_alu_rdwen = disp_i_rdwen;
  assign disp_o_alu_rdidx = disp_i_rdidx;
  assign disp_o_alu_info = disp_i_info;
  assign disp_o_alu_imm = disp_i_imm;
  assign disp_o_alu_pc = disp_i_pc;
  assign disp_o_alu_itag = disp_oitf_ptr;
  assign disp_o_alu_misalgn = disp_i_misalgn;
  assign disp_o_alu_buserr = disp_i_buserr;
  assign disp_o_alu_ilegl = disp_i_ilegl;
  assign disp_oitf_ena = disp_i_valid & disp_i_ready & disp_o_alu_longpipe;
  assign disp_oitf_rs1en = disp_i_rs1en;
  assign disp_oitf_rs2en = disp_i_rs2en;
  assign disp_oitf_rs3en = 1'b0;
  assign disp_oitf_rdwen = disp_i_rdwen;
  assign disp_oitf_rs1idx = disp_i_rs1idx;
  assign disp_oitf_rs2idx = disp_i_rs2idx;
  assign disp_oitf_rs3idx = 0;
  assign disp_oitf_rdidx = disp_i_rdidx;
  assign disp_oitf_pc = disp_i_pc;
  assign disp_oitf_rs1fpu = 1'b0;
  assign disp_oitf_rs2fpu = 1'b0;
  assign disp_oitf_rs3fpu = 1'b0;
  assign disp_oitf_rdfpu = 1'b0;
  assign wfi_halt_exu_ack = oitf_empty & ~amo_wait;

endmodule

// Dispatch handshake
// Pass-through to ALU (x0 hardwired to 0)
// OITF dispatch: allocate entry for long-pipe instructions
// No FPU in E203
// WFI halt ack: EXU ready to halt when OITF is empty and no AMO
// E203 Outstanding Instruction Track FIFO (OITF)
// Tracks in-flight long-latency instructions for hazard detection.
// Circular FIFO with 2 entries; stores rd info + FPU flags.
// Matches RealBench port interface.
module e203_exu_oitf #(
  parameter int OITF_DEPTH = 2
) (
  input logic clk,
  input logic rst_n,
  output logic dis_ready,
  input logic dis_ena,
  input logic ret_ena,
  output logic [0:0] dis_ptr,
  output logic [0:0] ret_ptr,
  output logic [4:0] ret_rdidx,
  output logic ret_rdwen,
  output logic ret_rdfpu,
  output logic [31:0] ret_pc,
  input logic disp_i_rs1en,
  input logic disp_i_rs2en,
  input logic disp_i_rs3en,
  input logic disp_i_rdwen,
  input logic disp_i_rs1fpu,
  input logic disp_i_rs2fpu,
  input logic disp_i_rs3fpu,
  input logic disp_i_rdfpu,
  input logic [4:0] disp_i_rs1idx,
  input logic [4:0] disp_i_rs2idx,
  input logic [4:0] disp_i_rs3idx,
  input logic [4:0] disp_i_rdidx,
  input logic [31:0] disp_i_pc,
  output logic oitfrd_match_disprs1,
  output logic oitfrd_match_disprs2,
  output logic oitfrd_match_disprs3,
  output logic oitfrd_match_disprd,
  output logic oitf_empty
);

  // ── Dispatch interface ────────────────────────────────────────────
  // ── Pointer outputs ───────────────────────────────────────────────
  // ── Retire info outputs ───────────────────────────────────────────
  // ── Dispatch info inputs ──────────────────────────────────────────
  // ── Hazard check outputs ──────────────────────────────────────────
  // ── FIFO state registers ──────────────────────────────────────────
  logic valid_0 = 1'b0;
  logic valid_1 = 1'b0;
  logic [4:0] rdidx_0;
  logic [4:0] rdidx_1;
  logic rdwen_0;
  logic rdwen_1;
  logic rdfpu_0;
  logic rdfpu_1;
  logic [31:0] pc_0;
  logic [31:0] pc_1;
  logic wr_ptr_r = 1'b0;
  logic rd_ptr_r = 1'b0;
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      rd_ptr_r <= 1'b0;
      valid_0 <= 1'b0;
      valid_1 <= 1'b0;
      wr_ptr_r <= 1'b0;
    end else begin
      // Allocate: write new entry at wr_ptr
      if (dis_ena & dis_ready) begin
        if (wr_ptr_r == 1'b0) begin
          valid_0 <= 1'b1;
          rdidx_0 <= disp_i_rdidx;
          rdwen_0 <= disp_i_rdwen;
          rdfpu_0 <= disp_i_rdfpu;
          pc_0 <= disp_i_pc;
        end else begin
          valid_1 <= 1'b1;
          rdidx_1 <= disp_i_rdidx;
          rdwen_1 <= disp_i_rdwen;
          rdfpu_1 <= disp_i_rdfpu;
          pc_1 <= disp_i_pc;
        end
        wr_ptr_r <= ~wr_ptr_r;
      end
      // Deallocate: clear oldest entry at rd_ptr
      if (ret_ena) begin
        if (rd_ptr_r == 1'b0) begin
          valid_0 <= 1'b0;
        end else begin
          valid_1 <= 1'b0;
        end
        rd_ptr_r <= ~rd_ptr_r;
      end
    end
  end
  always_comb begin
    // FIFO status
    oitf_empty = ~valid_0 & ~valid_1;
    dis_ready = ~(valid_0 & valid_1);
    // Pointer outputs
    dis_ptr = wr_ptr_r;
    ret_ptr = rd_ptr_r;
    // Return oldest entry info (at rd_ptr)
    if (rd_ptr_r == 1'b0) begin
      ret_rdidx = rdidx_0;
      ret_rdwen = rdwen_0;
      ret_rdfpu = rdfpu_0;
      ret_pc = pc_0;
    end else begin
      ret_rdidx = rdidx_1;
      ret_rdwen = rdwen_1;
      ret_rdfpu = rdfpu_1;
      ret_pc = pc_1;
    end
    // ── Hazard checks: compare dispatch rs/rd against all valid entries ──
    // Hazard checks: compare dispatch rs/rd against valid OITF entries
    // Match only when FPU types agree: both FPU or both integer
    oitfrd_match_disprs1 = (valid_0 & rdwen_0 & disp_i_rs1en & (disp_i_rs1idx == rdidx_0) & (disp_i_rs1fpu == rdfpu_0)) | (valid_1 & rdwen_1 & disp_i_rs1en & (disp_i_rs1idx == rdidx_1) & (disp_i_rs1fpu == rdfpu_1));
    oitfrd_match_disprs2 = (valid_0 & rdwen_0 & disp_i_rs2en & (disp_i_rs2idx == rdidx_0) & (disp_i_rs2fpu == rdfpu_0)) | (valid_1 & rdwen_1 & disp_i_rs2en & (disp_i_rs2idx == rdidx_1) & (disp_i_rs2fpu == rdfpu_1));
    oitfrd_match_disprs3 = (valid_0 & rdwen_0 & disp_i_rs3en & (disp_i_rs3idx == rdidx_0) & (disp_i_rs3fpu == rdfpu_0)) | (valid_1 & rdwen_1 & disp_i_rs3en & (disp_i_rs3idx == rdidx_1) & (disp_i_rs3fpu == rdfpu_1));
    oitfrd_match_disprd = (valid_0 & rdwen_0 & disp_i_rdwen & (disp_i_rdidx == rdidx_0) & (disp_i_rdfpu == rdfpu_0)) | (valid_1 & rdwen_1 & disp_i_rdwen & (disp_i_rdidx == rdidx_1) & (disp_i_rdfpu == rdfpu_1));
  end

endmodule

// E203 HBirdv2 ALU Top-Level
// Structural wrapper instantiating 7 sub-modules: dpath, bjp, csrctrl, lsuagu,
// muldiv, rglr, nice. Matches RealBench e203_exu_alu_ref.sv exactly.
module e203_exu_alu #(
  parameter int XLEN = 32
) (
  input logic clk,
  input logic rst_n,
  input logic i_valid,
  output logic i_ready,
  output logic i_longpipe,
  input logic nice_xs_off,
  output logic amo_wait,
  input logic oitf_empty,
  input logic i_itag,
  input logic [31:0] i_rs1,
  input logic [31:0] i_rs2,
  input logic [31:0] i_imm,
  input logic [31:0] i_info,
  input logic [31:0] i_pc,
  input logic [31:0] i_instr,
  input logic i_pc_vld,
  input logic [4:0] i_rdidx,
  input logic i_rdwen,
  input logic i_ilegl,
  input logic i_buserr,
  input logic i_misalgn,
  input logic flush_req,
  input logic flush_pulse,
  output logic cmt_o_valid,
  input logic cmt_o_ready,
  output logic cmt_o_pc_vld,
  output logic [31:0] cmt_o_pc,
  output logic [31:0] cmt_o_instr,
  output logic [31:0] cmt_o_imm,
  output logic cmt_o_rv32,
  output logic cmt_o_bjp,
  output logic cmt_o_mret,
  output logic cmt_o_dret,
  output logic cmt_o_ecall,
  output logic cmt_o_ebreak,
  output logic cmt_o_fencei,
  output logic cmt_o_wfi,
  output logic cmt_o_ifu_misalgn,
  output logic cmt_o_ifu_buserr,
  output logic cmt_o_ifu_ilegl,
  output logic cmt_o_bjp_prdt,
  output logic cmt_o_bjp_rslv,
  output logic cmt_o_misalgn,
  output logic cmt_o_ld,
  output logic cmt_o_stamo,
  output logic cmt_o_buserr,
  output logic [31:0] cmt_o_badaddr,
  output logic wbck_o_valid,
  input logic wbck_o_ready,
  output logic [31:0] wbck_o_wdat,
  output logic [4:0] wbck_o_rdidx,
  input logic mdv_nob2b,
  output logic csr_ena,
  output logic csr_wr_en,
  output logic csr_rd_en,
  output logic [11:0] csr_idx,
  input logic nonflush_cmt_ena,
  input logic csr_access_ilgl,
  input logic [31:0] read_csr_dat,
  output logic [31:0] wbck_csr_dat,
  output logic agu_icb_cmd_valid,
  input logic agu_icb_cmd_ready,
  output logic [31:0] agu_icb_cmd_addr,
  output logic agu_icb_cmd_read,
  output logic [31:0] agu_icb_cmd_wdata,
  output logic [3:0] agu_icb_cmd_wmask,
  output logic agu_icb_cmd_lock,
  output logic agu_icb_cmd_excl,
  output logic [1:0] agu_icb_cmd_size,
  output logic agu_icb_cmd_back2agu,
  output logic agu_icb_cmd_usign,
  output logic agu_icb_cmd_itag,
  input logic agu_icb_rsp_valid,
  output logic agu_icb_rsp_ready,
  input logic agu_icb_rsp_err,
  input logic agu_icb_rsp_excl_ok,
  input logic [31:0] agu_icb_rsp_rdata,
  output logic nice_req_valid,
  input logic nice_req_ready,
  output logic [31:0] nice_req_instr,
  output logic [31:0] nice_req_rs1,
  output logic [31:0] nice_req_rs2,
  input logic nice_rsp_multicyc_valid,
  output logic nice_rsp_multicyc_ready,
  output logic nice_longp_wbck_valid,
  input logic nice_longp_wbck_ready,
  output logic nice_o_itag,
  input logic i_nice_cmt_off_ilgl
);

  // ── Dispatch interface ───────────────────────────────────────────────────
  // ── Flush signals ────────────────────────────────────────────────────────
  // ── Commit output ────────────────────────────────────────────────────────
  // ── Write-back output ────────────────────────────────────────────────────
  // ── MDV control ──────────────────────────────────────────────────────────
  // ── CSR access ───────────────────────────────────────────────────────────
  // ── AGU ICB master interface ─────────────────────────────────────────────
  // ── NICE co-processor interface ──────────────────────────────────────────
  // ── Decode: group select from i_info[2:0] (GRP field) ───────────────────
  // GRP encoding: ALU=0, AGU=1, BJP=2, CSR=3, MULDIV=4, NICE=5
  logic [2:0] grp;
  assign grp = i_info[2:0];
  logic ifu_excp_op;
  assign ifu_excp_op = i_ilegl | i_buserr | i_misalgn;
  logic alu_op;
  assign alu_op = ~ifu_excp_op & (grp == 0);
  logic agu_op;
  assign agu_op = ~ifu_excp_op & (grp == 1);
  logic bjp_op;
  assign bjp_op = ~ifu_excp_op & (grp == 2);
  logic csr_op;
  assign csr_op = ~ifu_excp_op & (grp == 3);
  logic mdv_op;
  assign mdv_op = ~ifu_excp_op & (grp == 4);
  logic nice_op;
  assign nice_op = ~ifu_excp_op & (grp == 5);
  // ── Per-target valid ────────────────────────────────────────────────────
  logic mdv_i_valid;
  assign mdv_i_valid = i_valid & mdv_op;
  logic agu_i_valid;
  assign agu_i_valid = i_valid & agu_op;
  logic alu_i_valid;
  assign alu_i_valid = i_valid & alu_op;
  logic bjp_i_valid;
  assign bjp_i_valid = i_valid & bjp_op;
  logic csr_i_valid;
  assign csr_i_valid = i_valid & csr_op;
  logic ifu_excp_i_valid;
  assign ifu_excp_i_valid = i_valid & ifu_excp_op;
  logic nice_i_valid;
  assign nice_i_valid = i_valid & nice_op;
  // ── Per-target ready wires ──────────────────────────────────────────────
  logic mdv_i_ready;
  logic agu_i_ready;
  logic alu_i_ready;
  logic bjp_i_ready;
  logic csr_i_ready;
  logic ifu_excp_i_ready;
  logic nice_i_ready;
  // ── i_ready: OR of each target's ready gated by its op select ───────────
  assign i_ready = (agu_i_ready & agu_op) | (mdv_i_ready & mdv_op) | (alu_i_ready & alu_op) | (ifu_excp_i_ready & ifu_excp_op) | (bjp_i_ready & bjp_op) | (csr_i_ready & csr_op) | (nice_i_ready & nice_op);
  // ── i_longpipe: OR of long-pipe flags from AGU, MULDIV, NICE ────────────
  logic agu_i_longpipe;
  logic mdv_i_longpipe;
  logic nice_o_longpipe;
  logic nice_i_longpipe;
  assign nice_i_longpipe = nice_o_longpipe;
  assign i_longpipe = (agu_i_longpipe & agu_op) | (mdv_i_longpipe & mdv_op) | (nice_i_longpipe & nice_op);
  // ── Gated operands per target (matches {WIDTH{op}} & signal pattern) ────
  logic [31:0] csr_i_rs1;
  assign csr_i_rs1 = csr_op ? i_rs1 : 0;
  logic [31:0] csr_i_rs2;
  assign csr_i_rs2 = csr_op ? i_rs2 : 0;
  logic [31:0] csr_i_imm;
  assign csr_i_imm = csr_op ? i_imm : 0;
  logic [31:0] csr_i_info;
  assign csr_i_info = csr_op ? i_info : 0;
  logic csr_i_rdwen;
  assign csr_i_rdwen = csr_op & i_rdwen;
  logic [31:0] bjp_i_rs1;
  assign bjp_i_rs1 = bjp_op ? i_rs1 : 0;
  logic [31:0] bjp_i_rs2;
  assign bjp_i_rs2 = bjp_op ? i_rs2 : 0;
  logic [31:0] bjp_i_imm;
  assign bjp_i_imm = bjp_op ? i_imm : 0;
  logic [31:0] bjp_i_info;
  assign bjp_i_info = bjp_op ? i_info : 0;
  logic [31:0] bjp_i_pc;
  assign bjp_i_pc = bjp_op ? i_pc : 0;
  logic [31:0] agu_i_rs1;
  assign agu_i_rs1 = agu_op ? i_rs1 : 0;
  logic [31:0] agu_i_rs2;
  assign agu_i_rs2 = agu_op ? i_rs2 : 0;
  logic [31:0] agu_i_imm;
  assign agu_i_imm = agu_op ? i_imm : 0;
  logic [31:0] agu_i_info;
  assign agu_i_info = agu_op ? i_info : 0;
  logic agu_i_itag;
  assign agu_i_itag = agu_op & i_itag;
  logic [31:0] alu_i_rs1;
  assign alu_i_rs1 = alu_op ? i_rs1 : 0;
  logic [31:0] alu_i_rs2;
  assign alu_i_rs2 = alu_op ? i_rs2 : 0;
  logic [31:0] alu_i_imm;
  assign alu_i_imm = alu_op ? i_imm : 0;
  logic [31:0] alu_i_info;
  assign alu_i_info = alu_op ? i_info : 0;
  logic [31:0] alu_i_pc;
  assign alu_i_pc = alu_op ? i_pc : 0;
  logic [31:0] mdv_i_rs1;
  assign mdv_i_rs1 = mdv_op ? i_rs1 : 0;
  logic [31:0] mdv_i_rs2;
  assign mdv_i_rs2 = mdv_op ? i_rs2 : 0;
  logic [31:0] mdv_i_imm;
  assign mdv_i_imm = mdv_op ? i_imm : 0;
  logic [31:0] mdv_i_info;
  assign mdv_i_info = mdv_op ? i_info : 0;
  logic mdv_i_itag;
  assign mdv_i_itag = mdv_op & i_itag;
  logic [31:0] nice_i_rs1;
  assign nice_i_rs1 = nice_op ? i_rs1 : 0;
  logic [31:0] nice_i_rs2;
  assign nice_i_rs2 = nice_op ? i_rs2 : 0;
  logic nice_i_itag;
  assign nice_i_itag = nice_op & i_itag;
  // ── Sub-module output wires ─────────────────────────────────────────────
  // CSR
  logic csr_o_valid;
  logic csr_o_ready;
  logic [31:0] csr_o_wbck_wdat;
  logic csr_o_wbck_err;
  // BJP
  logic bjp_o_valid;
  logic bjp_o_ready;
  logic [31:0] bjp_o_wbck_wdat;
  logic bjp_o_wbck_err;
  logic bjp_o_cmt_bjp;
  logic bjp_o_cmt_mret;
  logic bjp_o_cmt_dret;
  logic bjp_o_cmt_fencei;
  logic bjp_o_cmt_prdt;
  logic bjp_o_cmt_rslv;
  logic [31:0] bjp_req_alu_op1;
  logic [31:0] bjp_req_alu_op2;
  logic bjp_req_alu_cmp_eq;
  logic bjp_req_alu_cmp_ne;
  logic bjp_req_alu_cmp_lt;
  logic bjp_req_alu_cmp_gt;
  logic bjp_req_alu_cmp_ltu;
  logic bjp_req_alu_cmp_gtu;
  logic bjp_req_alu_add;
  logic [31:0] bjp_req_alu_add_res;
  logic bjp_req_alu_cmp_res;
  // AGU
  logic agu_o_valid;
  logic agu_o_ready;
  logic [31:0] agu_o_wbck_wdat;
  logic agu_o_wbck_err;
  logic agu_o_cmt_misalgn;
  logic agu_o_cmt_ld;
  logic agu_o_cmt_stamo;
  logic agu_o_cmt_buserr;
  logic [31:0] agu_o_cmt_badaddr;
  logic [31:0] agu_req_alu_op1;
  logic [31:0] agu_req_alu_op2;
  logic agu_req_alu_swap;
  logic agu_req_alu_add;
  logic agu_req_alu_and;
  logic agu_req_alu_or;
  logic agu_req_alu_xor;
  logic agu_req_alu_max;
  logic agu_req_alu_min;
  logic agu_req_alu_maxu;
  logic agu_req_alu_minu;
  logic [31:0] agu_req_alu_res;
  logic agu_sbf_0_ena;
  logic [31:0] agu_sbf_0_nxt;
  logic [31:0] agu_sbf_0_r;
  logic agu_sbf_1_ena;
  logic [31:0] agu_sbf_1_nxt;
  logic [31:0] agu_sbf_1_r;
  // Regular ALU
  logic alu_o_valid;
  logic alu_o_ready;
  logic [31:0] alu_o_wbck_wdat;
  logic alu_o_wbck_err;
  logic alu_o_cmt_ecall;
  logic alu_o_cmt_ebreak;
  logic alu_o_cmt_wfi;
  logic alu_req_alu_add;
  logic alu_req_alu_sub;
  logic alu_req_alu_xor;
  logic alu_req_alu_sll;
  logic alu_req_alu_srl;
  logic alu_req_alu_sra;
  logic alu_req_alu_or;
  logic alu_req_alu_and;
  logic alu_req_alu_slt;
  logic alu_req_alu_sltu;
  logic alu_req_alu_lui;
  logic [31:0] alu_req_alu_op1;
  logic [31:0] alu_req_alu_op2;
  logic [31:0] alu_req_alu_res;
  // MulDiv
  logic mdv_o_valid;
  logic mdv_o_ready;
  logic [31:0] mdv_o_wbck_wdat;
  logic mdv_o_wbck_err;
  logic [34:0] muldiv_req_alu_op1;
  logic [34:0] muldiv_req_alu_op2;
  logic muldiv_req_alu_add;
  logic muldiv_req_alu_sub;
  logic [34:0] muldiv_req_alu_res;
  logic muldiv_sbf_0_ena;
  logic [32:0] muldiv_sbf_0_nxt;
  logic [32:0] muldiv_sbf_0_r;
  logic muldiv_sbf_1_ena;
  logic [32:0] muldiv_sbf_1_nxt;
  logic [32:0] muldiv_sbf_1_r;
  // NICE
  logic nice_o_valid;
  logic nice_o_ready;
  // ── Datapath request enables ─────────────────────────────────────────────
  logic alu_req_alu;
  assign alu_req_alu = alu_op & i_rdwen;
  logic muldiv_req_alu;
  assign muldiv_req_alu = mdv_op;
  logic bjp_req_alu;
  assign bjp_req_alu = bjp_op;
  logic agu_req_alu;
  assign agu_req_alu = agu_op;
  // ── Sub-module instances ─────────────────────────────────────────────────
  // NICE co-processor interface
  e203_exu_nice nice (
    .clk(clk),
    .rst_n(rst_n),
    .nice_i_xs_off(nice_xs_off),
    .nice_i_valid(nice_i_valid),
    .nice_i_ready(nice_i_ready),
    .nice_i_instr(i_instr),
    .nice_i_rs1(nice_i_rs1),
    .nice_i_rs2(nice_i_rs2),
    .nice_i_itag(nice_i_itag),
    .nice_o_longpipe(nice_o_longpipe),
    .nice_o_valid(nice_o_valid),
    .nice_o_ready(nice_o_ready),
    .nice_o_itag_valid(nice_longp_wbck_valid),
    .nice_o_itag_ready(nice_longp_wbck_ready),
    .nice_o_itag(nice_o_itag),
    .nice_rsp_multicyc_valid(nice_rsp_multicyc_valid),
    .nice_rsp_multicyc_ready(nice_rsp_multicyc_ready),
    .nice_req_valid(nice_req_valid),
    .nice_req_ready(nice_req_ready),
    .nice_req_instr(nice_req_instr),
    .nice_req_rs1(nice_req_rs1),
    .nice_req_rs2(nice_req_rs2)
  );
  // CSR control
  e203_exu_alu_csrctrl csrctrl (
    .clk(clk),
    .rst_n(rst_n),
    .csr_access_ilgl(csr_access_ilgl),
    .csr_i_valid(csr_i_valid),
    .csr_i_ready(csr_i_ready),
    .csr_i_rs1(csr_i_rs1),
    .csr_i_info(csr_i_info[25:0]),
    .csr_i_rdwen(csr_i_rdwen),
    .csr_ena(csr_ena),
    .csr_idx(csr_idx),
    .csr_rd_en(csr_rd_en),
    .csr_wr_en(csr_wr_en),
    .read_csr_dat(read_csr_dat),
    .wbck_csr_dat(wbck_csr_dat),
    .csr_o_valid(csr_o_valid),
    .csr_o_ready(csr_o_ready),
    .csr_o_wbck_wdat(csr_o_wbck_wdat),
    .csr_o_wbck_err(csr_o_wbck_err)
  );
  // Branch/jump unit
  e203_exu_alu_bjp bjp (
    .clk(clk),
    .rst_n(rst_n),
    .bjp_i_valid(bjp_i_valid),
    .bjp_i_ready(bjp_i_ready),
    .bjp_i_rs1(bjp_i_rs1),
    .bjp_i_rs2(bjp_i_rs2),
    .bjp_i_info(bjp_i_info[16:0]),
    .bjp_i_imm(bjp_i_imm),
    .bjp_i_pc(bjp_i_pc),
    .bjp_o_valid(bjp_o_valid),
    .bjp_o_ready(bjp_o_ready),
    .bjp_o_wbck_wdat(bjp_o_wbck_wdat),
    .bjp_o_wbck_err(bjp_o_wbck_err),
    .bjp_o_cmt_bjp(bjp_o_cmt_bjp),
    .bjp_o_cmt_mret(bjp_o_cmt_mret),
    .bjp_o_cmt_dret(bjp_o_cmt_dret),
    .bjp_o_cmt_fencei(bjp_o_cmt_fencei),
    .bjp_o_cmt_prdt(bjp_o_cmt_prdt),
    .bjp_o_cmt_rslv(bjp_o_cmt_rslv),
    .bjp_req_alu_op1(bjp_req_alu_op1),
    .bjp_req_alu_op2(bjp_req_alu_op2),
    .bjp_req_alu_cmp_eq(bjp_req_alu_cmp_eq),
    .bjp_req_alu_cmp_ne(bjp_req_alu_cmp_ne),
    .bjp_req_alu_cmp_lt(bjp_req_alu_cmp_lt),
    .bjp_req_alu_cmp_gt(bjp_req_alu_cmp_gt),
    .bjp_req_alu_cmp_ltu(bjp_req_alu_cmp_ltu),
    .bjp_req_alu_cmp_gtu(bjp_req_alu_cmp_gtu),
    .bjp_req_alu_add(bjp_req_alu_add),
    .bjp_req_alu_cmp_res(bjp_req_alu_cmp_res),
    .bjp_req_alu_add_res(bjp_req_alu_add_res)
  );
  // AGU (load/store address generation)
  e203_exu_alu_lsuagu lsuagu (
    .clk(clk),
    .rst_n(rst_n),
    .agu_i_valid(agu_i_valid),
    .agu_i_ready(agu_i_ready),
    .agu_i_rs1(agu_i_rs1),
    .agu_i_rs2(agu_i_rs2),
    .agu_i_imm(agu_i_imm),
    .agu_i_info(agu_i_info[20:0]),
    .agu_i_longpipe(agu_i_longpipe),
    .agu_i_itag(agu_i_itag),
    .flush_pulse(flush_pulse),
    .flush_req(flush_req),
    .amo_wait(amo_wait),
    .oitf_empty(oitf_empty),
    .agu_o_valid(agu_o_valid),
    .agu_o_ready(agu_o_ready),
    .agu_o_wbck_wdat(agu_o_wbck_wdat),
    .agu_o_wbck_err(agu_o_wbck_err),
    .agu_o_cmt_misalgn(agu_o_cmt_misalgn),
    .agu_o_cmt_ld(agu_o_cmt_ld),
    .agu_o_cmt_stamo(agu_o_cmt_stamo),
    .agu_o_cmt_buserr(agu_o_cmt_buserr),
    .agu_o_cmt_badaddr(agu_o_cmt_badaddr),
    .agu_icb_cmd_valid(agu_icb_cmd_valid),
    .agu_icb_cmd_ready(agu_icb_cmd_ready),
    .agu_icb_cmd_addr(agu_icb_cmd_addr),
    .agu_icb_cmd_read(agu_icb_cmd_read),
    .agu_icb_cmd_wdata(agu_icb_cmd_wdata),
    .agu_icb_cmd_wmask(agu_icb_cmd_wmask),
    .agu_icb_cmd_lock(agu_icb_cmd_lock),
    .agu_icb_cmd_excl(agu_icb_cmd_excl),
    .agu_icb_cmd_size(agu_icb_cmd_size),
    .agu_icb_cmd_back2agu(agu_icb_cmd_back2agu),
    .agu_icb_cmd_usign(agu_icb_cmd_usign),
    .agu_icb_cmd_itag(agu_icb_cmd_itag),
    .agu_icb_rsp_valid(agu_icb_rsp_valid),
    .agu_icb_rsp_ready(agu_icb_rsp_ready),
    .agu_icb_rsp_err(agu_icb_rsp_err),
    .agu_icb_rsp_excl_ok(agu_icb_rsp_excl_ok),
    .agu_icb_rsp_rdata(agu_icb_rsp_rdata),
    .agu_req_alu_op1(agu_req_alu_op1),
    .agu_req_alu_op2(agu_req_alu_op2),
    .agu_req_alu_swap(agu_req_alu_swap),
    .agu_req_alu_add(agu_req_alu_add),
    .agu_req_alu_and(agu_req_alu_and),
    .agu_req_alu_or(agu_req_alu_or),
    .agu_req_alu_xor(agu_req_alu_xor),
    .agu_req_alu_max(agu_req_alu_max),
    .agu_req_alu_min(agu_req_alu_min),
    .agu_req_alu_maxu(agu_req_alu_maxu),
    .agu_req_alu_minu(agu_req_alu_minu),
    .agu_req_alu_res(agu_req_alu_res),
    .agu_sbf_0_ena(agu_sbf_0_ena),
    .agu_sbf_0_nxt(agu_sbf_0_nxt),
    .agu_sbf_0_r(agu_sbf_0_r),
    .agu_sbf_1_ena(agu_sbf_1_ena),
    .agu_sbf_1_nxt(agu_sbf_1_nxt),
    .agu_sbf_1_r(agu_sbf_1_r)
  );
  // Regular ALU
  e203_exu_alu_rglr rglr (
    .clk(clk),
    .rst_n(rst_n),
    .alu_i_valid(alu_i_valid),
    .alu_i_ready(alu_i_ready),
    .alu_i_rs1(alu_i_rs1),
    .alu_i_rs2(alu_i_rs2),
    .alu_i_info(alu_i_info[20:0]),
    .alu_i_imm(alu_i_imm),
    .alu_i_pc(alu_i_pc),
    .alu_o_valid(alu_o_valid),
    .alu_o_ready(alu_o_ready),
    .alu_o_wbck_wdat(alu_o_wbck_wdat),
    .alu_o_wbck_err(alu_o_wbck_err),
    .alu_o_cmt_ecall(alu_o_cmt_ecall),
    .alu_o_cmt_ebreak(alu_o_cmt_ebreak),
    .alu_o_cmt_wfi(alu_o_cmt_wfi),
    .alu_req_alu_add(alu_req_alu_add),
    .alu_req_alu_sub(alu_req_alu_sub),
    .alu_req_alu_xor(alu_req_alu_xor),
    .alu_req_alu_sll(alu_req_alu_sll),
    .alu_req_alu_srl(alu_req_alu_srl),
    .alu_req_alu_sra(alu_req_alu_sra),
    .alu_req_alu_or(alu_req_alu_or),
    .alu_req_alu_and(alu_req_alu_and),
    .alu_req_alu_slt(alu_req_alu_slt),
    .alu_req_alu_sltu(alu_req_alu_sltu),
    .alu_req_alu_lui(alu_req_alu_lui),
    .alu_req_alu_op1(alu_req_alu_op1),
    .alu_req_alu_op2(alu_req_alu_op2),
    .alu_req_alu_res(alu_req_alu_res)
  );
  // MulDiv (shared iterative multiplier/divider)
  e203_exu_alu_muldiv muldiv (
    .clk(clk),
    .rst_n(rst_n),
    .mdv_nob2b(mdv_nob2b),
    .muldiv_i_valid(mdv_i_valid),
    .muldiv_i_ready(mdv_i_ready),
    .muldiv_i_rs1(mdv_i_rs1),
    .muldiv_i_rs2(mdv_i_rs2),
    .muldiv_i_imm(mdv_i_imm),
    .muldiv_i_info(mdv_i_info[12:0]),
    .muldiv_i_longpipe(mdv_i_longpipe),
    .muldiv_i_itag(mdv_i_itag),
    .flush_pulse(flush_pulse),
    .muldiv_o_valid(mdv_o_valid),
    .muldiv_o_ready(mdv_o_ready),
    .muldiv_o_wbck_wdat(mdv_o_wbck_wdat),
    .muldiv_o_wbck_err(mdv_o_wbck_err),
    .muldiv_req_alu_op1(muldiv_req_alu_op1),
    .muldiv_req_alu_op2(muldiv_req_alu_op2),
    .muldiv_req_alu_add(muldiv_req_alu_add),
    .muldiv_req_alu_sub(muldiv_req_alu_sub),
    .muldiv_req_alu_res(muldiv_req_alu_res),
    .muldiv_sbf_0_ena(muldiv_sbf_0_ena),
    .muldiv_sbf_0_nxt(muldiv_sbf_0_nxt),
    .muldiv_sbf_0_r(muldiv_sbf_0_r),
    .muldiv_sbf_1_ena(muldiv_sbf_1_ena),
    .muldiv_sbf_1_nxt(muldiv_sbf_1_nxt),
    .muldiv_sbf_1_r(muldiv_sbf_1_r)
  );
  // Shared ALU datapath
  e203_exu_alu_dpath dpath (
    .clk(clk),
    .rst_n(rst_n),
    .alu_req_alu(alu_req_alu),
    .alu_req_alu_add(alu_req_alu_add),
    .alu_req_alu_sub(alu_req_alu_sub),
    .alu_req_alu_xor(alu_req_alu_xor),
    .alu_req_alu_sll(alu_req_alu_sll),
    .alu_req_alu_srl(alu_req_alu_srl),
    .alu_req_alu_sra(alu_req_alu_sra),
    .alu_req_alu_or(alu_req_alu_or),
    .alu_req_alu_and(alu_req_alu_and),
    .alu_req_alu_slt(alu_req_alu_slt),
    .alu_req_alu_sltu(alu_req_alu_sltu),
    .alu_req_alu_lui(alu_req_alu_lui),
    .alu_req_alu_op1(alu_req_alu_op1),
    .alu_req_alu_op2(alu_req_alu_op2),
    .alu_req_alu_res(alu_req_alu_res),
    .bjp_req_alu(bjp_req_alu),
    .bjp_req_alu_op1(bjp_req_alu_op1),
    .bjp_req_alu_op2(bjp_req_alu_op2),
    .bjp_req_alu_cmp_eq(bjp_req_alu_cmp_eq),
    .bjp_req_alu_cmp_ne(bjp_req_alu_cmp_ne),
    .bjp_req_alu_cmp_lt(bjp_req_alu_cmp_lt),
    .bjp_req_alu_cmp_gt(bjp_req_alu_cmp_gt),
    .bjp_req_alu_cmp_ltu(bjp_req_alu_cmp_ltu),
    .bjp_req_alu_cmp_gtu(bjp_req_alu_cmp_gtu),
    .bjp_req_alu_add(bjp_req_alu_add),
    .bjp_req_alu_cmp_res(bjp_req_alu_cmp_res),
    .bjp_req_alu_add_res(bjp_req_alu_add_res),
    .agu_req_alu(agu_req_alu),
    .agu_req_alu_op1(agu_req_alu_op1),
    .agu_req_alu_op2(agu_req_alu_op2),
    .agu_req_alu_swap(agu_req_alu_swap),
    .agu_req_alu_add(agu_req_alu_add),
    .agu_req_alu_and(agu_req_alu_and),
    .agu_req_alu_or(agu_req_alu_or),
    .agu_req_alu_xor(agu_req_alu_xor),
    .agu_req_alu_max(agu_req_alu_max),
    .agu_req_alu_min(agu_req_alu_min),
    .agu_req_alu_maxu(agu_req_alu_maxu),
    .agu_req_alu_minu(agu_req_alu_minu),
    .agu_req_alu_res(agu_req_alu_res),
    .agu_sbf_0_ena(agu_sbf_0_ena),
    .agu_sbf_0_nxt(agu_sbf_0_nxt),
    .agu_sbf_0_r(agu_sbf_0_r),
    .agu_sbf_1_ena(agu_sbf_1_ena),
    .agu_sbf_1_nxt(agu_sbf_1_nxt),
    .agu_sbf_1_r(agu_sbf_1_r),
    .muldiv_req_alu(muldiv_req_alu),
    .muldiv_req_alu_op1(muldiv_req_alu_op1),
    .muldiv_req_alu_op2(muldiv_req_alu_op2),
    .muldiv_req_alu_add(muldiv_req_alu_add),
    .muldiv_req_alu_sub(muldiv_req_alu_sub),
    .muldiv_req_alu_res(muldiv_req_alu_res),
    .muldiv_sbf_0_ena(muldiv_sbf_0_ena),
    .muldiv_sbf_0_nxt(muldiv_sbf_0_nxt),
    .muldiv_sbf_0_r(muldiv_sbf_0_r),
    .muldiv_sbf_1_ena(muldiv_sbf_1_ena),
    .muldiv_sbf_1_nxt(muldiv_sbf_1_nxt),
    .muldiv_sbf_1_r(muldiv_sbf_1_r)
  );
  // ── IFU exception passthrough ───────────────────────────────────────────
  logic ifu_excp_o_valid;
  assign ifu_excp_o_valid = ifu_excp_i_valid;
  logic [31:0] ifu_excp_o_wbck_wdat;
  assign ifu_excp_o_wbck_wdat = 0;
  logic ifu_excp_o_wbck_err;
  assign ifu_excp_o_wbck_err = 1'b1;
  // ── Result arbitration ──────────────────────────────────────────────────
  logic o_sel_ifu_excp;
  assign o_sel_ifu_excp = ifu_excp_op;
  logic o_sel_alu;
  assign o_sel_alu = alu_op;
  logic o_sel_bjp;
  assign o_sel_bjp = bjp_op;
  logic o_sel_csr;
  assign o_sel_csr = csr_op;
  logic o_sel_agu;
  assign o_sel_agu = agu_op;
  logic o_sel_mdv;
  assign o_sel_mdv = mdv_op;
  logic o_sel_nice;
  assign o_sel_nice = nice_op;
  // o_valid: OR of each target's valid gated by its select
  logic o_valid;
  logic o_ready;
  assign o_valid = (o_sel_alu & alu_o_valid) | (o_sel_bjp & bjp_o_valid) | (o_sel_csr & csr_o_valid) | (o_sel_agu & agu_o_valid) | (o_sel_ifu_excp & ifu_excp_o_valid) | (o_sel_mdv & mdv_o_valid) | (o_sel_nice & nice_o_valid);
  // Per-target ready: o_ready gated by select
  // ifu_excp_o_ready is a let, not wire, so we handle it differently
  // Per-target ready
  logic alu_o_ready_f;
  assign alu_o_ready_f = o_sel_alu & o_ready;
  logic agu_o_ready_f;
  assign agu_o_ready_f = o_sel_agu & o_ready;
  logic mdv_o_ready_f;
  assign mdv_o_ready_f = o_sel_mdv & o_ready;
  logic bjp_o_ready_f;
  assign bjp_o_ready_f = o_sel_bjp & o_ready;
  logic csr_o_ready_f;
  assign csr_o_ready_f = o_sel_csr & o_ready;
  logic nice_o_ready_f;
  assign nice_o_ready_f = o_sel_nice & o_ready;
  // Connect ready back to sub-modules (wire assignments need comb block)
  assign alu_o_ready = alu_o_ready_f;
  assign agu_o_ready = agu_o_ready_f;
  assign mdv_o_ready = mdv_o_ready_f;
  assign bjp_o_ready = bjp_o_ready_f;
  assign csr_o_ready = csr_o_ready_f;
  assign nice_o_ready = nice_o_ready_f;
  // wbck data mux (OR-based, matching reference)
  logic [31:0] wbck_o_wdat_f;
  assign wbck_o_wdat_f = (o_sel_alu ? alu_o_wbck_wdat : 0) | (o_sel_bjp ? bjp_o_wbck_wdat : 0) | (o_sel_csr ? csr_o_wbck_wdat : 0) | (o_sel_agu ? agu_o_wbck_wdat : 0) | (o_sel_mdv ? mdv_o_wbck_wdat : 0) | (o_sel_ifu_excp ? ifu_excp_o_wbck_wdat : 0);
  // wbck error OR
  logic wbck_o_err;
  assign wbck_o_err = (o_sel_alu & alu_o_wbck_err) | (o_sel_bjp & bjp_o_wbck_err) | (o_sel_csr & csr_o_wbck_err) | (o_sel_agu & agu_o_wbck_err) | (o_sel_mdv & mdv_o_wbck_err) | (o_sel_ifu_excp & ifu_excp_o_wbck_err) | (o_sel_nice & i_nice_cmt_off_ilgl);
  logic wbck_o_rdwen;
  assign wbck_o_rdwen = i_rdwen;
  // o_need_wbck: write to RD, not a long-pipe op, no error
  logic o_need_wbck;
  assign o_need_wbck = wbck_o_rdwen & ~i_longpipe & ~wbck_o_err;
  logic o_need_cmt;
  assign o_need_cmt = 1'b1;
  assign o_ready = (o_need_cmt ? cmt_o_ready : 1'b1) & (o_need_wbck ? wbck_o_ready : 1'b1);
  assign ifu_excp_i_ready = o_sel_ifu_excp & o_ready;
  assign wbck_o_valid = o_need_wbck & o_valid & (o_need_cmt ? cmt_o_ready : 1'b1);
  assign cmt_o_valid = o_need_cmt & o_valid & (o_need_wbck ? wbck_o_ready : 1'b1);
  assign wbck_o_wdat = wbck_o_wdat_f;
  assign wbck_o_rdidx = i_rdidx;
  assign cmt_o_instr = i_instr;
  assign cmt_o_pc = i_pc;
  assign cmt_o_imm = i_imm;
  assign cmt_o_rv32 = i_info[3:3] != 0;
  assign cmt_o_pc_vld = i_pc_vld;
  assign cmt_o_misalgn = o_sel_agu & agu_o_cmt_misalgn;
  assign cmt_o_ld = o_sel_agu & agu_o_cmt_ld;
  assign cmt_o_badaddr = o_sel_agu ? agu_o_cmt_badaddr : 0;
  assign cmt_o_buserr = o_sel_agu & agu_o_cmt_buserr;
  assign cmt_o_stamo = o_sel_agu & agu_o_cmt_stamo;
  assign cmt_o_bjp = o_sel_bjp & bjp_o_cmt_bjp;
  assign cmt_o_mret = o_sel_bjp & bjp_o_cmt_mret;
  assign cmt_o_dret = o_sel_bjp & bjp_o_cmt_dret;
  assign cmt_o_bjp_prdt = o_sel_bjp & bjp_o_cmt_prdt;
  assign cmt_o_bjp_rslv = o_sel_bjp & bjp_o_cmt_rslv;
  assign cmt_o_fencei = o_sel_bjp & bjp_o_cmt_fencei;
  assign cmt_o_ecall = o_sel_alu & alu_o_cmt_ecall;
  assign cmt_o_ebreak = o_sel_alu & alu_o_cmt_ebreak;
  assign cmt_o_wfi = o_sel_alu & alu_o_cmt_wfi;
  assign cmt_o_ifu_misalgn = i_misalgn;
  assign cmt_o_ifu_buserr = i_buserr;
  assign cmt_o_ifu_ilegl = i_ilegl | (o_sel_csr & csr_access_ilgl);

endmodule

// o_ready: both cmt and wbck must be ready when needed
// ifu_excp ready: equals its o_ready (which is o_sel_ifu_excp & o_ready)
// Commit passthrough
// AGU commit
// BJP commit
// RegALU commit
// IFU exception flags
// E203 NICE Co-processor Interface
// Routes custom instructions to external NICE accelerator.
// Includes itag FIFO for long-pipe tracking.
module e203_exu_nice (
  input logic clk,
  input logic rst_n,
  input logic nice_i_xs_off,
  input logic nice_i_valid,
  output logic nice_i_ready,
  input logic [31:0] nice_i_instr,
  input logic [31:0] nice_i_rs1,
  input logic [31:0] nice_i_rs2,
  input logic [0:0] nice_i_itag,
  output logic nice_o_longpipe,
  output logic nice_o_valid,
  input logic nice_o_ready,
  output logic nice_o_itag_valid,
  input logic nice_o_itag_ready,
  output logic [0:0] nice_o_itag,
  input logic nice_rsp_multicyc_valid,
  output logic nice_rsp_multicyc_ready,
  output logic nice_req_valid,
  input logic nice_req_ready,
  output logic [31:0] nice_req_instr,
  output logic [31:0] nice_req_rs1,
  output logic [31:0] nice_req_rs2
);

  // NICE extension disabled
  // Dispatch handshake
  // Output interface
  // Itag writeback (for long-pipe tracking)
  // Multi-cycle response
  // Request to coprocessor
  // Itag FIFO (4-deep, 1-bit wide)
  logic [3:0] [0:0] fifo_mem;
  logic [2:0] fifo_wptr;
  logic [2:0] fifo_rptr;
  logic fifo_empty;
  assign fifo_empty = fifo_wptr == fifo_rptr;
  logic fifo_full;
  assign fifo_full = (fifo_wptr[1:0] == fifo_rptr[1:0]) & (fifo_wptr[2:2] != fifo_rptr[2:2]);
  logic fifo_o_vld;
  assign fifo_o_vld = ~fifo_empty;
  logic [0:0] fifo_o_dat;
  assign fifo_o_dat = fifo_mem[fifo_rptr[1:0]];
  logic nice_req_ready_pos;
  assign nice_req_ready_pos = nice_i_xs_off ? 1'b1 : nice_req_ready;
  // FIFO write: when long-pipe request fires, drop if full (matches sirv_gnrl_fifo)
  logic fifo_wen;
  assign fifo_wen = nice_o_longpipe & nice_req_valid & nice_req_ready & ~fifo_full;
  // FIFO read: when multi-cycle response acknowledged
  logic fifo_ren;
  assign fifo_ren = nice_rsp_multicyc_valid & nice_rsp_multicyc_ready;
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      for (int __ri0 = 0; __ri0 < 4; __ri0++) begin
        fifo_mem[__ri0] <= 0;
      end
      fifo_rptr <= 0;
      fifo_wptr <= 0;
    end else begin
      if (fifo_wen) begin
        fifo_mem[fifo_wptr[1:0]] <= nice_i_itag;
        fifo_wptr <= 3'(fifo_wptr + 1);
      end
      if (fifo_ren) begin
        fifo_rptr <= 3'(fifo_rptr + 1);
      end
    end
  end
  assign nice_req_valid = ~nice_i_xs_off & nice_i_valid & nice_o_ready;
  assign nice_req_instr = nice_i_instr;
  assign nice_req_rs1 = nice_i_rs1;
  assign nice_req_rs2 = nice_i_rs2;
  assign nice_i_ready = nice_req_ready_pos & nice_o_ready;
  assign nice_o_valid = nice_i_valid & nice_req_ready_pos;
  assign nice_o_longpipe = ~nice_i_xs_off;
  assign nice_o_itag_valid = fifo_o_vld & nice_rsp_multicyc_valid;
  assign nice_o_itag = fifo_o_dat;
  assign nice_rsp_multicyc_ready = nice_o_itag_ready & fifo_o_vld;
  // synopsys translate_off
  // Auto-generated safety assertions (bounds / divide-by-zero)
  _auto_bound_vec_0: assert property (@(posedge clk) disable iff (!rst_n) (fifo_wptr[1:0]) < (4))
    else $fatal(1, "BOUNDS VIOLATION: e203_exu_nice._auto_bound_vec_0");
  // synopsys translate_on

endmodule

// E203 CSR Control Sub-unit
// Pure combinational: executes CSRRW/CSRRS/CSRRC instructions.
// Computes new CSR value from set/clear/write semantics.
module e203_exu_alu_csrctrl (
  input logic clk,
  input logic rst_n,
  input logic csr_i_valid,
  output logic csr_i_ready,
  input logic [31:0] csr_i_rs1,
  input logic [25:0] csr_i_info,
  input logic csr_i_rdwen,
  output logic csr_ena,
  output logic csr_wr_en,
  output logic csr_rd_en,
  output logic [11:0] csr_idx,
  input logic csr_access_ilgl,
  input logic [31:0] read_csr_dat,
  output logic [31:0] wbck_csr_dat,
  output logic csr_o_valid,
  input logic csr_o_ready,
  output logic [31:0] csr_o_wbck_wdat,
  output logic csr_o_wbck_err
);

  // Dispatch handshake
  // E203_DECINFO_CSR_WIDTH
  // rd write enable (is rd != x0?)
  // CSR register file interface
  // Result handshake
  // Decode info fields
  logic csrrw;
  assign csrrw = csr_i_info[4:4];
  logic csrrs;
  assign csrrs = csr_i_info[5:5];
  logic csrrc;
  assign csrrc = csr_i_info[6:6];
  logic rs1imm;
  assign rs1imm = csr_i_info[7:7];
  logic [4:0] zimm;
  assign zimm = csr_i_info[12:8];
  logic rs1is0;
  assign rs1is0 = csr_i_info[13:13];
  logic [11:0] csridx;
  assign csridx = csr_i_info[25:14];
  // Operand: zero-extended zimm or rs1
  logic [31:0] csr_op1;
  assign csr_op1 = rs1imm ? 32'($unsigned(zimm)) : csr_i_rs1;
  assign csr_o_valid = csr_i_valid;
  assign csr_i_ready = csr_o_ready;
  assign csr_o_wbck_wdat = read_csr_dat;
  assign csr_o_wbck_err = csr_access_ilgl;
  assign csr_idx = csridx;
  assign csr_ena = csr_o_valid & csr_o_ready;
  assign csr_rd_en = csr_i_valid & ((csrrw & csr_i_rdwen) | csrrs | csrrc);
  assign csr_wr_en = csr_i_valid & (csrrw | ((csrrs | csrrc) & ~rs1is0));
  assign wbck_csr_dat = (csr_op1 & {32{csrrw}}) | ((csr_op1 | read_csr_dat) & {32{csrrs}}) | (~csr_op1 & read_csr_dat & {32{csrrc}});

endmodule

// Pass-through handshake
// Writeback to register file = CSR read data
// CSR index passthrough
// CSR enable: fire when handshake completes
// Read enable: CSRRW reads only if rd written; CSRRS/CSRRC always read
// Write enable: CSRRW always writes; CSRRS/CSRRC write only if rs1 != x0
// Write data: CSRRW=direct, CSRRS=set bits, CSRRC=clear bits
// E203 HBirdv2 Branch/Jump Unit
// Decodes the 17-bit info bus to determine branch type, then issues
// ALU datapath requests. Purely combinational.
// Reference: e203_exu_alu_bjp_ref.sv
module e203_exu_alu_bjp #(
  parameter int XLEN = 32
) (
  input logic clk,
  input logic rst_n,
  input logic bjp_i_valid,
  output logic bjp_i_ready,
  input logic [31:0] bjp_i_rs1,
  input logic [31:0] bjp_i_rs2,
  input logic [31:0] bjp_i_imm,
  input logic [31:0] bjp_i_pc,
  input logic [16:0] bjp_i_info,
  output logic bjp_o_valid,
  input logic bjp_o_ready,
  output logic [31:0] bjp_o_wbck_wdat,
  output logic bjp_o_wbck_err,
  output logic bjp_o_cmt_bjp,
  output logic bjp_o_cmt_mret,
  output logic bjp_o_cmt_dret,
  output logic bjp_o_cmt_fencei,
  output logic bjp_o_cmt_prdt,
  output logic bjp_o_cmt_rslv,
  output logic [31:0] bjp_req_alu_op1,
  output logic [31:0] bjp_req_alu_op2,
  output logic bjp_req_alu_cmp_eq,
  output logic bjp_req_alu_cmp_ne,
  output logic bjp_req_alu_cmp_lt,
  output logic bjp_req_alu_cmp_gt,
  output logic bjp_req_alu_cmp_ltu,
  output logic bjp_req_alu_cmp_gtu,
  output logic bjp_req_alu_add,
  input logic bjp_req_alu_cmp_res,
  input logic [31:0] bjp_req_alu_add_res
);

  // ── Dispatch handshake ────────────────────────────────────────────
  // ── Writeback handshake ───────────────────────────────────────────
  // ── Commit signals ────────────────────────────────────────────────
  // ── ALU datapath request ──────────────────────────────────────────
  // ── ALU datapath results (from shared datapath) ───────────────────
  // ── Decode info bus ───────────────────────────────────────────────
  // Info bus layout (from e203_defines.v):
  //   info[3:0] = GRP (3 bits) + RV32 (1 bit)
  //   info[4]   = JUMP
  //   info[5]   = BPRDT
  //   info[6]   = BEQ
  //   info[7]   = BNE
  //   info[8]   = BLT
  //   info[9]   = BGT (= BGE in RISC-V)
  //   info[10]  = BLTU
  //   info[11]  = BGTU (= BGEU)
  //   info[12]  = BXX
  //   info[13]  = MRET
  //   info[14]  = DRET
  //   info[15]  = FENCE
  //   info[16]  = FENCEI
  logic jump;
  assign jump = bjp_i_info[4:4] != 0;
  logic bprdt;
  assign bprdt = bjp_i_info[5:5] != 0;
  logic beq;
  assign beq = bjp_i_info[6:6] != 0;
  logic bne;
  assign bne = bjp_i_info[7:7] != 0;
  logic blt;
  assign blt = bjp_i_info[8:8] != 0;
  logic bgt;
  assign bgt = bjp_i_info[9:9] != 0;
  // BGE in RISC-V
  logic bltu;
  assign bltu = bjp_i_info[10:10] != 0;
  logic bgtu;
  assign bgtu = bjp_i_info[11:11] != 0;
  // BGEU
  logic bxx;
  assign bxx = bjp_i_info[12:12] != 0;
  logic mret;
  assign mret = bjp_i_info[13:13] != 0;
  logic dret;
  assign dret = bjp_i_info[14:14] != 0;
  // let fence: Bool = bjp_i_info[15:15] != 0;  // unused in this module
  logic fencei;
  assign fencei = bjp_i_info[16:16] != 0;
  logic rv32;
  assign rv32 = bjp_i_info[3:3] != 0;
  // wbck_link: jump → compute PC+4/PC+2 for link address through ALU add
  logic wbck_link;
  assign wbck_link = jump;
  always_comb begin
    // ── Handshake passthrough ────────────────────────────────────
    bjp_o_valid = bjp_i_valid;
    bjp_i_ready = bjp_o_ready;
    bjp_o_wbck_err = 1'b0;
    // ALU op1: PC for jump (to compute PC+4 link), rs1 for branches
    if (wbck_link) begin
      bjp_req_alu_op1 = bjp_i_pc;
    end else begin
      bjp_req_alu_op1 = bjp_i_rs1;
    end
    // ALU op2: 4 or 2 for jump, rs2 for branches
    if (wbck_link) begin
      if (rv32) begin
        bjp_req_alu_op2 = 4;
      end else begin
        bjp_req_alu_op2 = 2;
      end
    end else begin
      bjp_req_alu_op2 = bjp_i_rs2;
    end
    // ── Commit outputs ───────────────────────────────────────────
    bjp_o_cmt_bjp = bxx | jump;
    bjp_o_cmt_mret = mret;
    bjp_o_cmt_dret = dret;
    bjp_o_cmt_fencei = fencei;
    bjp_o_cmt_prdt = bprdt;
    // Resolved: jumps always taken; branches depend on compare result
    if (jump) begin
      bjp_o_cmt_rslv = 1'b1;
    end else begin
      bjp_o_cmt_rslv = bjp_req_alu_cmp_res;
    end
    // ── ALU compare requests (one-hot from info bus) ─────────────
    bjp_req_alu_cmp_eq = beq;
    bjp_req_alu_cmp_ne = bne;
    bjp_req_alu_cmp_lt = blt;
    bjp_req_alu_cmp_gt = bgt;
    // BGE in reference
    bjp_req_alu_cmp_ltu = bltu;
    bjp_req_alu_cmp_gtu = bgtu;
    // BGEU in reference
    // ALU add: only for wbck_link (jump), not branches
    bjp_req_alu_add = wbck_link;
    // wbck_wdat from ALU add result (PC+4/PC+2 for link address)
    bjp_o_wbck_wdat = bjp_req_alu_add_res;
  end

endmodule

// E203 Load/Store Address Generation Unit
// Generates addresses for load/store/AMO instructions.
// AMO uses a multi-step state machine: read -> ALU compute -> write -> writeback.
module e203_exu_alu_lsuagu (
  input logic clk,
  input logic rst_n,
  input logic agu_i_valid,
  output logic agu_i_ready,
  input logic [31:0] agu_i_rs1,
  input logic [31:0] agu_i_rs2,
  input logic [31:0] agu_i_imm,
  input logic [20:0] agu_i_info,
  input logic [0:0] agu_i_itag,
  output logic agu_i_longpipe,
  input logic flush_req,
  input logic flush_pulse,
  output logic amo_wait,
  input logic oitf_empty,
  output logic agu_o_valid,
  input logic agu_o_ready,
  output logic [31:0] agu_o_wbck_wdat,
  output logic agu_o_wbck_err,
  output logic agu_o_cmt_misalgn,
  output logic agu_o_cmt_ld,
  output logic agu_o_cmt_stamo,
  output logic agu_o_cmt_buserr,
  output logic [31:0] agu_o_cmt_badaddr,
  output logic agu_icb_cmd_valid,
  input logic agu_icb_cmd_ready,
  output logic [31:0] agu_icb_cmd_addr,
  output logic agu_icb_cmd_read,
  output logic [31:0] agu_icb_cmd_wdata,
  output logic [3:0] agu_icb_cmd_wmask,
  output logic agu_icb_cmd_back2agu,
  output logic agu_icb_cmd_lock,
  output logic agu_icb_cmd_excl,
  output logic [1:0] agu_icb_cmd_size,
  output logic [0:0] agu_icb_cmd_itag,
  output logic agu_icb_cmd_usign,
  input logic agu_icb_rsp_valid,
  output logic agu_icb_rsp_ready,
  input logic agu_icb_rsp_err,
  input logic agu_icb_rsp_excl_ok,
  input logic [31:0] agu_icb_rsp_rdata,
  output logic [31:0] agu_req_alu_op1,
  output logic [31:0] agu_req_alu_op2,
  output logic agu_req_alu_swap,
  output logic agu_req_alu_add,
  output logic agu_req_alu_and,
  output logic agu_req_alu_or,
  output logic agu_req_alu_xor,
  output logic agu_req_alu_max,
  output logic agu_req_alu_min,
  output logic agu_req_alu_maxu,
  output logic agu_req_alu_minu,
  input logic [31:0] agu_req_alu_res,
  output logic agu_sbf_0_ena,
  output logic [31:0] agu_sbf_0_nxt,
  input logic [31:0] agu_sbf_0_r,
  output logic agu_sbf_1_ena,
  output logic [31:0] agu_sbf_1_nxt,
  input logic [31:0] agu_sbf_1_r
);

  // Dispatch handshake
  // Flush
  // AMO state
  // Result handshake
  // ICB command interface
  // ICB response interface
  // Shared ALU datapath
  // Shared buffers
  // Decode info fields (from agu_i_info)
  logic i_load;
  assign i_load = agu_i_info[4:4];
  logic i_store;
  assign i_store = agu_i_info[5:5];
  logic [1:0] i_size;
  assign i_size = agu_i_info[7:6];
  logic i_usign;
  assign i_usign = agu_i_info[8:8];
  logic i_excl;
  assign i_excl = agu_i_info[9:9];
  logic i_amo;
  assign i_amo = agu_i_info[10:10];
  logic i_amoswap;
  assign i_amoswap = agu_i_info[11:11];
  logic i_amoadd;
  assign i_amoadd = agu_i_info[12:12];
  logic i_amoand;
  assign i_amoand = agu_i_info[13:13];
  logic i_amoor;
  assign i_amoor = agu_i_info[14:14];
  logic i_amoxor;
  assign i_amoxor = agu_i_info[15:15];
  logic i_amomax;
  assign i_amomax = agu_i_info[16:16];
  logic i_amomin;
  assign i_amomin = agu_i_info[17:17];
  logic i_amomaxu;
  assign i_amomaxu = agu_i_info[18:18];
  logic i_amominu;
  assign i_amominu = agu_i_info[19:19];
  logic size_b;
  assign size_b = i_size == 0;
  logic size_hw;
  assign size_hw = i_size == 1;
  logic size_w;
  assign size_w = i_size == 2;
  // AMO ICB state machine
  // IDLE=0, 1ST=1, WAIT2ND=2, 2ND=3, AMOALU=4, AMORDY=5, WBCK=6
  logic [3:0] icb_state_r;
  logic sta_idle;
  assign sta_idle = icb_state_r == 0;
  logic sta_1st;
  assign sta_1st = icb_state_r == 1;
  logic sta_wait2nd;
  assign sta_wait2nd = icb_state_r == 2;
  logic sta_2nd;
  assign sta_2nd = icb_state_r == 3;
  logic sta_amoalu;
  assign sta_amoalu = icb_state_r == 4;
  logic sta_amordy;
  assign sta_amordy = icb_state_r == 5;
  logic sta_wbck;
  assign sta_wbck = icb_state_r == 6;
  logic flush_block;
  assign flush_block = flush_req & sta_idle;
  logic ld;
  assign ld = i_load & ~flush_block;
  logic st;
  assign st = i_store & ~flush_block;
  logic amo;
  assign amo = i_amo & ~flush_block;
  logic ofst0;
  assign ofst0 = amo | ((ld | st) & i_excl);
  // unalgn_flg_r: captures alignment at state machine entry, held until exit
  logic unalgn_flg_r;
  // Address alignment check (uses agu_req_alu_res = computed address)
  logic addr_unalgn_i;
  // combinational alignment (idle only)
  logic agu_addr_unalgn;
  // muxed: comb in idle, registered otherwise
  logic algnld;
  logic algnst;
  logic algn_ldst;
  logic algn_amo;
  logic unalgn_ldst;
  logic unalgn_amo;
  // Store data/mask
  logic [31:0] algnst_wdata;
  logic [3:0] algnst_wmask;
  // Address generation offset
  logic [31:0] addr_gen_op2;
  assign addr_gen_op2 = ofst0 ? 0 : agu_i_imm;
  // ICB handshake signals
  logic icb_cmd_hsked;
  assign icb_cmd_hsked = agu_icb_cmd_valid & agu_icb_cmd_ready;
  logic icb_rsp_hsked;
  assign icb_rsp_hsked = agu_icb_rsp_valid & agu_icb_rsp_ready;
  // AMO uop flags
  logic amo_1stuop;
  assign amo_1stuop = sta_1st & algn_amo;
  logic amo_2nduop;
  assign amo_2nduop = sta_2nd & algn_amo;
  // Leftover buffer (shared with sbf_0)
  logic leftover_ena;
  logic [31:0] leftover_nxt;
  logic [31:0] leftover_r;
  assign leftover_r = agu_sbf_0_r;
  // Leftover error tracking
  logic leftover_err_ena;
  logic leftover_err_nxt;
  logic leftover_err_r;
  // Leftover_1 buffer (shared with sbf_1) for ALU result
  logic leftover_1_ena;
  logic [31:0] leftover_1_nxt;
  logic [31:0] leftover_1_r;
  assign leftover_1_r = agu_sbf_1_r;
  // State machine exit enable signals
  logic state_idle_exit_ena;
  assign state_idle_exit_ena = sta_idle & algn_amo & oitf_empty & icb_cmd_hsked & ~flush_pulse;
  logic state_1st_exit_ena;
  assign state_1st_exit_ena = sta_1st & (icb_rsp_hsked | flush_pulse);
  logic state_amoalu_exit_ena;
  assign state_amoalu_exit_ena = sta_amoalu;
  logic state_amordy_exit_ena;
  assign state_amordy_exit_ena = sta_amordy;
  logic state_wait2nd_exit_ena;
  assign state_wait2nd_exit_ena = sta_wait2nd & (agu_icb_cmd_ready | flush_pulse);
  logic state_2nd_exit_ena;
  assign state_2nd_exit_ena = sta_2nd & (icb_rsp_hsked | flush_pulse);
  logic state_wbck_exit_ena;
  assign state_wbck_exit_ena = sta_wbck & (agu_o_ready | flush_pulse);
  logic state_last_exit_ena;
  assign state_last_exit_ena = state_wbck_exit_ena;
  // State machine update
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      icb_state_r <= 0;
    end else begin
      if (state_idle_exit_ena) begin
        icb_state_r <= 1;
      end else if (state_1st_exit_ena) begin
        icb_state_r <= flush_pulse ? 0 : 4;
      end else if (state_amoalu_exit_ena) begin
        icb_state_r <= flush_pulse ? 0 : 5;
      end else if (state_amordy_exit_ena) begin
        icb_state_r <= flush_pulse ? 0 : 2;
      end else if (state_wait2nd_exit_ena) begin
        icb_state_r <= flush_pulse ? 0 : 3;
      end else if (state_2nd_exit_ena) begin
        icb_state_r <= flush_pulse ? 0 : 6;
      end else if (state_wbck_exit_ena) begin
        icb_state_r <= 0;
      end
    end
  end
  // Leftover error register
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      leftover_err_r <= 1'b0;
    end else begin
      if (leftover_err_ena) begin
        leftover_err_r <= leftover_err_nxt;
      end
    end
  end
  // unalgn_flg_r: captures alignment at FSM entry, cleared at exit
  logic unalgn_flg_set;
  logic unalgn_flg_clr;
  logic unalgn_flg_ena;
  logic unalgn_flg_nxt_w;
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      unalgn_flg_r <= 1'b0;
    end else begin
      if (unalgn_flg_ena) begin
        unalgn_flg_r <= unalgn_flg_nxt_w;
      end
    end
  end
  assign addr_unalgn_i = (size_hw & agu_icb_cmd_addr[0:0]) | (size_w & (agu_icb_cmd_addr[1:0] != 0));
  assign agu_addr_unalgn = sta_idle ? addr_unalgn_i : unalgn_flg_r;
  assign unalgn_flg_set = addr_unalgn_i & state_idle_exit_ena;
  assign unalgn_flg_clr = unalgn_flg_r & state_last_exit_ena;
  assign unalgn_flg_ena = unalgn_flg_set | unalgn_flg_clr;
  assign unalgn_flg_nxt_w = unalgn_flg_set | ~unalgn_flg_clr;
  assign algnld = ~agu_addr_unalgn & ld;
  assign algnst = ~agu_addr_unalgn & st;
  assign algn_ldst = algnld | algnst;
  assign algn_amo = ~agu_addr_unalgn & amo;
  assign unalgn_ldst = agu_addr_unalgn & (ld | st);
  assign unalgn_amo = agu_addr_unalgn & amo;
  assign algnst_wdata = size_b ? {4{agu_i_rs2[7:0]}} : size_hw ? {2{agu_i_rs2[15:0]}} : size_w ? agu_i_rs2 : 0;
  assign algnst_wmask = size_b ? 4'd1 << agu_icb_cmd_addr[1:0] : size_hw ? 4'd3 << {agu_icb_cmd_addr[1:1], 1'd0} : size_w ? 4'd15 : 0;
  assign agu_req_alu_op1 = sta_idle ? agu_i_rs1 : sta_amoalu ? leftover_r : i_amo & (sta_wait2nd | sta_2nd | sta_wbck) ? agu_i_rs1 : 0;
  assign agu_req_alu_op2 = sta_idle ? addr_gen_op2 : sta_amoalu ? agu_i_rs2 : i_amo & (sta_wait2nd | sta_2nd | sta_wbck) ? addr_gen_op2 : 0;
  assign agu_req_alu_add = (sta_amoalu & i_amoadd) | (i_amo & (sta_wait2nd | sta_2nd | sta_wbck)) | sta_idle;
  assign agu_req_alu_swap = sta_amoalu & i_amoswap;
  assign agu_req_alu_and = sta_amoalu & i_amoand;
  assign agu_req_alu_or = sta_amoalu & i_amoor;
  assign agu_req_alu_xor = sta_amoalu & i_amoxor;
  assign agu_req_alu_max = sta_amoalu & i_amomax;
  assign agu_req_alu_min = sta_amoalu & i_amomin;
  assign agu_req_alu_maxu = sta_amoalu & i_amomaxu;
  assign agu_req_alu_minu = sta_amoalu & i_amominu;
  assign leftover_ena = icb_rsp_hsked & (amo_1stuop | amo_2nduop);
  assign leftover_nxt = amo_1stuop ? agu_icb_rsp_rdata : amo_2nduop ? leftover_r : 0;
  assign leftover_err_ena = leftover_ena;
  assign leftover_err_nxt = (amo_1stuop & agu_icb_rsp_err) | (amo_2nduop & (agu_icb_rsp_err | leftover_err_r));
  assign agu_sbf_0_ena = leftover_ena;
  assign agu_sbf_0_nxt = leftover_nxt;
  assign leftover_1_ena = sta_amoalu;
  assign leftover_1_nxt = agu_req_alu_res;
  assign agu_sbf_1_ena = leftover_1_ena;
  assign agu_sbf_1_nxt = leftover_1_nxt;
  assign agu_icb_cmd_valid = (algn_ldst & agu_i_valid & agu_o_ready) | (algn_amo & ((sta_idle & agu_i_valid & agu_o_ready) | sta_wait2nd)) | (unalgn_amo & 1'b0);
  assign agu_icb_cmd_addr = agu_req_alu_res;
  assign agu_icb_cmd_read = (algn_ldst & ld) | (algn_amo & sta_idle);
  assign agu_icb_cmd_wdata = amo ? leftover_1_r : algnst_wdata;
  assign agu_icb_cmd_wmask = amo ? leftover_err_r ? 0 : 4'd15 : algnst_wmask;
  assign agu_icb_cmd_back2agu = algn_amo;
  assign agu_icb_cmd_lock = algn_amo & sta_idle;
  assign agu_icb_cmd_excl = i_excl;
  assign agu_icb_cmd_size = i_size;
  assign agu_icb_cmd_itag = agu_i_itag;
  assign agu_icb_cmd_usign = i_usign;
  assign agu_icb_rsp_ready = 1'b1;
  assign agu_o_valid = sta_wbck | (agu_i_valid & (algn_ldst | unalgn_ldst | unalgn_amo) & agu_icb_cmd_ready);
  assign agu_o_wbck_wdat = algn_amo ? leftover_r : 0;
  assign agu_o_cmt_misalgn = unalgn_amo | unalgn_ldst;
  assign agu_o_cmt_ld = ld & ~i_excl;
  assign agu_o_cmt_stamo = st | amo | i_excl;
  assign agu_o_cmt_buserr = algn_amo & leftover_err_r;
  assign agu_o_cmt_badaddr = agu_icb_cmd_addr;
  assign agu_o_wbck_err = agu_o_cmt_buserr | agu_o_cmt_misalgn;
  assign agu_i_ready = algn_amo ? state_last_exit_ena : agu_icb_cmd_ready & agu_o_ready;
  assign agu_i_longpipe = algn_ldst;
  assign amo_wait = ~sta_idle;

endmodule

// Address alignment (combinational — uses agu_icb_cmd_addr = computed address)
// Muxed alignment: combinational in idle, registered otherwise (matches reference)
// unalgn_flg control
// Store data: byte/half replicated (0 for invalid size, matching reference)
// Store mask: shifted based on address LSBs (0 for invalid size)
// ALU operand 1
// ALU operand 2
// ALU operation selection
// Leftover buffer 0: loaded on response handshake during AMO 1st or 2nd uop
// Leftover error: merge errors from both uops
// Leftover buffer 1: ALU result saved during AMOALU
// ICB command valid
// Output valid: AMO wbck state OR (normal ldst/unalgn at dispatch with cmd_ready)
// Output data
// Commit signals (use flush-blocked versions matching reference)
// Ready: AMO goes through state machine, others go directly
// E203 Regular ALU Sub-unit
// Pure combinational: decodes info bits, selects operands (rs1/pc, rs2/imm),
// routes to shared ALU datapath, passes result back. ecall/ebreak/wfi set error.
module e203_exu_alu_rglr (
  input logic clk,
  input logic rst_n,
  input logic alu_i_valid,
  output logic alu_i_ready,
  input logic [31:0] alu_i_rs1,
  input logic [31:0] alu_i_rs2,
  input logic [31:0] alu_i_imm,
  input logic [31:0] alu_i_pc,
  input logic [20:0] alu_i_info,
  output logic alu_o_valid,
  input logic alu_o_ready,
  output logic [31:0] alu_o_wbck_wdat,
  output logic alu_o_wbck_err,
  output logic alu_o_cmt_ecall,
  output logic alu_o_cmt_ebreak,
  output logic alu_o_cmt_wfi,
  output logic alu_req_alu_add,
  output logic alu_req_alu_sub,
  output logic alu_req_alu_xor,
  output logic alu_req_alu_sll,
  output logic alu_req_alu_srl,
  output logic alu_req_alu_sra,
  output logic alu_req_alu_or,
  output logic alu_req_alu_and,
  output logic alu_req_alu_slt,
  output logic alu_req_alu_sltu,
  output logic alu_req_alu_lui,
  output logic [31:0] alu_req_alu_op1,
  output logic [31:0] alu_req_alu_op2,
  input logic [31:0] alu_req_alu_res
);

  // Dispatch handshake
  // E203_DECINFO_ALU_WIDTH
  // Result handshake
  // Exception signals
  // Shared ALU datapath request
  // Decode info fields
  logic op2imm;
  assign op2imm = alu_i_info[15:15];
  logic op1pc;
  assign op1pc = alu_i_info[16:16];
  logic nop;
  assign nop = alu_i_info[17:17];
  logic ecall;
  assign ecall = alu_i_info[18:18];
  logic ebreak;
  assign ebreak = alu_i_info[19:19];
  logic wfi;
  assign wfi = alu_i_info[20:20];
  assign alu_req_alu_op1 = op1pc ? alu_i_pc : alu_i_rs1;
  assign alu_req_alu_op2 = op2imm ? alu_i_imm : alu_i_rs2;
  assign alu_req_alu_add = alu_i_info[4:4] & ~nop;
  assign alu_req_alu_sub = alu_i_info[5:5];
  assign alu_req_alu_xor = alu_i_info[6:6];
  assign alu_req_alu_sll = alu_i_info[7:7];
  assign alu_req_alu_srl = alu_i_info[8:8];
  assign alu_req_alu_sra = alu_i_info[9:9];
  assign alu_req_alu_or = alu_i_info[10:10];
  assign alu_req_alu_and = alu_i_info[11:11];
  assign alu_req_alu_slt = alu_i_info[12:12];
  assign alu_req_alu_sltu = alu_i_info[13:13];
  assign alu_req_alu_lui = alu_i_info[14:14];
  assign alu_o_valid = alu_i_valid;
  assign alu_i_ready = alu_o_ready;
  assign alu_o_wbck_wdat = alu_req_alu_res;
  assign alu_o_cmt_ecall = ecall;
  assign alu_o_cmt_ebreak = ebreak;
  assign alu_o_cmt_wfi = wfi;
  assign alu_o_wbck_err = ecall | ebreak | wfi;

endmodule

// Operand selection
// Operation select (one-hot from info bits)
// Pass-through handshake
// Exception signals
// E203 MUL/DIV Sub-unit (Iterative, Shared Adder)
// 17-cycle Booth-4 multiply, 33-cycle non-restoring divide + correction.
// Uses shared 35-bit adder and two 33-bit buffers from the ALU top.
module e203_exu_alu_muldiv (
  input logic clk,
  input logic rst_n,
  input logic mdv_nob2b,
  input logic muldiv_i_valid,
  output logic muldiv_i_ready,
  input logic [31:0] muldiv_i_rs1,
  input logic [31:0] muldiv_i_rs2,
  input logic [31:0] muldiv_i_imm,
  input logic [12:0] muldiv_i_info,
  input logic [0:0] muldiv_i_itag,
  output logic muldiv_i_longpipe,
  input logic flush_pulse,
  output logic muldiv_o_valid,
  input logic muldiv_o_ready,
  output logic [31:0] muldiv_o_wbck_wdat,
  output logic muldiv_o_wbck_err,
  output logic [34:0] muldiv_req_alu_op1,
  output logic [34:0] muldiv_req_alu_op2,
  output logic muldiv_req_alu_add,
  output logic muldiv_req_alu_sub,
  input logic [34:0] muldiv_req_alu_res,
  output logic muldiv_sbf_0_ena,
  output logic [32:0] muldiv_sbf_0_nxt,
  input logic [32:0] muldiv_sbf_0_r,
  output logic muldiv_sbf_1_ena,
  output logic [32:0] muldiv_sbf_1_nxt,
  input logic [32:0] muldiv_sbf_1_r
);

  // Dispatch handshake
  // Result handshake
  // Shared 35-bit adder
  // Shared buffers (33-bit each)
  // Decode info fields
  logic i_mul;
  assign i_mul = muldiv_i_info[4:4];
  logic i_mulh;
  assign i_mulh = muldiv_i_info[5:5];
  logic i_mulhsu;
  assign i_mulhsu = muldiv_i_info[6:6];
  logic i_mulhu;
  assign i_mulhu = muldiv_i_info[7:7];
  logic i_div;
  assign i_div = muldiv_i_info[8:8];
  logic i_divu;
  assign i_divu = muldiv_i_info[9:9];
  logic i_rem;
  assign i_rem = muldiv_i_info[10:10];
  logic i_remu;
  assign i_remu = muldiv_i_info[11:11];
  logic i_b2b;
  assign i_b2b = muldiv_i_info[12:12];
  logic is_mul;
  assign is_mul = i_mul | i_mulh | i_mulhsu | i_mulhu;
  logic is_div;
  assign is_div = i_div | i_divu | i_rem | i_remu;
  // Signed handling
  logic mul_rs1_sign;
  assign mul_rs1_sign = i_mulhu ? 1'b0 : muldiv_i_rs1[31:31];
  logic mul_rs2_sign;
  assign mul_rs2_sign = i_mulhsu | i_mulhu ? 1'b0 : muldiv_i_rs2[31:31];
  logic div_rs1_sign;
  assign div_rs1_sign = i_divu | i_remu ? 1'b0 : muldiv_i_rs1[31:31];
  logic div_rs2_sign;
  assign div_rs2_sign = i_divu | i_remu ? 1'b0 : muldiv_i_rs2[31:31];
  // States: 0TH=0, EXEC=1, REMD_CHCK=2, QUOT_CORR=3, REMD_CORR=4
  logic [2:0] state_r;
  logic [5:0] exec_cnt_r;
  logic flushed_r;
  logic part_prdt_sft1_r;
  logic part_remd_sft1_r;
  logic sta_0th;
  assign sta_0th = state_r == 0;
  logic sta_exec;
  assign sta_exec = state_r == 1;
  logic sta_remd_chck;
  assign sta_remd_chck = state_r == 2;
  logic sta_quot_corr;
  assign sta_quot_corr = state_r == 3;
  logic sta_remd_corr;
  assign sta_remd_corr = state_r == 4;
  logic o_hsked;
  assign o_hsked = muldiv_o_valid & muldiv_o_ready;
  logic back2back_seq;
  assign back2back_seq = i_b2b & ~flushed_r & ~mdv_nob2b;
  // Div special cases
  logic div_by_0;
  assign div_by_0 = muldiv_i_rs2 == 0;
  logic div_ovf;
  assign div_ovf = (i_div | i_rem) & (muldiv_i_rs2 == 32'd4294967295) & muldiv_i_rs1[31:31] & (muldiv_i_rs1[30:0] == 0);
  logic special_cases;
  assign special_cases = is_div & (div_by_0 | div_ovf);
  logic muldiv_i_valid_nb2b;
  assign muldiv_i_valid_nb2b = muldiv_i_valid & ~back2back_seq & ~special_cases;
  // Cycle counting
  logic cycle_0th;
  assign cycle_0th = sta_0th;
  logic cycle_16th;
  assign cycle_16th = exec_cnt_r == 16;
  logic cycle_32nd;
  assign cycle_32nd = exec_cnt_r == 32;
  logic exec_last;
  assign exec_last = is_mul ? cycle_16th : cycle_32nd;
  // State exit enables
  logic state_0th_exit_ena;
  assign state_0th_exit_ena = sta_0th & muldiv_i_valid_nb2b & ~flush_pulse;
  logic state_exec_exit_ena;
  assign state_exec_exit_ena = sta_exec & ((exec_last & (is_div | o_hsked)) | flush_pulse);
  logic state_quot_corr_exit_ena;
  assign state_quot_corr_exit_ena = sta_quot_corr;
  logic state_remd_corr_exit_ena;
  assign state_remd_corr_exit_ena = sta_remd_corr & (flush_pulse | o_hsked);
  logic state_exec_enter_ena;
  assign state_exec_enter_ena = state_0th_exit_ena;
  // Aliases to shared buffers
  logic [32:0] part_prdt_hi_r;
  assign part_prdt_hi_r = muldiv_sbf_0_r;
  logic [32:0] part_prdt_lo_r;
  assign part_prdt_lo_r = muldiv_sbf_1_r;
  logic [32:0] part_remd_r;
  assign part_remd_r = muldiv_sbf_0_r;
  logic [32:0] part_quot_r;
  assign part_quot_r = muldiv_sbf_1_r;
  // All intermediate wires
  logic div_need_corrct;
  logic state_remd_chck_exit_ena;
  logic [2:0] booth_code;
  logic booth_sel_zero;
  logic booth_sel_two;
  logic booth_sel_one;
  logic booth_sel_sub;
  logic [34:0] mul_exe_alu_op1;
  logic [34:0] mul_exe_alu_op2;
  logic mul_exe_alu_add;
  logic mul_exe_alu_sub;
  logic [65:0] dividend;
  logic [33:0] divisor;
  logic quot_0cycl;
  logic [66:0] dividend_lsft1;
  logic prev_quot;
  logic current_quot;
  logic [33:0] div_exe_alu_op1;
  logic [33:0] div_exe_alu_op2;
  logic div_exe_alu_add;
  logic div_exe_alu_sub;
  logic [33:0] div_exe_alu_res;
  logic [66:0] div_exe_part_remd;
  logic [67:0] div_exe_part_remd_lsft1;
  logic corrct_phase;
  logic check_phase;
  logic [32:0] div_remd;
  logic [32:0] div_quot;
  logic remd_is_0;
  logic [33:0] div_remd_chck_alu_res_w;
  logic remd_is_neg_divs;
  logic remd_is_divs;
  logic remd_inc_quot_dec;
  logic [33:0] div_remd_chck_alu_op1;
  logic [33:0] div_remd_chck_alu_op2;
  logic [33:0] div_quot_corr_alu_op1;
  logic [33:0] div_quot_corr_alu_op2;
  logic div_quot_corr_alu_add;
  logic div_quot_corr_alu_sub;
  logic [33:0] div_remd_corr_alu_op1;
  logic [33:0] div_remd_corr_alu_op2;
  logic div_remd_corr_alu_add;
  logic div_remd_corr_alu_sub;
  logic [32:0] part_prdt_hi_nxt;
  logic [32:0] part_prdt_lo_nxt;
  logic [32:0] part_remd_nxt;
  logic [32:0] part_quot_nxt;
  logic mul_exe_cnt_set;
  logic mul_exe_cnt_inc;
  logic div_exe_cnt_set;
  logic div_exe_cnt_inc;
  logic part_prdt_hi_ena;
  logic part_remd_ena;
  logic part_quot_ena;
  logic req_alu_sel1;
  logic req_alu_sel2;
  logic req_alu_sel3;
  logic req_alu_sel4;
  logic req_alu_sel5;
  logic [31:0] mul_res;
  logic [31:0] div_res;
  logic [31:0] div_special_res;
  logic [31:0] back2back_res;
  logic wbck_condi;
  // State register
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      state_r <= 0;
    end else begin
      if (state_0th_exit_ena) begin
        state_r <= 1;
      end else if (state_exec_exit_ena) begin
        state_r <= flush_pulse ? 0 : is_div ? 2 : 0;
      end else if (state_remd_chck_exit_ena) begin
        state_r <= flush_pulse ? 0 : div_need_corrct ? 3 : 0;
      end else if (state_quot_corr_exit_ena) begin
        state_r <= flush_pulse ? 0 : 4;
      end else if (state_remd_corr_exit_ena) begin
        state_r <= 0;
      end
    end
  end
  // Exec counter
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      exec_cnt_r <= 0;
    end else begin
      if (state_exec_enter_ena) begin
        exec_cnt_r <= 1;
      end else if (sta_exec & ~exec_last) begin
        exec_cnt_r <= 6'(exec_cnt_r + 1);
      end else if (state_exec_exit_ena) begin
        exec_cnt_r <= 0;
      end
    end
  end
  // Flushed flag
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      flushed_r <= 1'b0;
    end else begin
      if (flush_pulse) begin
        flushed_r <= 1'b1;
      end else if (o_hsked & ~flush_pulse) begin
        flushed_r <= 1'b0;
      end
    end
  end
  // Part product shift1 register
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      part_prdt_sft1_r <= 1'b0;
    end else begin
      if ((is_mul & (state_exec_enter_ena | (sta_exec & ~exec_last))) | state_exec_exit_ena) begin
        part_prdt_sft1_r <= cycle_0th ? muldiv_i_rs1[1:1] : part_prdt_lo_r[1:1];
      end
    end
  end
  // Part remainder shift1 register
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      part_remd_sft1_r <= 1'b0;
    end else begin
      if ((is_div & (state_exec_enter_ena | (sta_exec & ~exec_last))) | state_exec_exit_ena | state_remd_corr_exit_ena) begin
        part_remd_sft1_r <= muldiv_req_alu_res[32:32];
      end
    end
  end
  assign booth_code = cycle_0th ? {muldiv_i_rs1[1:0], 1'd0} : cycle_16th ? {mul_rs1_sign, part_prdt_lo_r[0:0], part_prdt_sft1_r} : {part_prdt_lo_r[1:0], part_prdt_sft1_r};
  assign booth_sel_zero = (booth_code == 0) | (booth_code == 7);
  assign booth_sel_two = (booth_code == 3) | (booth_code == 4);
  assign booth_sel_one = ~booth_sel_zero & ~booth_sel_two;
  assign booth_sel_sub = booth_code[2:2];
  assign mul_exe_alu_op1 = cycle_0th ? 0 : {part_prdt_hi_r[32:32], part_prdt_hi_r[32:32], part_prdt_hi_r};
  assign mul_exe_alu_op2 = (booth_sel_one ? {mul_rs2_sign, mul_rs2_sign, mul_rs2_sign, muldiv_i_rs2} : 0) | (booth_sel_two ? {mul_rs2_sign, mul_rs2_sign, muldiv_i_rs2, 1'd0} : 0);
  assign mul_exe_alu_add = ~booth_sel_sub;
  assign mul_exe_alu_sub = booth_sel_sub;
  assign dividend = {{33{div_rs1_sign}}, div_rs1_sign, muldiv_i_rs1};
  assign divisor = {div_rs2_sign, div_rs2_sign, muldiv_i_rs2};
  assign quot_0cycl = dividend[65:65] ^ divisor[33:33] ? 1'b0 : 1'b1;
  assign dividend_lsft1 = {dividend[65:0], quot_0cycl};
  assign prev_quot = cycle_0th ? quot_0cycl : part_quot_r[0:0];
  assign div_exe_alu_op1 = cycle_0th ? dividend_lsft1[66:33] : {part_remd_sft1_r, part_remd_r[32:0]};
  assign div_exe_alu_op2 = divisor;
  assign div_exe_alu_add = ~prev_quot;
  assign div_exe_alu_sub = prev_quot;
  assign div_exe_alu_res = muldiv_req_alu_res[33:0];
  assign current_quot = div_exe_alu_res[33:33] ^ divisor[33:33] ? 1'b0 : 1'b1;
  assign div_exe_part_remd = {div_exe_alu_res, cycle_0th ? dividend_lsft1[32:0] : part_quot_r[32:0]};
  assign div_exe_part_remd_lsft1 = {div_exe_part_remd[66:0], current_quot};
  assign corrct_phase = sta_remd_corr | sta_quot_corr;
  assign check_phase = sta_remd_chck;
  assign div_remd = check_phase ? part_remd_r : corrct_phase ? muldiv_req_alu_res[32:0] : div_exe_part_remd[65:33];
  assign div_quot = check_phase ? part_quot_r : corrct_phase ? part_quot_r : {div_exe_part_remd[31:0], 1'd1};
  assign remd_is_0 = part_remd_r == 0;
  assign div_remd_chck_alu_res_w = muldiv_req_alu_res[33:0];
  assign remd_is_neg_divs = div_remd_chck_alu_res_w == 0;
  assign remd_is_divs = part_remd_r == divisor[32:0];
  assign div_need_corrct = is_div & (((part_remd_r[32:32] ^ dividend[65:65]) & ~remd_is_0) | remd_is_neg_divs | remd_is_divs);
  assign state_remd_chck_exit_ena = sta_remd_chck & (div_need_corrct | o_hsked | flush_pulse);
  assign remd_inc_quot_dec = part_remd_r[32:32] ^ divisor[33:33];
  assign div_remd_chck_alu_op1 = {part_remd_r[32:32], part_remd_r};
  assign div_remd_chck_alu_op2 = divisor;
  assign div_quot_corr_alu_op1 = {part_quot_r[32:32], part_quot_r};
  assign div_quot_corr_alu_op2 = 1;
  assign div_quot_corr_alu_add = ~remd_inc_quot_dec;
  assign div_quot_corr_alu_sub = remd_inc_quot_dec;
  assign div_remd_corr_alu_op1 = {part_remd_r[32:32], part_remd_r};
  assign div_remd_corr_alu_op2 = divisor;
  assign div_remd_corr_alu_add = remd_inc_quot_dec;
  assign div_remd_corr_alu_sub = ~remd_inc_quot_dec;
  assign part_prdt_hi_nxt = muldiv_req_alu_res[34:2];
  assign part_prdt_lo_nxt = {muldiv_req_alu_res[1:0], cycle_0th ? {mul_rs1_sign, muldiv_i_rs1[31:2]} : part_prdt_lo_r[32:2]};
  assign part_remd_nxt = corrct_phase ? muldiv_req_alu_res[32:0] : sta_exec & cycle_32nd ? div_remd : div_exe_part_remd_lsft1[65:33];
  assign part_quot_nxt = corrct_phase ? muldiv_req_alu_res[32:0] : sta_exec & cycle_32nd ? div_quot : div_exe_part_remd_lsft1[32:0];
  assign mul_exe_cnt_set = state_exec_enter_ena & is_mul;
  assign mul_exe_cnt_inc = sta_exec & ~exec_last & is_mul;
  assign div_exe_cnt_set = state_exec_enter_ena & is_div;
  assign div_exe_cnt_inc = sta_exec & ~exec_last & is_div;
  assign part_prdt_hi_ena = mul_exe_cnt_set | mul_exe_cnt_inc | state_exec_exit_ena;
  assign part_remd_ena = div_exe_cnt_set | div_exe_cnt_inc | state_exec_exit_ena | state_remd_corr_exit_ena;
  assign part_quot_ena = div_exe_cnt_set | div_exe_cnt_inc | state_exec_exit_ena | state_quot_corr_exit_ena;
  assign muldiv_sbf_0_ena = part_remd_ena | part_prdt_hi_ena;
  assign muldiv_sbf_0_nxt = is_mul ? part_prdt_hi_nxt : part_remd_nxt;
  assign muldiv_sbf_1_ena = part_quot_ena | part_prdt_hi_ena;
  assign muldiv_sbf_1_nxt = is_mul ? part_prdt_lo_nxt : part_quot_nxt;
  assign req_alu_sel1 = is_mul;
  assign req_alu_sel2 = is_div & (sta_0th | sta_exec);
  assign req_alu_sel3 = is_div & sta_quot_corr;
  assign req_alu_sel4 = is_div & sta_remd_corr;
  assign req_alu_sel5 = is_div & sta_remd_chck;
  assign muldiv_req_alu_op1 = (req_alu_sel1 ? mul_exe_alu_op1 : 0) | (req_alu_sel2 ? 35'($unsigned(div_exe_alu_op1)) : 0) | (req_alu_sel3 ? 35'($unsigned(div_quot_corr_alu_op1)) : 0) | (req_alu_sel4 ? 35'($unsigned(div_remd_corr_alu_op1)) : 0) | (req_alu_sel5 ? 35'($unsigned(div_remd_chck_alu_op1)) : 0);
  assign muldiv_req_alu_op2 = (req_alu_sel1 ? mul_exe_alu_op2 : 0) | (req_alu_sel2 ? 35'($unsigned(div_exe_alu_op2)) : 0) | (req_alu_sel3 ? 35'($unsigned(div_quot_corr_alu_op2)) : 0) | (req_alu_sel4 ? 35'($unsigned(div_remd_corr_alu_op2)) : 0) | (req_alu_sel5 ? 35'($unsigned(div_remd_chck_alu_op2)) : 0);
  assign muldiv_req_alu_add = (req_alu_sel1 & mul_exe_alu_add) | (req_alu_sel2 & div_exe_alu_add) | (req_alu_sel3 & div_quot_corr_alu_add) | (req_alu_sel4 & div_remd_corr_alu_add) | req_alu_sel5;
  assign muldiv_req_alu_sub = (req_alu_sel1 & mul_exe_alu_sub) | (req_alu_sel2 & div_exe_alu_sub) | (req_alu_sel3 & div_quot_corr_alu_sub) | (req_alu_sel4 & div_remd_corr_alu_sub);
  assign mul_res = i_mul ? part_prdt_lo_r[32:1] : muldiv_req_alu_res[31:0];
  assign div_res = i_div | i_divu ? div_quot[31:0] : div_remd[31:0];
  assign div_special_res = div_by_0 ? i_div | i_divu ? 32'd4294967295 : muldiv_i_rs1 : i_div | i_divu ? 32'd2147483648 : 0;
  assign back2back_res = (i_mul ? {part_prdt_lo_r[30:0], part_prdt_sft1_r} : 0) | (i_rem | i_remu ? part_remd_r[31:0] : 0) | (i_div | i_divu ? part_quot_r[31:0] : 0);
  assign wbck_condi = back2back_seq | special_cases ? 1'b1 : (sta_exec & exec_last & ~is_div) | (sta_remd_chck & ~div_need_corrct) | sta_remd_corr;
  assign muldiv_o_valid = wbck_condi & muldiv_i_valid;
  assign muldiv_i_ready = wbck_condi & muldiv_o_ready;
  assign muldiv_o_wbck_wdat = (back2back_seq & ~special_cases ? back2back_res : 0) | (special_cases ? div_special_res : 0) | (~back2back_seq & ~special_cases & is_div ? div_res : 0) | (~back2back_seq & ~special_cases & is_mul ? mul_res : 0);
  assign muldiv_o_wbck_err = 1'b0;
  assign muldiv_i_longpipe = 1'b0;

endmodule

// Booth multiply
// Divide
// Correction check
// Correction ALU operands
// Buffer next values
// Buffer enables
// ALU operand muxing
// Results
// Special results
// Back-to-back results
// Output
// E203 HBirdv2 ALU shared datapath
// Handles ALU, BJP (branch/jump), AGU (address gen), and MulDiv operation requests.
// Pure combinational except for shared-buffer registers (no reset).
// Reference: e203_exu_alu_dpath_ref.sv
module e203_exu_alu_dpath #(
  parameter int XLEN = 32,
  parameter int ALU_ADDER_WIDTH = 35
) (
  input logic clk,
  input logic rst_n,
  input logic alu_req_alu,
  input logic alu_req_alu_add,
  input logic alu_req_alu_sub,
  input logic alu_req_alu_xor,
  input logic alu_req_alu_sll,
  input logic alu_req_alu_srl,
  input logic alu_req_alu_sra,
  input logic alu_req_alu_or,
  input logic alu_req_alu_and,
  input logic alu_req_alu_slt,
  input logic alu_req_alu_sltu,
  input logic alu_req_alu_lui,
  input logic [31:0] alu_req_alu_op1,
  input logic [31:0] alu_req_alu_op2,
  output logic [31:0] alu_req_alu_res,
  input logic bjp_req_alu,
  input logic [31:0] bjp_req_alu_op1,
  input logic [31:0] bjp_req_alu_op2,
  input logic bjp_req_alu_cmp_eq,
  input logic bjp_req_alu_cmp_ne,
  input logic bjp_req_alu_cmp_lt,
  input logic bjp_req_alu_cmp_gt,
  input logic bjp_req_alu_cmp_ltu,
  input logic bjp_req_alu_cmp_gtu,
  input logic bjp_req_alu_add,
  output logic [31:0] bjp_req_alu_add_res,
  output logic bjp_req_alu_cmp_res,
  input logic agu_req_alu,
  input logic [31:0] agu_req_alu_op1,
  input logic [31:0] agu_req_alu_op2,
  input logic agu_req_alu_swap,
  input logic agu_req_alu_add,
  input logic agu_req_alu_and,
  input logic agu_req_alu_or,
  input logic agu_req_alu_xor,
  input logic agu_req_alu_max,
  input logic agu_req_alu_min,
  input logic agu_req_alu_maxu,
  input logic agu_req_alu_minu,
  output logic [31:0] agu_req_alu_res,
  input logic agu_sbf_0_ena,
  input logic [31:0] agu_sbf_0_nxt,
  output logic [31:0] agu_sbf_0_r,
  input logic agu_sbf_1_ena,
  input logic [31:0] agu_sbf_1_nxt,
  output logic [31:0] agu_sbf_1_r,
  input logic muldiv_req_alu,
  input logic [34:0] muldiv_req_alu_op1,
  input logic [34:0] muldiv_req_alu_op2,
  input logic muldiv_req_alu_add,
  input logic muldiv_req_alu_sub,
  output logic [34:0] muldiv_req_alu_res,
  input logic muldiv_sbf_0_ena,
  input logic [32:0] muldiv_sbf_0_nxt,
  output logic [32:0] muldiv_sbf_0_r,
  input logic muldiv_sbf_1_ena,
  input logic [32:0] muldiv_sbf_1_nxt,
  output logic [32:0] muldiv_sbf_1_r
);

  // ── Regular ALU requests ───────────────────────────────────────────────
  // ── Branch/Jump unit requests ──────────────────────────────────────────
  // ── AGU requests (AMO + address calc) ─────────────────────────────────
  // ── Shared-buffer load enables (AGU multi-cycle AMO) ──────────────────
  // ── MulDiv requests ───────────────────────────────────────────────────
  // 35-bit adder ports (E203_ALU_ADDER_WIDTH = 35 when SUPPORT_SHARE_MULDIV)
  // ── MulDiv shared buffers (33-bit each) ───────────────────────────────
  // ── Shared buffer registers (33-bit, no reset — shared by AGU/MulDiv) ─
  logic [32:0] sbf_0_r = 0;
  logic [32:0] sbf_1_r = 0;
  // ── SBF muxing: MulDiv has priority over AGU ─────────────────────────
  logic sbf_0_ena;
  assign sbf_0_ena = muldiv_req_alu ? muldiv_sbf_0_ena : agu_sbf_0_ena;
  logic sbf_1_ena;
  assign sbf_1_ena = muldiv_req_alu ? muldiv_sbf_1_ena : agu_sbf_1_ena;
  // AGU data is 32-bit, pad with leading zero for the 33-bit register
  logic [32:0] sbf_0_nxt;
  assign sbf_0_nxt = muldiv_req_alu ? muldiv_sbf_0_nxt : {1'b0, agu_sbf_0_nxt};
  logic [32:0] sbf_1_nxt;
  assign sbf_1_nxt = muldiv_req_alu ? muldiv_sbf_1_nxt : {1'b0, agu_sbf_1_nxt};
  always_ff @(posedge clk) begin
    if (sbf_0_ena) begin
      sbf_0_r <= sbf_0_nxt;
    end
    if (sbf_1_ena) begin
      sbf_1_r <= sbf_1_nxt;
    end
  end
  // ── OR-based operation selects (matching reference concatenated mux) ─
  // Each requestor's signals are gated by its own req_alu, then OR'd together.
  // Matches the reference's {DPATH_MUX_WIDTH{req_alu}} & {...} | ... pattern.
  // MulDiv is NOT part of this mux — it connects directly to the adder.
  logic [31:0] misc_op1;
  assign misc_op1 = (alu_req_alu ? alu_req_alu_op1 : 0) | (bjp_req_alu ? bjp_req_alu_op1 : 0) | (agu_req_alu ? agu_req_alu_op1 : 0);
  logic [31:0] misc_op2;
  assign misc_op2 = (alu_req_alu ? alu_req_alu_op2 : 0) | (bjp_req_alu ? bjp_req_alu_op2 : 0) | (agu_req_alu ? agu_req_alu_op2 : 0);
  logic op_max;
  assign op_max = agu_req_alu & agu_req_alu_max;
  logic op_min;
  assign op_min = agu_req_alu & agu_req_alu_min;
  logic op_maxu;
  assign op_maxu = agu_req_alu & agu_req_alu_maxu;
  logic op_minu;
  assign op_minu = agu_req_alu & agu_req_alu_minu;
  logic op_add;
  assign op_add = (alu_req_alu & alu_req_alu_add) | (bjp_req_alu & bjp_req_alu_add) | (agu_req_alu & agu_req_alu_add);
  logic op_sub;
  assign op_sub = alu_req_alu & alu_req_alu_sub;
  logic op_or;
  assign op_or = (alu_req_alu & alu_req_alu_or) | (agu_req_alu & agu_req_alu_or);
  logic op_xor;
  assign op_xor = (alu_req_alu & alu_req_alu_xor) | (agu_req_alu & agu_req_alu_xor);
  logic op_and;
  assign op_and = (alu_req_alu & alu_req_alu_and) | (agu_req_alu & agu_req_alu_and);
  logic op_sll;
  assign op_sll = alu_req_alu & alu_req_alu_sll;
  logic op_srl;
  assign op_srl = alu_req_alu & alu_req_alu_srl;
  logic op_sra;
  assign op_sra = alu_req_alu & alu_req_alu_sra;
  logic op_slt;
  assign op_slt = alu_req_alu & alu_req_alu_slt;
  logic op_sltu;
  assign op_sltu = alu_req_alu & alu_req_alu_sltu;
  // mvop2: LUI for ALU, SWAP for AGU (both just pass through op2)
  logic op_mvop2;
  assign op_mvop2 = (alu_req_alu & alu_req_alu_lui) | (agu_req_alu & agu_req_alu_swap);
  logic op_cmp_eq;
  assign op_cmp_eq = bjp_req_alu & bjp_req_alu_cmp_eq;
  logic op_cmp_ne;
  assign op_cmp_ne = bjp_req_alu & bjp_req_alu_cmp_ne;
  logic op_cmp_lt;
  assign op_cmp_lt = bjp_req_alu & bjp_req_alu_cmp_lt;
  logic op_cmp_gt;
  assign op_cmp_gt = bjp_req_alu & bjp_req_alu_cmp_gt;
  logic op_cmp_ltu;
  assign op_cmp_ltu = bjp_req_alu & bjp_req_alu_cmp_ltu;
  logic op_cmp_gtu;
  assign op_cmp_gtu = bjp_req_alu & bjp_req_alu_cmp_gtu;
  logic op_addsub;
  assign op_addsub = op_add | op_sub;
  // ── Shared adder (35-bit) ─────────────────────────────────────────────
  // Subtraction-like ops: cmp_lt/gt/ltu/gtu, max/min/maxu/minu, slt/sltu
  // NOTE: cmp_eq/cmp_ne use XOR, NOT the adder
  logic do_sub;
  assign do_sub = op_sub | op_slt | op_sltu | op_cmp_lt | op_cmp_gt | op_cmp_ltu | op_cmp_gtu | op_max | op_maxu | op_min | op_minu;
  logic adder_add;
  assign adder_add = muldiv_req_alu ? muldiv_req_alu_add : op_add;
  logic adder_sub;
  assign adder_sub = muldiv_req_alu ? muldiv_req_alu_sub : do_sub;
  logic adder_addsub;
  assign adder_addsub = adder_add | adder_sub;
  // Unsigned ops: zero-extend; signed ops: sign-extend
  logic op_unsigned;
  assign op_unsigned = op_sltu | op_cmp_ltu | op_cmp_gtu | op_maxu | op_minu;
  // Sign-extend misc operands from 32 to 35 bits (3 bits of sign extension)
  logic [34:0] misc_adder_op1;
  assign misc_adder_op1 = op_unsigned ? 35'($unsigned(misc_op1)) : {misc_op1[31:31], misc_op1[31:31], misc_op1[31:31], misc_op1};
  logic [34:0] misc_adder_op2;
  assign misc_adder_op2 = op_unsigned ? 35'($unsigned(misc_op2)) : {misc_op2[31:31], misc_op2[31:31], misc_op2[31:31], misc_op2};
  // MulDiv operands are already 35-bit (sign-extended by caller)
  logic [34:0] adder_op1;
  assign adder_op1 = muldiv_req_alu ? muldiv_req_alu_op1 : misc_adder_op1;
  logic [34:0] adder_op2;
  assign adder_op2 = muldiv_req_alu ? muldiv_req_alu_op2 : misc_adder_op2;
  // Gating for power (matches reference behavior)
  logic [34:0] adder_in1;
  assign adder_in1 = adder_addsub ? adder_op1 : 0;
  logic [34:0] adder_in2;
  assign adder_in2 = adder_addsub ? adder_sub ? ~adder_op2 : adder_op2 : 0;
  logic [34:0] adder_cin;
  assign adder_cin = 35'($unsigned(adder_addsub & adder_sub));
  logic [34:0] adder_res;
  assign adder_res = 35'(adder_in1 + adder_in2 + adder_cin);
  logic [31:0] adder_res32;
  assign adder_res32 = adder_res[31:0];
  // Sign and carry at bit 32 (E203_XLEN)
  logic adder_sign;
  assign adder_sign = adder_res[32:32] != 0;
  logic op1_gt_op2;
  assign op1_gt_op2 = adder_res[32:32] == 0;
  // ── XOR-based EQ/NE ───────────────────────────────────────────────────
  logic xorer_op;
  assign xorer_op = op_xor | op_cmp_eq | op_cmp_ne;
  logic [31:0] xorer_in1;
  assign xorer_in1 = xorer_op ? misc_op1 : 0;
  logic [31:0] xorer_in2;
  assign xorer_in2 = xorer_op ? misc_op2 : 0;
  logic [31:0] xor_res;
  assign xor_res = xorer_in1 ^ xorer_in2;
  logic neq;
  assign neq = xor_res != 0;
  // ── Shift amount: lower 5 bits of ALU op2 ────────────────────────────
  logic [4:0] shamt;
  assign shamt = alu_req_alu_op2[4:0];
  logic op_shift;
  assign op_shift = op_sra | op_sll | op_srl;
  // ── Shifter: single left-shifter with bit reversal for right shifts ──
  logic [31:0] shifter_in1_rev;
  assign shifter_in1_rev = op_srl | op_sra ? {alu_req_alu_op1[0], alu_req_alu_op1[1], alu_req_alu_op1[2], alu_req_alu_op1[3], alu_req_alu_op1[4], alu_req_alu_op1[5], alu_req_alu_op1[6], alu_req_alu_op1[7], alu_req_alu_op1[8], alu_req_alu_op1[9], alu_req_alu_op1[10], alu_req_alu_op1[11], alu_req_alu_op1[12], alu_req_alu_op1[13], alu_req_alu_op1[14], alu_req_alu_op1[15], alu_req_alu_op1[16], alu_req_alu_op1[17], alu_req_alu_op1[18], alu_req_alu_op1[19], alu_req_alu_op1[20], alu_req_alu_op1[21], alu_req_alu_op1[22], alu_req_alu_op1[23], alu_req_alu_op1[24], alu_req_alu_op1[25], alu_req_alu_op1[26], alu_req_alu_op1[27], alu_req_alu_op1[28], alu_req_alu_op1[29], alu_req_alu_op1[30], alu_req_alu_op1[31]} : alu_req_alu_op1;
  logic [31:0] shifter_in1;
  assign shifter_in1 = op_shift ? shifter_in1_rev : 0;
  logic [4:0] shifter_in2;
  assign shifter_in2 = op_shift ? shamt : 0;
  logic [31:0] shifter_res;
  assign shifter_res = shifter_in1 << shifter_in2;
  logic [31:0] sll_res;
  assign sll_res = shifter_res;
  logic [31:0] srl_res;
  assign srl_res = {shifter_res[0], shifter_res[1], shifter_res[2], shifter_res[3], shifter_res[4], shifter_res[5], shifter_res[6], shifter_res[7], shifter_res[8], shifter_res[9], shifter_res[10], shifter_res[11], shifter_res[12], shifter_res[13], shifter_res[14], shifter_res[15], shifter_res[16], shifter_res[17], shifter_res[18], shifter_res[19], shifter_res[20], shifter_res[21], shifter_res[22], shifter_res[23], shifter_res[24], shifter_res[25], shifter_res[26], shifter_res[27], shifter_res[28], shifter_res[29], shifter_res[30], shifter_res[31]};
  // SRA: apply sign-extension mask
  logic [31:0] zero32;
  assign zero32 = 0;
  logic [31:0] eff_mask;
  assign eff_mask = ~zero32 >> shifter_in2;
  logic [31:0] sra_res;
  assign sra_res = (srl_res & eff_mask) | ({32{alu_req_alu_op1[31:31] != 0}} & ~eff_mask);
  // ── Adder-based results ───────────────────────────────────────────────
  logic op_slttu;
  assign op_slttu = op_slt | op_sltu;
  logic slttu_cmp_lt;
  assign slttu_cmp_lt = op_slttu & adder_sign;
  logic [31:0] slttu_res;
  assign slttu_res = slttu_cmp_lt ? 1 : 0;
  logic maxmin_sel_op1;
  assign maxmin_sel_op1 = ((op_max | op_maxu) & op1_gt_op2) | ((op_min | op_minu) & ~op1_gt_op2);
  logic [31:0] maxmin_res;
  assign maxmin_res = maxmin_sel_op1 ? misc_op1 : misc_op2;
  // mvop2 result (LUI / SWAP)
  logic [31:0] mvop2_res;
  assign mvop2_res = misc_op2;
  // OR/AND results (no gating needed — lightweight)
  logic [31:0] orer_res;
  assign orer_res = misc_op1 | misc_op2;
  logic [31:0] ander_res;
  assign ander_res = misc_op1 & misc_op2;
  // ── Unified result (OR-based, matching reference bitwise-select pattern) ─
  logic [31:0] alu_dpath_res;
  assign alu_dpath_res = (op_or ? orer_res : 0) | (op_and ? ander_res : 0) | (op_xor ? xor_res : 0) | (op_addsub ? adder_res32 : 0) | (op_srl ? srl_res : 0) | (op_sll ? sll_res : 0) | (op_sra ? sra_res : 0) | (op_mvop2 ? mvop2_res : 0) | (op_slttu ? slttu_res : 0) | (op_max | op_maxu | op_min | op_minu ? maxmin_res : 0);
  // ── BJP compare result ────────────────────────────────────────────────
  logic cmp_res_ne;
  assign cmp_res_ne = op_cmp_ne & neq;
  logic cmp_res_eq;
  assign cmp_res_eq = op_cmp_eq & ~neq;
  logic cmp_res_lt;
  assign cmp_res_lt = op_cmp_lt & adder_sign;
  logic cmp_res_ltu;
  assign cmp_res_ltu = op_cmp_ltu & adder_sign;
  logic cmp_res_gt;
  assign cmp_res_gt = op_cmp_gt & op1_gt_op2;
  logic cmp_res_gtu;
  assign cmp_res_gtu = op_cmp_gtu & op1_gt_op2;
  logic cmp_res;
  assign cmp_res = cmp_res_eq | cmp_res_ne | cmp_res_lt | cmp_res_gt | cmp_res_ltu | cmp_res_gtu;
  // ── Combinational outputs ──────────────────────────────────────────────
  assign alu_req_alu_res = alu_dpath_res;
  assign agu_req_alu_res = alu_dpath_res;
  assign bjp_req_alu_add_res = alu_dpath_res;
  assign bjp_req_alu_cmp_res = cmp_res;
  assign muldiv_req_alu_res = adder_res;
  assign agu_sbf_0_r = sbf_0_r[31:0];
  assign agu_sbf_1_r = sbf_1_r[31:0];
  assign muldiv_sbf_0_r = sbf_0_r;
  assign muldiv_sbf_1_r = sbf_1_r;

endmodule

// MulDiv gets the full 35-bit adder result
// Shared buffer outputs
// E203 Long-Pipe Writeback Collector
// Collects results from LSU and NICE coprocessor, arbitrates into
// a single writeback port and exception port.
// Matches RealBench port interface.
module e203_exu_longpwbck (
  input logic clk,
  input logic rst_n,
  input logic lsu_wbck_i_valid,
  output logic lsu_wbck_i_ready,
  input logic [31:0] lsu_wbck_i_wdat,
  input logic [0:0] lsu_wbck_i_itag,
  input logic lsu_wbck_i_err,
  input logic lsu_cmt_i_buserr,
  input logic [31:0] lsu_cmt_i_badaddr,
  input logic lsu_cmt_i_ld,
  input logic lsu_cmt_i_st,
  output logic longp_wbck_o_valid,
  input logic longp_wbck_o_ready,
  output logic [31:0] longp_wbck_o_wdat,
  output logic [4:0] longp_wbck_o_flags,
  output logic [4:0] longp_wbck_o_rdidx,
  output logic longp_wbck_o_rdfpu,
  output logic longp_excp_o_valid,
  input logic longp_excp_o_ready,
  output logic longp_excp_o_insterr,
  output logic longp_excp_o_ld,
  output logic longp_excp_o_st,
  output logic longp_excp_o_buserr,
  output logic [31:0] longp_excp_o_badaddr,
  output logic [31:0] longp_excp_o_pc,
  input logic oitf_empty,
  input logic [0:0] oitf_ret_ptr,
  input logic [4:0] oitf_ret_rdidx,
  input logic [31:0] oitf_ret_pc,
  input logic oitf_ret_rdwen,
  input logic oitf_ret_rdfpu,
  output logic oitf_ret_ena,
  input logic nice_longp_wbck_i_valid,
  output logic nice_longp_wbck_i_ready,
  input logic [31:0] nice_longp_wbck_i_wdat,
  input logic [0:0] nice_longp_wbck_i_itag,
  input logic nice_longp_wbck_i_err
);

  // ── LSU writeback input ───────────────────────────────────────────
  // ── LSU commit info ───────────────────────────────────────────────
  // ── Merged writeback output ───────────────────────────────────────
  // ── Exception output ──────────────────────────────────────────────
  // ── OITF interface ────────────────────────────────────────────────
  // ── NICE writeback input ──────────────────────────────────────────
  // OITF tag matching: only writeback when itag matches OITF top entry
  logic wbck_ready4lsu;
  assign wbck_ready4lsu = (lsu_wbck_i_itag == oitf_ret_ptr) & ~oitf_empty;
  logic wbck_sel_lsu;
  assign wbck_sel_lsu = lsu_wbck_i_valid & wbck_ready4lsu;
  logic wbck_ready4nice;
  assign wbck_ready4nice = (nice_longp_wbck_i_itag == oitf_ret_ptr) & ~oitf_empty;
  logic wbck_sel_nice;
  assign wbck_sel_nice = nice_longp_wbck_i_valid & wbck_ready4nice;
  // Merged valid/data/err
  logic wbck_i_valid;
  assign wbck_i_valid = wbck_sel_lsu | wbck_sel_nice;
  logic [31:0] wbck_i_wdat;
  assign wbck_i_wdat = wbck_sel_lsu ? lsu_wbck_i_wdat : wbck_sel_nice ? nice_longp_wbck_i_wdat : 0;
  logic wbck_i_err;
  assign wbck_i_err = wbck_sel_lsu & lsu_wbck_i_err;
  logic wbck_i_rdwen;
  assign wbck_i_rdwen = oitf_ret_rdwen;
  // Need writeback (rdwen=1 and no error) or need exception (error)
  logic need_wbck;
  assign need_wbck = wbck_i_rdwen & ~wbck_i_err;
  logic need_excp;
  assign need_excp = wbck_i_err;
  // Combined ready: need both writeback and exception paths ready
  logic wbck_i_ready;
  assign wbck_i_ready = (need_wbck ? longp_wbck_o_ready : 1'b1) & (need_excp ? longp_excp_o_ready : 1'b1);
  assign lsu_wbck_i_ready = wbck_ready4lsu & wbck_i_ready;
  assign nice_longp_wbck_i_ready = wbck_ready4nice & wbck_i_ready;
  assign oitf_ret_ena = wbck_i_valid & wbck_i_ready;
  assign longp_wbck_o_valid = need_wbck & wbck_i_valid & (need_excp ? longp_excp_o_ready : 1'b1);
  assign longp_wbck_o_wdat = wbck_i_wdat;
  assign longp_wbck_o_rdidx = oitf_ret_rdidx;
  assign longp_wbck_o_rdfpu = oitf_ret_rdfpu;
  assign longp_wbck_o_flags = 0;
  assign longp_excp_o_valid = need_excp & wbck_i_valid & (need_wbck ? longp_wbck_o_ready : 1'b1);
  assign longp_excp_o_insterr = 1'b0;
  assign longp_excp_o_ld = wbck_sel_lsu & lsu_cmt_i_ld;
  assign longp_excp_o_st = wbck_sel_lsu & lsu_cmt_i_st;
  assign longp_excp_o_buserr = wbck_sel_lsu & lsu_cmt_i_buserr;
  assign longp_excp_o_badaddr = wbck_sel_lsu ? lsu_cmt_i_badaddr : 0;
  assign longp_excp_o_pc = oitf_ret_pc;

endmodule

// Source ready: tag match AND combined ready
// Writeback output: valid when need wbck AND combined ready
// Exception output: valid when need excp AND combined ready
// Exception info gated by wbck_sel_lsu (only LSU generates exceptions)
// E203 HBirdv2 write-back arbiter
// Arbitrates between ALU (lower priority) and long-pipeline (higher priority)
// write-back requests, forwarding the winner to the integer register file.
// Purely combinational — no registers, no reset used.
module e203_exu_wbck #(
  parameter int XLEN = 32,
  parameter int RFIDX_WIDTH = 5
) (
  input logic clk,
  input logic rst_n,
  input logic alu_wbck_i_valid,
  output logic alu_wbck_i_ready,
  input logic [31:0] alu_wbck_i_wdat,
  input logic [4:0] alu_wbck_i_rdidx,
  input logic longp_wbck_i_valid,
  output logic longp_wbck_i_ready,
  input logic [31:0] longp_wbck_i_wdat,
  input logic [4:0] longp_wbck_i_flags,
  input logic [4:0] longp_wbck_i_rdidx,
  input logic longp_wbck_i_rdfpu,
  output logic rf_wbck_o_ena,
  output logic [31:0] rf_wbck_o_wdat,
  output logic [4:0] rf_wbck_o_rdidx
);

  // present for interface compatibility; unused
  // ALU write-back (lower priority)
  // Long-pipeline write-back (higher priority)
  // Register file write port
  // ALU ready when no longp writeback (matches reference wbck_ready4alu)
  logic wbck_ready4alu;
  assign wbck_ready4alu = ~longp_wbck_i_valid;
  logic wbck_sel_alu;
  assign wbck_sel_alu = alu_wbck_i_valid & wbck_ready4alu;
  logic wbck_sel_longp;
  assign wbck_sel_longp = longp_wbck_i_valid;
  assign longp_wbck_i_ready = 1;
  assign alu_wbck_i_ready = wbck_ready4alu;
  assign rf_wbck_o_wdat = wbck_sel_alu ? alu_wbck_i_wdat : longp_wbck_i_wdat;
  assign rf_wbck_o_rdidx = wbck_sel_alu ? alu_wbck_i_rdidx : longp_wbck_i_rdidx;
  assign rf_wbck_o_ena = (wbck_sel_longp & ~longp_wbck_i_rdfpu) | wbck_sel_alu;

endmodule

// RF is always ready (single write port)
// Muxed writeback data using wbck_sel_alu (matches reference exactly)
// ena: valid & ~rdfpu (rdfpu only true for longp FPU, always false for ALU)
// E203 HBirdv2 Execution Commit Unit
// Wrapper that instantiates e203_exu_excp (exception/interrupt) and
// e203_exu_branchslv (branch/jump resolve). Matches RealBench port interface.
module e203_exu_commit #(
  parameter int XLEN = 32
) (
  input logic clk,
  input logic rst_n,
  output logic commit_mret,
  output logic commit_trap,
  output logic core_wfi,
  output logic nonflush_cmt_ena,
  output logic excp_active,
  input logic amo_wait,
  output logic wfi_halt_ifu_req,
  output logic wfi_halt_exu_req,
  input logic wfi_halt_ifu_ack,
  input logic wfi_halt_exu_ack,
  input logic dbg_irq_r,
  input logic lcl_irq_r,
  input logic ext_irq_r,
  input logic sft_irq_r,
  input logic tmr_irq_r,
  input logic evt_r,
  input logic status_mie_r,
  input logic mtie_r,
  input logic msie_r,
  input logic meie_r,
  input logic alu_cmt_i_valid,
  output logic alu_cmt_i_ready,
  input logic [31:0] alu_cmt_i_pc,
  input logic [31:0] alu_cmt_i_instr,
  input logic alu_cmt_i_pc_vld,
  input logic [31:0] alu_cmt_i_imm,
  input logic alu_cmt_i_rv32,
  input logic alu_cmt_i_bjp,
  input logic alu_cmt_i_wfi,
  input logic alu_cmt_i_fencei,
  input logic alu_cmt_i_mret,
  input logic alu_cmt_i_dret,
  input logic alu_cmt_i_ecall,
  input logic alu_cmt_i_ebreak,
  input logic alu_cmt_i_ifu_misalgn,
  input logic alu_cmt_i_ifu_buserr,
  input logic alu_cmt_i_ifu_ilegl,
  input logic alu_cmt_i_bjp_prdt,
  input logic alu_cmt_i_bjp_rslv,
  input logic alu_cmt_i_misalgn,
  input logic alu_cmt_i_ld,
  input logic alu_cmt_i_stamo,
  input logic alu_cmt_i_buserr,
  input logic [31:0] alu_cmt_i_badaddr,
  output logic [31:0] cmt_badaddr,
  output logic cmt_badaddr_ena,
  output logic [31:0] cmt_epc,
  output logic cmt_epc_ena,
  output logic [31:0] cmt_cause,
  output logic cmt_cause_ena,
  output logic cmt_instret_ena,
  output logic cmt_status_ena,
  output logic [31:0] cmt_dpc,
  output logic cmt_dpc_ena,
  output logic [2:0] cmt_dcause,
  output logic cmt_dcause_ena,
  output logic cmt_mret_ena,
  input logic [31:0] csr_epc_r,
  input logic [31:0] csr_dpc_r,
  input logic [31:0] csr_mtvec_r,
  input logic dbg_mode,
  input logic dbg_halt_r,
  input logic dbg_step_r,
  input logic dbg_ebreakm_r,
  input logic oitf_empty,
  input logic u_mode,
  input logic s_mode,
  input logic h_mode,
  input logic m_mode,
  output logic longp_excp_i_ready,
  input logic longp_excp_i_valid,
  input logic longp_excp_i_ld,
  input logic longp_excp_i_st,
  input logic longp_excp_i_buserr,
  input logic [31:0] longp_excp_i_badaddr,
  input logic longp_excp_i_insterr,
  input logic [31:0] longp_excp_i_pc,
  output logic flush_pulse,
  output logic flush_req,
  input logic pipe_flush_ack,
  output logic pipe_flush_req,
  output logic [31:0] pipe_flush_add_op1,
  output logic [31:0] pipe_flush_add_op2,
  output logic [31:0] pipe_flush_pc
);

  // ── Commit status outputs ─────────────────────────────────────────
  // ── AMO wait ──────────────────────────────────────────────────────
  // ── WFI halt interface ────────────────────────────────────────────
  // ── Interrupt inputs ──────────────────────────────────────────────
  // ── ALU commit input channel ──────────────────────────────────────
  // ── CSR commit outputs ────────────────────────────────────────────
  // ── CSR read inputs ───────────────────────────────────────────────
  // ── Debug mode inputs ─────────────────────────────────────────────
  // ── Privilege mode inputs ─────────────────────────────────────────
  // ── Long-pipe exception input ─────────────────────────────────────
  // ── Flush outputs ─────────────────────────────────────────────────
  // ── Sub-module interconnect wires ─────────────────────────────────
  logic alu_brchmis_cmt_i_ready;
  logic alu_brchmis_flush_req;
  logic [31:0] alu_brchmis_flush_add_op1;
  logic [31:0] alu_brchmis_flush_add_op2;
  logic [31:0] alu_brchmis_flush_pc;
  logic cmt_dret_ena;
  logic alu_excp_cmt_i_ready;
  logic excpirq_flush_req;
  logic [31:0] excpirq_flush_add_op1;
  logic [31:0] excpirq_flush_add_op2;
  logic [31:0] excpirq_flush_pc;
  logic nonalu_excpirq_flush_req_raw;
  // cmt_ena: valid handshaked
  logic cmt_ena;
  assign cmt_ena = alu_cmt_i_valid & alu_cmt_i_ready;
  // ── Sub-module instances ──────────────────────────────────────────
  e203_exu_branchslv branchslv (
    .clk(clk),
    .rst_n(rst_n),
    .cmt_i_ready(alu_brchmis_cmt_i_ready),
    .cmt_i_valid(alu_cmt_i_valid),
    .cmt_i_rv32(alu_cmt_i_rv32),
    .cmt_i_bjp(alu_cmt_i_bjp),
    .cmt_i_fencei(alu_cmt_i_fencei),
    .cmt_i_mret(alu_cmt_i_mret),
    .cmt_i_dret(alu_cmt_i_dret),
    .cmt_i_bjp_prdt(alu_cmt_i_bjp_prdt),
    .cmt_i_bjp_rslv(alu_cmt_i_bjp_rslv),
    .cmt_i_pc(alu_cmt_i_pc),
    .cmt_i_imm(alu_cmt_i_imm),
    .cmt_mret_ena(cmt_mret_ena),
    .cmt_dret_ena(cmt_dret_ena),
    .csr_epc_r(csr_epc_r),
    .csr_dpc_r(csr_dpc_r),
    .nonalu_excpirq_flush_req_raw(nonalu_excpirq_flush_req_raw),
    .brchmis_flush_ack(pipe_flush_ack),
    .brchmis_flush_req(alu_brchmis_flush_req),
    .brchmis_flush_add_op1(alu_brchmis_flush_add_op1),
    .brchmis_flush_add_op2(alu_brchmis_flush_add_op2),
    .brchmis_flush_pc(alu_brchmis_flush_pc)
  );
  e203_exu_excp excp (
    .clk(clk),
    .rst_n(rst_n),
    .commit_trap(commit_trap),
    .core_wfi(core_wfi),
    .wfi_halt_ifu_req(wfi_halt_ifu_req),
    .wfi_halt_exu_req(wfi_halt_exu_req),
    .wfi_halt_ifu_ack(wfi_halt_ifu_ack),
    .wfi_halt_exu_ack(wfi_halt_exu_ack),
    .cmt_badaddr(cmt_badaddr),
    .cmt_badaddr_ena(cmt_badaddr_ena),
    .cmt_epc(cmt_epc),
    .cmt_epc_ena(cmt_epc_ena),
    .cmt_cause(cmt_cause),
    .cmt_cause_ena(cmt_cause_ena),
    .cmt_status_ena(cmt_status_ena),
    .cmt_dpc(cmt_dpc),
    .cmt_dpc_ena(cmt_dpc_ena),
    .cmt_dcause(cmt_dcause),
    .cmt_dcause_ena(cmt_dcause_ena),
    .cmt_dret_ena(cmt_dret_ena),
    .cmt_ena(cmt_ena),
    .alu_excp_i_valid(alu_cmt_i_valid),
    .alu_excp_i_ready(alu_excp_cmt_i_ready),
    .alu_excp_i_misalgn(alu_cmt_i_misalgn),
    .alu_excp_i_ld(alu_cmt_i_ld),
    .alu_excp_i_stamo(alu_cmt_i_stamo),
    .alu_excp_i_buserr(alu_cmt_i_buserr),
    .alu_excp_i_pc(alu_cmt_i_pc),
    .alu_excp_i_instr(alu_cmt_i_instr),
    .alu_excp_i_pc_vld(alu_cmt_i_pc_vld),
    .alu_excp_i_badaddr(alu_cmt_i_badaddr),
    .alu_excp_i_ecall(alu_cmt_i_ecall),
    .alu_excp_i_ebreak(alu_cmt_i_ebreak),
    .alu_excp_i_wfi(alu_cmt_i_wfi),
    .alu_excp_i_ifu_misalgn(alu_cmt_i_ifu_misalgn),
    .alu_excp_i_ifu_buserr(alu_cmt_i_ifu_buserr),
    .alu_excp_i_ifu_ilegl(alu_cmt_i_ifu_ilegl),
    .longp_excp_i_ready(longp_excp_i_ready),
    .longp_excp_i_valid(longp_excp_i_valid),
    .longp_excp_i_ld(longp_excp_i_ld),
    .longp_excp_i_st(longp_excp_i_st),
    .longp_excp_i_buserr(longp_excp_i_buserr),
    .longp_excp_i_badaddr(longp_excp_i_badaddr),
    .longp_excp_i_insterr(longp_excp_i_insterr),
    .longp_excp_i_pc(longp_excp_i_pc),
    .csr_mtvec_r(csr_mtvec_r),
    .dbg_irq_r(dbg_irq_r),
    .lcl_irq_r(lcl_irq_r),
    .ext_irq_r(ext_irq_r),
    .sft_irq_r(sft_irq_r),
    .tmr_irq_r(tmr_irq_r),
    .status_mie_r(status_mie_r),
    .mtie_r(mtie_r),
    .msie_r(msie_r),
    .meie_r(meie_r),
    .dbg_mode(dbg_mode),
    .dbg_halt_r(dbg_halt_r),
    .dbg_step_r(dbg_step_r),
    .dbg_ebreakm_r(dbg_ebreakm_r),
    .oitf_empty(oitf_empty),
    .u_mode(u_mode),
    .s_mode(s_mode),
    .h_mode(h_mode),
    .m_mode(m_mode),
    .excpirq_flush_ack(pipe_flush_ack),
    .excpirq_flush_req(excpirq_flush_req),
    .nonalu_excpirq_flush_req_raw(nonalu_excpirq_flush_req_raw),
    .excpirq_flush_add_op1(excpirq_flush_add_op1),
    .excpirq_flush_add_op2(excpirq_flush_add_op2),
    .excpirq_flush_pc(excpirq_flush_pc),
    .excp_active(excp_active),
    .amo_wait(amo_wait)
  );
  // ── Wrapper-level combinational glue ─────────────────────────────
  assign alu_cmt_i_ready = alu_excp_cmt_i_ready & alu_brchmis_cmt_i_ready;
  assign pipe_flush_req = excpirq_flush_req | alu_brchmis_flush_req;
  assign pipe_flush_add_op1 = excpirq_flush_req ? excpirq_flush_add_op1 : alu_brchmis_flush_add_op1;
  assign pipe_flush_add_op2 = excpirq_flush_req ? excpirq_flush_add_op2 : alu_brchmis_flush_add_op2;
  assign pipe_flush_pc = excpirq_flush_req ? excpirq_flush_pc : alu_brchmis_flush_pc;
  assign commit_mret = cmt_mret_ena;
  assign cmt_instret_ena = cmt_ena & ~alu_brchmis_flush_req;
  assign nonflush_cmt_ena = cmt_ena & ~pipe_flush_req;
  assign flush_pulse = pipe_flush_ack & pipe_flush_req;
  assign flush_req = nonalu_excpirq_flush_req_raw;

endmodule

// ALU ready: both sub-modules must be ready
// Pipe flush: either exception or branch-mispredict flush
// Flush target: exception has priority over branch mispredict
// Commit status
// instret: commit without branch-mispredict flush
// nonflush: commit without pipe flush
// Flush pulse: ack AND req
// Flush req to ALU (non-ALU sources like MUL-div)
// E203 Branch Resolve Unit
// Pure combinational: detects mispredictions, generates flush requests,
// computes flush target PC for branches, jumps, mret, dret, fencei.
module e203_exu_branchslv (
  input logic clk,
  input logic rst_n,
  input logic cmt_i_valid,
  output logic cmt_i_ready,
  input logic cmt_i_rv32,
  input logic cmt_i_dret,
  input logic cmt_i_mret,
  input logic cmt_i_fencei,
  input logic cmt_i_bjp,
  input logic cmt_i_bjp_prdt,
  input logic cmt_i_bjp_rslv,
  input logic [31:0] cmt_i_pc,
  input logic [31:0] cmt_i_imm,
  input logic [31:0] csr_epc_r,
  input logic [31:0] csr_dpc_r,
  input logic nonalu_excpirq_flush_req_raw,
  input logic brchmis_flush_ack,
  output logic brchmis_flush_req,
  output logic [31:0] brchmis_flush_add_op1,
  output logic [31:0] brchmis_flush_add_op2,
  output logic [31:0] brchmis_flush_pc,
  output logic cmt_mret_ena,
  output logic cmt_dret_ena,
  output logic cmt_fencei_ena
);

  // Commit interface
  // CSR values
  // Flush interface
  // Commit enables
  logic is_branch;
  assign is_branch = cmt_i_bjp | cmt_i_fencei | cmt_i_mret | cmt_i_dret;
  logic need_flush;
  assign need_flush = (cmt_i_bjp & (cmt_i_bjp_prdt ^ cmt_i_bjp_rslv)) | cmt_i_fencei | cmt_i_mret | cmt_i_dret;
  logic flush_req_pre;
  assign flush_req_pre = cmt_i_valid & need_flush;
  logic flush_ack_pre;
  assign flush_ack_pre = brchmis_flush_ack & ~nonalu_excpirq_flush_req_raw;
  logic [31:0] pc_incr;
  assign pc_incr = cmt_i_rv32 ? 4 : 2;
  assign brchmis_flush_req = flush_req_pre & ~nonalu_excpirq_flush_req_raw;
  assign brchmis_flush_add_op1 = cmt_i_dret ? csr_dpc_r : cmt_i_mret ? csr_epc_r : cmt_i_pc;
  assign brchmis_flush_add_op2 = cmt_i_dret ? 0 : cmt_i_mret ? 0 : cmt_i_fencei | cmt_i_bjp_prdt ? pc_incr : cmt_i_imm;
  assign brchmis_flush_pc = cmt_i_fencei | (cmt_i_bjp & cmt_i_bjp_prdt) ? 32'(cmt_i_pc + pc_incr) : cmt_i_bjp & ~cmt_i_bjp_prdt ? 32'(cmt_i_pc + cmt_i_imm) : cmt_i_dret ? csr_dpc_r : csr_epc_r;
  assign cmt_mret_ena = cmt_i_mret & brchmis_flush_req & brchmis_flush_ack;
  assign cmt_dret_ena = cmt_i_dret & brchmis_flush_req & brchmis_flush_ack;
  assign cmt_fencei_ena = cmt_i_fencei & brchmis_flush_req & brchmis_flush_ack;
  assign cmt_i_ready = ~is_branch | ((need_flush ? flush_ack_pre : 1'b1) & ~nonalu_excpirq_flush_req_raw);

endmodule

// Flush target operands (for external adder)
// Pre-computed flush PC (timing boost path, matching reference priority)
// Commit enables: fire on flush handshake (req & ack)
// Ready: non-branch always ready; branch waits for flush ack
// E203 Exception Handler
// Manages exceptions, interrupts, debug-mode entry, and WFI.
// Priority: longpipe_excp > debug_entry > IRQ > ALU_excp.
module e203_exu_excp (
  input logic clk,
  input logic rst_n,
  output logic commit_trap,
  output logic core_wfi,
  output logic wfi_halt_ifu_req,
  output logic wfi_halt_exu_req,
  input logic wfi_halt_ifu_ack,
  input logic wfi_halt_exu_ack,
  input logic amo_wait,
  output logic alu_excp_i_ready,
  input logic alu_excp_i_valid,
  input logic alu_excp_i_ld,
  input logic alu_excp_i_stamo,
  input logic alu_excp_i_misalgn,
  input logic alu_excp_i_buserr,
  input logic alu_excp_i_ecall,
  input logic alu_excp_i_ebreak,
  input logic alu_excp_i_wfi,
  input logic alu_excp_i_ifu_misalgn,
  input logic alu_excp_i_ifu_buserr,
  input logic alu_excp_i_ifu_ilegl,
  input logic [31:0] alu_excp_i_badaddr,
  input logic [31:0] alu_excp_i_pc,
  input logic [31:0] alu_excp_i_instr,
  input logic alu_excp_i_pc_vld,
  output logic longp_excp_i_ready,
  input logic longp_excp_i_valid,
  input logic longp_excp_i_ld,
  input logic longp_excp_i_st,
  input logic longp_excp_i_buserr,
  input logic longp_excp_i_insterr,
  input logic [31:0] longp_excp_i_badaddr,
  input logic [31:0] longp_excp_i_pc,
  input logic excpirq_flush_ack,
  output logic excpirq_flush_req,
  output logic nonalu_excpirq_flush_req_raw,
  output logic [31:0] excpirq_flush_add_op1,
  output logic [31:0] excpirq_flush_add_op2,
  output logic [31:0] excpirq_flush_pc,
  input logic [31:0] csr_mtvec_r,
  input logic cmt_dret_ena,
  input logic cmt_ena,
  output logic [31:0] cmt_badaddr,
  output logic [31:0] cmt_epc,
  output logic [31:0] cmt_cause,
  output logic cmt_badaddr_ena,
  output logic cmt_epc_ena,
  output logic cmt_cause_ena,
  output logic cmt_status_ena,
  output logic [31:0] cmt_dpc,
  output logic cmt_dpc_ena,
  output logic [2:0] cmt_dcause,
  output logic cmt_dcause_ena,
  input logic dbg_irq_r,
  input logic lcl_irq_r,
  input logic ext_irq_r,
  input logic sft_irq_r,
  input logic tmr_irq_r,
  input logic status_mie_r,
  input logic mtie_r,
  input logic msie_r,
  input logic meie_r,
  input logic dbg_mode,
  input logic dbg_halt_r,
  input logic dbg_step_r,
  input logic dbg_ebreakm_r,
  input logic oitf_empty,
  input logic u_mode,
  input logic s_mode,
  input logic h_mode,
  input logic m_mode,
  output logic excp_active
);

  // ALU exception inputs
  // Long-pipe exception inputs
  // Flush interface
  // CSR inputs
  // CSR outputs
  // IRQ inputs
  // CSR status
  // Debug
  // Privilege mode
  // Internal state
  logic wfi_flag_r;
  logic wfi_halt_req_r;
  logic step_req_r;
  // Wires for complex combinational logic
  logic irq_req;
  logic wfi_irq_req;
  logic irq_req_active_w;
  logic longp_need_flush;
  logic alu_need_flush;
  logic dbg_entry_req;
  logic nonalu_dbg_entry_req;
  logic nonalu_dbg_entry_req_raw_w;
  logic dbg_step_req;
  logic dbg_trig_req;
  logic dbg_ebrk_req;
  logic dbg_irq_req_w;
  logic dbg_halt_req;
  logic alu_ebreakm_flush_req;
  logic alu_excp_i_ebreak4excp;
  logic alu_excp_i_ebreak4dbg;
  logic longp_excp_flush_req;
  logic dbg_entry_flush_req;
  logic irq_flush_req;
  logic alu_excp_i_ready4dbg;
  logic alu_excp_i_ready4nondbg;
  logic alu_excp_flush_req;
  logic all_excp_flush_req;
  logic excpirq_taken_ena;
  logic excp_taken_ena;
  logic irq_taken_ena;
  logic dbg_entry_taken_ena;
  logic wfi_flag_set;
  logic wfi_flag_clr;
  logic wfi_halt_req_set;
  logic [31:0] irq_cause_w;
  logic [31:0] excp_cause_w;
  logic excp_flush_by_alu_agu;
  logic excp_flush_by_longp_ldst;
  // WFI flag: set on 4-way handshake, clear on irq/dbg
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      wfi_flag_r <= 1'b0;
    end else begin
      if (wfi_flag_set | wfi_flag_clr) begin
        wfi_flag_r <= wfi_flag_set & ~wfi_flag_clr;
      end
    end
  end
  // WFI halt request: set on WFI commit, clear same as wfi_flag
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      wfi_halt_req_r <= 1'b0;
    end else begin
      if (wfi_halt_req_set | wfi_flag_clr) begin
        wfi_halt_req_r <= wfi_halt_req_set & ~wfi_flag_clr;
      end
    end
  end
  // Step request
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      step_req_r <= 1'b0;
    end else begin
      if (dbg_entry_taken_ena) begin
        step_req_r <= 1'b0;
      end else if (~dbg_mode & dbg_step_r & cmt_ena & ~dbg_entry_taken_ena) begin
        step_req_r <= 1'b1;
      end
    end
  end
  always_comb begin
    // Long-pipe exception always causes flush
    longp_need_flush = longp_excp_i_valid;
    // ebreak handling: ebreak4excp does NOT depend on alu_need_flush
    alu_excp_i_ebreak4excp = alu_excp_i_ebreak & (~dbg_ebreakm_r | dbg_mode);
    // ALU exceptions (computed before ebreak4dbg to break circular dep)
    alu_need_flush = alu_excp_i_misalgn | alu_excp_i_buserr | alu_excp_i_ebreak4excp | alu_excp_i_ecall | alu_excp_i_ifu_misalgn | alu_excp_i_ifu_buserr | alu_excp_i_ifu_ilegl;
    // ebreak4dbg depends on alu_need_flush (overridden by other ALU exceptions)
    alu_excp_i_ebreak4dbg = alu_excp_i_ebreak & ~alu_need_flush & dbg_ebreakm_r & ~dbg_mode;
    alu_ebreakm_flush_req = alu_excp_i_valid & alu_excp_i_ebreak4dbg;
    // Debug entry priority
    dbg_step_req = step_req_r;
    dbg_trig_req = 1'b0;
    // No trigger support
    dbg_ebrk_req = alu_ebreakm_flush_req & ~step_req_r;
    dbg_irq_req_w = dbg_irq_r & ~alu_ebreakm_flush_req & ~step_req_r;
    dbg_halt_req = dbg_halt_r & ~dbg_irq_r & ~alu_ebreakm_flush_req & ~step_req_r & ~dbg_step_r;
    dbg_entry_req = ~dbg_mode & ((dbg_irq_req_w & ~amo_wait) | (dbg_halt_req & ~amo_wait) | dbg_step_req | dbg_ebrk_req);
    nonalu_dbg_entry_req = ~dbg_mode & ((dbg_irq_r & ~step_req_r & ~amo_wait) | (dbg_halt_r & ~dbg_irq_r & ~step_req_r & ~dbg_step_r & ~amo_wait) | step_req_r);
    nonalu_dbg_entry_req_raw_w = ~dbg_mode & (dbg_irq_r | dbg_halt_r | step_req_r);
    // IRQ handling
    irq_req = ~(dbg_mode | dbg_step_r | ~status_mie_r | amo_wait) & ((ext_irq_r & meie_r) | (sft_irq_r & msie_r) | (tmr_irq_r & mtie_r));
    wfi_irq_req = ~(dbg_mode | dbg_step_r) & ((ext_irq_r & meie_r) | (sft_irq_r & msie_r) | (tmr_irq_r & mtie_r));
    irq_req_active_w = wfi_flag_r ? wfi_irq_req : irq_req;
    excp_active = irq_req_active_w | nonalu_dbg_entry_req_raw_w;
    // Flush request priority
    longp_excp_flush_req = longp_need_flush;
    dbg_entry_flush_req = dbg_entry_req & oitf_empty & alu_excp_i_pc_vld & ~longp_need_flush;
    irq_flush_req = irq_req & oitf_empty & alu_excp_i_pc_vld & ~dbg_entry_req & ~longp_need_flush;
    alu_excp_flush_req = alu_excp_i_valid & alu_need_flush & oitf_empty & ~irq_req & ~dbg_entry_req & ~longp_need_flush;
    all_excp_flush_req = longp_excp_flush_req | alu_excp_flush_req;
    excpirq_flush_req = longp_excp_flush_req | dbg_entry_flush_req | irq_flush_req | alu_excp_flush_req;
    nonalu_excpirq_flush_req_raw = longp_need_flush | nonalu_dbg_entry_req_raw_w | irq_req;
    // Taken enables
    excpirq_taken_ena = excpirq_flush_req & excpirq_flush_ack;
    excp_taken_ena = all_excp_flush_req & excpirq_taken_ena;
    irq_taken_ena = irq_flush_req & excpirq_taken_ena;
    dbg_entry_taken_ena = dbg_entry_flush_req & excpirq_taken_ena;
    commit_trap = excpirq_taken_ena;
    // WFI control
    // wfi_halt_req set on WFI commit (not in debug mode)
    wfi_halt_req_set = alu_excp_i_wfi & cmt_ena & ~dbg_mode;
    // wfi_flag_clr computed first (depends only on irq/dbg, no circularity)
    wfi_flag_clr = wfi_irq_req | dbg_entry_req;
    // wfi_halt outputs (depend on registered values and wfi_flag_clr)
    wfi_halt_ifu_req = wfi_halt_req_r & ~wfi_flag_clr;
    wfi_halt_exu_req = wfi_halt_req_r;
    // wfi_flag set on full 4-way handshake
    wfi_flag_set = wfi_halt_ifu_req & wfi_halt_ifu_ack & wfi_halt_exu_req & wfi_halt_exu_ack;
    core_wfi = wfi_flag_r & ~wfi_flag_clr;
    // Ready signals
    longp_excp_i_ready = excpirq_flush_ack;
    // alu_excp_i_ready: matches reference alu_ebreakm_flush_req_novld (no valid qualifier)
    alu_excp_i_ready4dbg = excpirq_flush_ack & oitf_empty & alu_excp_i_pc_vld & ~longp_need_flush;
    alu_excp_i_ready4nondbg = alu_need_flush ? excpirq_flush_ack & oitf_empty & ~irq_req & ~nonalu_dbg_entry_req & ~longp_need_flush : ~irq_req & ~nonalu_dbg_entry_req & ~longp_need_flush;
    // alu_ebreakm_flush_req_novld = alu_excp_i_ebreak4dbg (no alu_excp_i_valid qualifier)
    alu_excp_i_ready = alu_excp_i_ebreak4dbg | 1'b0 ? alu_excp_i_ready4dbg : alu_excp_i_ready4nondbg;
    // Flush target
    if (dbg_entry_flush_req) begin
      excpirq_flush_add_op1 = 32'd2048;
      excpirq_flush_add_op2 = 0;
      excpirq_flush_pc = 32'd2048;
    end else if (all_excp_flush_req & dbg_mode) begin
      excpirq_flush_add_op1 = 32'd2056;
      excpirq_flush_add_op2 = 0;
      excpirq_flush_pc = 32'd2056;
    end else begin
      excpirq_flush_add_op1 = csr_mtvec_r;
      excpirq_flush_add_op2 = 0;
      excpirq_flush_pc = csr_mtvec_r;
    end
    // IRQ cause
    irq_cause_w = {1'd1, 27'd0, sft_irq_r & msie_r ? 4'd3 : tmr_irq_r & mtie_r ? 4'd7 : ext_irq_r & meie_r ? 4'd11 : 4'd0};
    // Exception cause helpers
    excp_flush_by_alu_agu = (alu_excp_flush_req & alu_excp_i_ld & alu_excp_i_misalgn) | (alu_excp_flush_req & alu_excp_i_ld & alu_excp_i_buserr) | (alu_excp_flush_req & alu_excp_i_stamo & alu_excp_i_misalgn) | (alu_excp_flush_req & alu_excp_i_stamo & alu_excp_i_buserr);
    excp_flush_by_longp_ldst = (longp_excp_flush_req & longp_excp_i_ld & longp_excp_i_buserr) | (longp_excp_flush_req & longp_excp_i_st & longp_excp_i_buserr);
    // Exception cause encoding
    if (alu_excp_flush_req & alu_excp_i_ifu_misalgn) begin
      excp_cause_w = 0;
    end else if (alu_excp_flush_req & alu_excp_i_ifu_buserr) begin
      excp_cause_w = 1;
    end else if (alu_excp_flush_req & alu_excp_i_ifu_ilegl) begin
      excp_cause_w = 2;
    end else if (alu_excp_flush_req & alu_excp_i_ebreak4excp) begin
      excp_cause_w = 3;
    end else if (alu_excp_flush_req & alu_excp_i_ld & alu_excp_i_misalgn) begin
      excp_cause_w = 4;
    end else if ((longp_excp_flush_req & longp_excp_i_ld & longp_excp_i_buserr) | (alu_excp_flush_req & alu_excp_i_ld & alu_excp_i_buserr)) begin
      excp_cause_w = 5;
    end else if (alu_excp_flush_req & alu_excp_i_stamo & alu_excp_i_misalgn) begin
      excp_cause_w = 6;
    end else if ((longp_excp_flush_req & longp_excp_i_st & longp_excp_i_buserr) | (alu_excp_flush_req & alu_excp_i_stamo & alu_excp_i_buserr)) begin
      excp_cause_w = 7;
    end else if (alu_excp_flush_req & alu_excp_i_ecall & u_mode) begin
      excp_cause_w = 8;
    end else if (alu_excp_flush_req & alu_excp_i_ecall & s_mode) begin
      excp_cause_w = 9;
    end else if (alu_excp_flush_req & alu_excp_i_ecall & h_mode) begin
      excp_cause_w = 10;
    end else if (alu_excp_flush_req & alu_excp_i_ecall & m_mode) begin
      excp_cause_w = 11;
    end else if (longp_excp_flush_req & longp_excp_i_insterr) begin
      excp_cause_w = 16;
    end else begin
      excp_cause_w = 31;
    end
    // CSR updates
    cmt_cause = excp_taken_ena ? excp_cause_w : irq_cause_w;
    cmt_epc = longp_excp_i_valid ? longp_excp_i_pc : alu_excp_i_pc;
    // Badaddr
    if (excp_flush_by_longp_ldst) begin
      cmt_badaddr = longp_excp_i_badaddr;
    end else if (excp_flush_by_alu_agu) begin
      cmt_badaddr = alu_excp_i_badaddr;
    end else if ((alu_excp_flush_req & alu_excp_i_ebreak4excp) | (alu_excp_flush_req & alu_excp_i_ifu_misalgn) | (alu_excp_flush_req & alu_excp_i_ifu_buserr)) begin
      cmt_badaddr = alu_excp_i_pc;
    end else if (alu_excp_flush_req & alu_excp_i_ifu_ilegl) begin
      cmt_badaddr = alu_excp_i_instr;
    end else begin
      cmt_badaddr = 0;
    end
    cmt_epc_ena = ~dbg_mode & (excp_taken_ena | irq_taken_ena);
    cmt_cause_ena = cmt_epc_ena;
    cmt_status_ena = cmt_epc_ena;
    cmt_badaddr_ena = cmt_epc_ena & excpirq_flush_req;
    // Debug CSR updates
    cmt_dpc = alu_excp_i_pc;
    cmt_dpc_ena = dbg_entry_taken_ena;
    cmt_dcause = dbg_entry_taken_ena ? dbg_trig_req ? 2 : dbg_ebrk_req ? 1 : dbg_irq_req_w ? 3 : dbg_step_req ? 4 : dbg_halt_req ? 5 : 0 : 0;
    cmt_dcause_ena = dbg_entry_taken_ena | cmt_dret_ena;
  end

endmodule

// E203 HBirdv2 CSR Register File
// Machine-mode CSRs for RV32IM with debug support.
// Matches reference e203_exu_csr_ref.sv behavior exactly.
module e203_exu_csr #(
  parameter int XLEN = 32
) (
  input logic clk,
  input logic rst_n,
  input logic clk_aon,
  input logic nonflush_cmt_ena,
  input logic csr_ena,
  input logic csr_wr_en,
  input logic csr_rd_en,
  input logic [11:0] csr_idx,
  output logic csr_access_ilgl,
  output logic [31:0] read_csr_dat,
  input logic [31:0] wbck_csr_dat,
  output logic nice_xs_off,
  output logic tm_stop,
  output logic core_cgstop,
  output logic tcm_cgstop,
  output logic itcm_nohold,
  output logic mdv_nob2b,
  input logic core_mhartid,
  input logic ext_irq_r,
  input logic sft_irq_r,
  input logic tmr_irq_r,
  output logic status_mie_r,
  output logic mtie_r,
  output logic msie_r,
  output logic meie_r,
  output logic wr_dcsr_ena,
  output logic wr_dpc_ena,
  output logic wr_dscratch_ena,
  input logic [31:0] dcsr_r,
  input logic [31:0] dpc_r,
  input logic [31:0] dscratch_r,
  output logic [31:0] wr_csr_nxt,
  input logic dbg_mode,
  input logic dbg_stopcycle,
  output logic u_mode,
  output logic s_mode,
  output logic h_mode,
  output logic m_mode,
  input logic [31:0] cmt_badaddr,
  input logic cmt_badaddr_ena,
  input logic [31:0] cmt_epc,
  input logic cmt_epc_ena,
  input logic [31:0] cmt_cause,
  input logic cmt_cause_ena,
  input logic cmt_status_ena,
  input logic cmt_instret_ena,
  input logic cmt_mret_ena,
  output logic [31:0] csr_epc_r,
  output logic [31:0] csr_dpc_r,
  output logic [31:0] csr_mtvec_r
);

  // ── CSR access interface ──────────────────────────────────────────
  // ── Control outputs ───────────────────────────────────────────────
  // ── Hart ID ───────────────────────────────────────────────────────
  // ── Interrupt status outputs ──────────────────────────────────────
  // ── Debug CSR interface ───────────────────────────────────────────
  // ── Debug mode ────────────────────────────────────────────────────
  // ── Privilege mode outputs ────────────────────────────────────────
  // ── Commit inputs ─────────────────────────────────────────────────
  // ── CSR vector outputs ────────────────────────────────────────────
  // ── CSR select signals ────────────────────────────────────────────
  logic sel_mstatus;
  assign sel_mstatus = csr_idx == 'h300;
  logic sel_misa;
  assign sel_misa = csr_idx == 'h301;
  logic sel_mie;
  assign sel_mie = csr_idx == 'h304;
  logic sel_mtvec;
  assign sel_mtvec = csr_idx == 'h305;
  logic sel_mscratch;
  assign sel_mscratch = csr_idx == 'h340;
  logic sel_mepc;
  assign sel_mepc = csr_idx == 'h341;
  logic sel_mcause;
  assign sel_mcause = csr_idx == 'h342;
  logic sel_mbadaddr;
  assign sel_mbadaddr = csr_idx == 'h343;
  logic sel_mip;
  assign sel_mip = csr_idx == 'h344;
  logic sel_mcycle;
  assign sel_mcycle = csr_idx == 'hB00;
  logic sel_minstret;
  assign sel_minstret = csr_idx == 'hB02;
  logic sel_mcycleh;
  assign sel_mcycleh = csr_idx == 'hB80;
  logic sel_minstreth;
  assign sel_minstreth = csr_idx == 'hB82;
  logic sel_counterstop;
  assign sel_counterstop = csr_idx == 'hBFF;
  logic sel_mcgstop;
  assign sel_mcgstop = csr_idx == 'hBFE;
  logic sel_itcmnohold;
  assign sel_itcmnohold = csr_idx == 'hBFD;
  logic sel_mdvnob2b;
  assign sel_mdvnob2b = csr_idx == 'hBF0;
  logic sel_mvendorid;
  assign sel_mvendorid = csr_idx == 'hF11;
  logic sel_marchid;
  assign sel_marchid = csr_idx == 'hF12;
  logic sel_mimpid;
  assign sel_mimpid = csr_idx == 'hF13;
  logic sel_mhartid;
  assign sel_mhartid = csr_idx == 'hF14;
  logic sel_dcsr;
  assign sel_dcsr = csr_idx == 'h7B0;
  logic sel_dpc;
  assign sel_dpc = csr_idx == 'h7B1;
  logic sel_dscratch;
  assign sel_dscratch = csr_idx == 'h7B2;
  // ── Write/read enable gating ──────────────────────────────────────
  logic wbck_csr_wen;
  assign wbck_csr_wen = csr_wr_en & csr_ena;
  // Write strobes (one per CSR)
  logic wr_mstatus;
  assign wr_mstatus = sel_mstatus & csr_wr_en;
  logic wr_mie;
  assign wr_mie = sel_mie & csr_wr_en;
  logic wr_mtvec;
  assign wr_mtvec = sel_mtvec & csr_wr_en;
  logic wr_mscratch;
  assign wr_mscratch = sel_mscratch & csr_wr_en;
  logic wr_mepc;
  assign wr_mepc = sel_mepc & csr_wr_en;
  logic wr_mcause;
  assign wr_mcause = sel_mcause & csr_wr_en;
  logic wr_mbadaddr;
  assign wr_mbadaddr = sel_mbadaddr & csr_wr_en;
  logic wr_mcycle;
  assign wr_mcycle = sel_mcycle & csr_wr_en;
  logic wr_mcycleh;
  assign wr_mcycleh = sel_mcycleh & csr_wr_en;
  logic wr_minstret;
  assign wr_minstret = sel_minstret & csr_wr_en;
  logic wr_minstreth;
  assign wr_minstreth = sel_minstreth & csr_wr_en;
  logic wr_counterstop;
  assign wr_counterstop = sel_counterstop & csr_wr_en;
  logic wr_mcgstop;
  assign wr_mcgstop = sel_mcgstop & csr_wr_en;
  logic wr_itcmnohold;
  assign wr_itcmnohold = sel_itcmnohold & csr_wr_en;
  logic wr_mdvnob2b;
  assign wr_mdvnob2b = sel_mdvnob2b & csr_wr_en;
  // Read strobes (debug CSRs gated by dbg_mode)
  logic rd_mstatus;
  assign rd_mstatus = csr_rd_en & sel_mstatus;
  logic rd_misa;
  assign rd_misa = csr_rd_en & sel_misa;
  logic rd_mie;
  assign rd_mie = csr_rd_en & sel_mie;
  logic rd_mtvec;
  assign rd_mtvec = csr_rd_en & sel_mtvec;
  logic rd_mscratch;
  assign rd_mscratch = csr_rd_en & sel_mscratch;
  logic rd_mepc;
  assign rd_mepc = csr_rd_en & sel_mepc;
  logic rd_mcause;
  assign rd_mcause = csr_rd_en & sel_mcause;
  logic rd_mbadaddr;
  assign rd_mbadaddr = csr_rd_en & sel_mbadaddr;
  logic rd_mip;
  assign rd_mip = csr_rd_en & sel_mip;
  logic rd_mcycle;
  assign rd_mcycle = csr_rd_en & sel_mcycle;
  logic rd_mcycleh;
  assign rd_mcycleh = csr_rd_en & sel_mcycleh;
  logic rd_minstret;
  assign rd_minstret = csr_rd_en & sel_minstret;
  logic rd_minstreth;
  assign rd_minstreth = csr_rd_en & sel_minstreth;
  logic rd_counterstop;
  assign rd_counterstop = csr_rd_en & sel_counterstop;
  logic rd_mcgstop;
  assign rd_mcgstop = csr_rd_en & sel_mcgstop;
  logic rd_itcmnohold;
  assign rd_itcmnohold = csr_rd_en & sel_itcmnohold;
  logic rd_mdvnob2b;
  assign rd_mdvnob2b = csr_rd_en & sel_mdvnob2b;
  logic rd_mvendorid;
  assign rd_mvendorid = csr_rd_en & sel_mvendorid;
  logic rd_marchid;
  assign rd_marchid = csr_rd_en & sel_marchid;
  logic rd_mimpid;
  assign rd_mimpid = csr_rd_en & sel_mimpid;
  logic rd_mhartid;
  assign rd_mhartid = csr_rd_en & sel_mhartid;
  logic rd_dcsr;
  assign rd_dcsr = dbg_mode & csr_rd_en & sel_dcsr;
  logic rd_dpc;
  assign rd_dpc = dbg_mode & csr_rd_en & sel_dpc;
  logic rd_dscratch;
  assign rd_dscratch = dbg_mode & csr_rd_en & sel_dscratch;
  // ── Register file ──────────────────────────────────────────────────
  // mstatus: separate MIE/MPIE DFFs matching reference
  logic mie_bit_r = 1'b0;
  logic mpie_bit_r = 1'b0;
  logic [31:0] mie_r = 0;
  logic [31:0] mtvec_r = 0;
  logic [31:0] mscratch_r = 0;
  logic [31:0] mepc_r = 0;
  logic [31:0] mcause_r = 0;
  logic [31:0] mbadaddr_r = 0;
  logic [31:0] mcycle_r = 0;
  logic [31:0] mcycleh_r = 0;
  logic [31:0] minstret_r = 0;
  logic [31:0] minstreth_r = 0;
  logic [31:0] counterstop_r = 0;
  logic [31:0] mcgstop_r = 0;
  logic [31:0] itcmnohold_r = 0;
  logic [31:0] mdvnob2b_r = 0;
  // mip: registered interrupt pending bits (read-only)
  logic meip_r = 1'b0;
  logic msip_r = 1'b0;
  logic mtip_r = 1'b0;
  // ── MIE/MPIE next-value logic (priority: trap > mret > csr_write > hold) ──
  logic mstatus_wen;
  assign mstatus_wen = wr_mstatus & wbck_csr_wen;
  logic status_mpie_ena;
  assign status_mpie_ena = mstatus_wen | cmt_mret_ena | cmt_status_ena;
  logic status_mie_ena;
  assign status_mie_ena = status_mpie_ena;
  logic status_mpie_nxt;
  assign status_mpie_nxt = cmt_status_ena ? mie_bit_r : cmt_mret_ena ? 1'b1 : mstatus_wen ? wbck_csr_dat[7:7] != 0 : mpie_bit_r;
  logic status_mie_nxt;
  assign status_mie_nxt = cmt_status_ena ? 1'b0 : cmt_mret_ena ? mpie_bit_r : mstatus_wen ? wbck_csr_dat[3:3] != 0 : mie_bit_r;
  // ── Assembled mstatus for CSR reads ────────────────────────────────
  // SD=0, XS=0, FS=0, MPP=2'b11, SPP=0, SPIE=0, UPIE=0, SIE=0, UIE=0
  logic [31:0] csr_mstatus;
  assign csr_mstatus = {1'b0, {8{1'b0}}, {6{1'b0}}, {2{1'b0}}, {2{1'b0}}, {2{1'b1}}, {2{1'b0}}, 1'b0, mpie_bit_r, 1'b0, 1'b0, 1'b0, mie_bit_r, 1'b0, 1'b0, 1'b0};
  // 31: SD
  // 30:23 Reserved
  // 22:17 TSR--MPRV
  // 16:15 XS
  // 14:13 FS
  // 12:11 MPP
  // 10:9 Reserved
  // 8: SPP
  // 7: MPIE
  // 6: Reserved
  // 5: SPIE
  // 4: UPIE
  // 3: MIE
  // 2: Reserved
  // 1: SIE
  // 0: UIE
  // ── Other CSR assembled values ─────────────────────────────────────
  // mie: only bits 11 (MEIE), 7 (MTIE), 3 (MSIE) writable
  logic mie_ena;
  assign mie_ena = wr_mie & wbck_csr_wen;
  logic [31:0] mie_nxt;
  assign mie_nxt = {{20{1'b0}}, wbck_csr_dat[11:11], {3{1'b0}}, wbck_csr_dat[7:7], {3{1'b0}}, wbck_csr_dat[3:3], {3{1'b0}}};
  // mtvec
  logic mtvec_ena;
  assign mtvec_ena = wr_mtvec & wbck_csr_wen;
  logic [31:0] mtvec_nxt;
  assign mtvec_nxt = wbck_csr_dat;
  // mscratch
  logic mscratch_ena;
  assign mscratch_ena = wr_mscratch & wbck_csr_wen;
  logic [31:0] mscratch_nxt;
  assign mscratch_nxt = wbck_csr_dat;
  // mepc: bit 0 hardwired to 0, cmt_epc_ena takes priority over csr write
  logic mepc_ena;
  assign mepc_ena = (wr_mepc & wbck_csr_wen) | cmt_epc_ena;
  logic [31:0] mepc_nxt;
  assign mepc_nxt = cmt_epc_ena ? {cmt_epc[31:1], 1'b0} : {wbck_csr_dat[31:1], 1'b0};
  // mcause: only bit 31 (interrupt) and bits 3:0 (exception code) stored
  logic mcause_ena;
  assign mcause_ena = (wr_mcause & wbck_csr_wen) | cmt_cause_ena;
  logic [31:0] mcause_nxt;
  assign mcause_nxt = cmt_cause_ena ? {cmt_cause[31:31], {27{1'b0}}, cmt_cause[3:0]} : {wbck_csr_dat[31:31], {27{1'b0}}, wbck_csr_dat[3:0]};
  // mbadaddr
  logic mbadaddr_ena;
  assign mbadaddr_ena = (wr_mbadaddr & wbck_csr_wen) | cmt_badaddr_ena;
  logic [31:0] mbadaddr_nxt;
  assign mbadaddr_nxt = cmt_badaddr_ena ? cmt_badaddr : wbck_csr_dat;
  // ── Counter stop logic ─────────────────────────────────────────────
  logic cy_stop;
  assign cy_stop = counterstop_r[0:0] != 0;
  logic ir_stop;
  assign ir_stop = counterstop_r[2:2] != 0;
  logic stop_cycle_in_dbg;
  assign stop_cycle_in_dbg = dbg_stopcycle & dbg_mode;
  // ── Counter enables and next values ────────────────────────────────
  // mcycle increments every cycle unless stopped
  logic mcycle_wr_ena;
  assign mcycle_wr_ena = wr_mcycle & wbck_csr_wen;
  logic mcycleh_wr_ena;
  assign mcycleh_wr_ena = wr_mcycleh & wbck_csr_wen;
  logic minstret_wr_ena;
  assign minstret_wr_ena = wr_minstret & wbck_csr_wen;
  logic minstreth_wr_ena;
  assign minstreth_wr_ena = wr_minstreth & wbck_csr_wen;
  logic mcycle_ena;
  assign mcycle_ena = mcycle_wr_ena | (~cy_stop & ~stop_cycle_in_dbg);
  logic mcycleh_ena;
  assign mcycleh_ena = mcycleh_wr_ena | (~cy_stop & ~stop_cycle_in_dbg & (mcycle_r == 'hFFFFFFFF));
  logic minstret_ena;
  assign minstret_ena = minstret_wr_ena | (~ir_stop & ~stop_cycle_in_dbg & cmt_instret_ena);
  logic minstreth_ena;
  assign minstreth_ena = minstreth_wr_ena | (~ir_stop & ~stop_cycle_in_dbg & cmt_instret_ena & (minstret_r == 'hFFFFFFFF));
  logic [31:0] mcycle_nxt;
  assign mcycle_nxt = mcycle_wr_ena ? wbck_csr_dat : 32'(mcycle_r + 1);
  logic [31:0] mcycleh_nxt;
  assign mcycleh_nxt = mcycleh_wr_ena ? wbck_csr_dat : 32'(mcycleh_r + 1);
  logic [31:0] minstret_nxt;
  assign minstret_nxt = minstret_wr_ena ? wbck_csr_dat : 32'(minstret_r + 1);
  logic [31:0] minstreth_nxt;
  assign minstreth_nxt = minstreth_wr_ena ? wbck_csr_dat : 32'(minstreth_r + 1);
  // ── Custom CSR enables and next values ─────────────────────────────
  logic counterstop_wr_ena;
  assign counterstop_wr_ena = wr_counterstop & wbck_csr_wen;
  logic mcgstop_wr_ena;
  assign mcgstop_wr_ena = wr_mcgstop & wbck_csr_wen;
  logic itcmnohold_wr_ena;
  assign itcmnohold_wr_ena = wr_itcmnohold & wbck_csr_wen;
  logic mdvnob2b_wr_ena;
  assign mdvnob2b_wr_ena = wr_mdvnob2b & wbck_csr_wen;
  logic [31:0] counterstop_nxt;
  assign counterstop_nxt = {{29{1'b0}}, wbck_csr_dat[2:0]};
  logic [31:0] mcgstop_nxt;
  assign mcgstop_nxt = {{30{1'b0}}, wbck_csr_dat[1:0]};
  logic [31:0] itcmnohold_nxt;
  assign itcmnohold_nxt = {{31{1'b0}}, wbck_csr_dat[0:0]};
  logic [31:0] mdvnob2b_nxt;
  assign mdvnob2b_nxt = {{31{1'b0}}, wbck_csr_dat[0:0]};
  // ── mip assembled value for reads ──────────────────────────────────
  logic [31:0] csr_mip;
  assign csr_mip = {{20{1'b0}}, meip_r, {3{1'b0}}, mtip_r, {3{1'b0}}, msip_r, {3{1'b0}}};
  // ── Other CSR read-only values ─────────────────────────────────────
  // misa: RV32IMAC (MXL=1, M=1, I=1, C=1, A=1)
  logic [31:0] csr_misa;
  assign csr_misa = 'h40001105;
  logic [31:0] csr_mvendorid;
  assign csr_mvendorid = 'h536;
  logic [31:0] csr_marchid;
  assign csr_marchid = 'hE203;
  logic [31:0] csr_mimpid;
  assign csr_mimpid = 'h1;
  logic [31:0] csr_mhartid;
  assign csr_mhartid = 32'($unsigned(core_mhartid));
  // Debug CSRs: pass through inputs
  logic [31:0] csr_dcsr;
  assign csr_dcsr = dcsr_r;
  logic [31:0] csr_dpc;
  assign csr_dpc = dpc_r;
  logic [31:0] csr_dscratch;
  assign csr_dscratch = dscratch_r;
  // ── Sequential logic on always-on clock (mcycle/mcycleh) ───────────
  always_ff @(posedge clk_aon or negedge rst_n) begin
    if ((!rst_n)) begin
      mcycle_r <= 0;
      mcycleh_r <= 0;
    end else begin
      if (mcycle_ena) begin
        mcycle_r <= mcycle_nxt;
      end
      if (mcycleh_ena) begin
        mcycleh_r <= mcycleh_nxt;
      end
    end
  end
  // ── Sequential logic on main clock (all other registers) ───────────
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      counterstop_r <= 0;
      itcmnohold_r <= 0;
      mbadaddr_r <= 0;
      mcause_r <= 0;
      mcgstop_r <= 0;
      mdvnob2b_r <= 0;
      meip_r <= 1'b0;
      mepc_r <= 0;
      mie_bit_r <= 1'b0;
      mie_r <= 0;
      minstret_r <= 0;
      minstreth_r <= 0;
      mpie_bit_r <= 1'b0;
      mscratch_r <= 0;
      msip_r <= 1'b0;
      mtip_r <= 1'b0;
      mtvec_r <= 0;
    end else begin
      // mstatus MIE/MPIE (shared enable)
      if (status_mie_ena) begin
        mie_bit_r <= status_mie_nxt;
        mpie_bit_r <= status_mpie_nxt;
      end
      // mie
      if (mie_ena) begin
        mie_r <= mie_nxt;
      end
      // mtvec
      if (mtvec_ena) begin
        mtvec_r <= mtvec_nxt;
      end
      // mscratch
      if (mscratch_ena) begin
        mscratch_r <= mscratch_nxt;
      end
      // mepc
      if (mepc_ena) begin
        mepc_r <= mepc_nxt;
      end
      // mcause
      if (mcause_ena) begin
        mcause_r <= mcause_nxt;
      end
      // mbadaddr
      if (mbadaddr_ena) begin
        mbadaddr_r <= mbadaddr_nxt;
      end
      // minstret/minstreth
      if (minstret_ena) begin
        minstret_r <= minstret_nxt;
      end
      if (minstreth_ena) begin
        minstreth_r <= minstreth_nxt;
      end
      // Custom CSRs
      if (counterstop_wr_ena) begin
        counterstop_r <= counterstop_nxt;
      end
      if (mcgstop_wr_ena) begin
        mcgstop_r <= mcgstop_nxt;
      end
      if (itcmnohold_wr_ena) begin
        itcmnohold_r <= itcmnohold_nxt;
      end
      if (mdvnob2b_wr_ena) begin
        mdvnob2b_r <= mdvnob2b_nxt;
      end
      // mip: always sample interrupt pending inputs
      meip_r <= ext_irq_r;
      msip_r <= sft_irq_r;
      mtip_r <= tmr_irq_r;
    end
  end
  // ── Combinational outputs ──────────────────────────────────────────
  assign read_csr_dat = (rd_mstatus ? csr_mstatus : 0) | (rd_misa ? csr_misa : 0) | (rd_mie ? mie_r : 0) | (rd_mtvec ? mtvec_r : 0) | (rd_mscratch ? mscratch_r : 0) | (rd_mepc ? mepc_r : 0) | (rd_mcause ? mcause_r : 0) | (rd_mbadaddr ? mbadaddr_r : 0) | (rd_mip ? csr_mip : 0) | (rd_mcycle ? mcycle_r : 0) | (rd_mcycleh ? mcycleh_r : 0) | (rd_minstret ? minstret_r : 0) | (rd_minstreth ? minstreth_r : 0) | (rd_counterstop ? counterstop_r : 0) | (rd_mcgstop ? mcgstop_r : 0) | (rd_itcmnohold ? itcmnohold_r : 0) | (rd_mdvnob2b ? mdvnob2b_r : 0) | (rd_mvendorid ? csr_mvendorid : 0) | (rd_marchid ? csr_marchid : 0) | (rd_mimpid ? csr_mimpid : 0) | (rd_mhartid ? csr_mhartid : 0) | (rd_dcsr ? csr_dcsr : 0) | (rd_dpc ? csr_dpc : 0) | (rd_dscratch ? csr_dscratch : 0);
  assign status_mie_r = mie_bit_r;
  assign meie_r = mie_r[11:11] != 0;
  assign mtie_r = mie_r[7:7] != 0;
  assign msie_r = mie_r[3:3] != 0;
  assign csr_epc_r = mepc_r;
  assign csr_dpc_r = dpc_r;
  assign csr_mtvec_r = mtvec_r;
  assign nice_xs_off = 1'b0;
  assign tm_stop = counterstop_r[1:1] != 0;
  assign core_cgstop = mcgstop_r[0:0] != 0;
  assign tcm_cgstop = mcgstop_r[1:1] != 0;
  assign itcm_nohold = itcmnohold_r[0:0] != 0;
  assign mdv_nob2b = mdvnob2b_r[0:0] != 0;
  assign u_mode = 1'b0;
  assign s_mode = 1'b0;
  assign h_mode = 1'b0;
  assign m_mode = 1'b1;
  assign csr_access_ilgl = 1'b0;
  assign wr_dcsr_ena = dbg_mode & csr_wr_en & sel_dcsr;
  assign wr_dpc_ena = dbg_mode & csr_wr_en & sel_dpc;
  assign wr_dscratch_ena = dbg_mode & csr_wr_en & sel_dscratch;
  assign wr_csr_nxt = wbck_csr_dat;

endmodule

// OR-based read mux matching reference pattern
// Interrupt enables from mie register
// CSR vector outputs
// Control outputs from custom CSRs
// Privilege mode (machine mode only)
// CSR access (always legal)
// Debug CSR write enables (gated by dbg_mode)
// E203 HBirdv2 integer register file
// 32 x 32-bit, 2 async read ports, 1 sync write port.
// x0 hardwired to 0. x1 exposed as output for link register.
// No reset on data entries (matches E203 spec).
module e203_exu_regfile #(
  parameter int XLEN = 32,
  parameter int NREGS = 32
) (
  input logic clk,
  input logic rst_n,
  input logic test_mode,
  input logic [4:0] read_src1_idx,
  output logic [31:0] read_src1_dat,
  input logic [4:0] read_src2_idx,
  output logic [31:0] read_src2_dat,
  input logic wbck_dest_wen,
  input logic [4:0] wbck_dest_idx,
  input logic [31:0] wbck_dest_dat,
  output logic [31:0] x1_r
);

  // Read port 1
  // Read port 2
  // Write port
  // x1 (ra) exposed for IFU link-register read
  // Register file storage — 32 registers, no reset
  logic [31:0] rf_0 = 0;
  logic [31:0] rf_1 = 0;
  logic [31:0] rf_2 = 0;
  logic [31:0] rf_3 = 0;
  logic [31:0] rf_4 = 0;
  logic [31:0] rf_5 = 0;
  logic [31:0] rf_6 = 0;
  logic [31:0] rf_7 = 0;
  logic [31:0] rf_8 = 0;
  logic [31:0] rf_9 = 0;
  logic [31:0] rf_10 = 0;
  logic [31:0] rf_11 = 0;
  logic [31:0] rf_12 = 0;
  logic [31:0] rf_13 = 0;
  logic [31:0] rf_14 = 0;
  logic [31:0] rf_15 = 0;
  logic [31:0] rf_16 = 0;
  logic [31:0] rf_17 = 0;
  logic [31:0] rf_18 = 0;
  logic [31:0] rf_19 = 0;
  logic [31:0] rf_20 = 0;
  logic [31:0] rf_21 = 0;
  logic [31:0] rf_22 = 0;
  logic [31:0] rf_23 = 0;
  logic [31:0] rf_24 = 0;
  logic [31:0] rf_25 = 0;
  logic [31:0] rf_26 = 0;
  logic [31:0] rf_27 = 0;
  logic [31:0] rf_28 = 0;
  logic [31:0] rf_29 = 0;
  logic [31:0] rf_30 = 0;
  logic [31:0] rf_31 = 0;
  // Write port — x0 is hardwired to 0 (skip writes to index 0)
  always_ff @(posedge clk) begin
    if (wbck_dest_wen & (wbck_dest_idx != 0)) begin
      if (wbck_dest_idx == 1) begin
        rf_1 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 2) begin
        rf_2 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 3) begin
        rf_3 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 4) begin
        rf_4 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 5) begin
        rf_5 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 6) begin
        rf_6 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 7) begin
        rf_7 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 8) begin
        rf_8 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 9) begin
        rf_9 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 10) begin
        rf_10 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 11) begin
        rf_11 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 12) begin
        rf_12 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 13) begin
        rf_13 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 14) begin
        rf_14 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 15) begin
        rf_15 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 16) begin
        rf_16 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 17) begin
        rf_17 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 18) begin
        rf_18 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 19) begin
        rf_19 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 20) begin
        rf_20 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 21) begin
        rf_21 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 22) begin
        rf_22 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 23) begin
        rf_23 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 24) begin
        rf_24 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 25) begin
        rf_25 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 26) begin
        rf_26 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 27) begin
        rf_27 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 28) begin
        rf_28 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 29) begin
        rf_29 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 30) begin
        rf_30 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 31) begin
        rf_31 <= wbck_dest_dat;
      end
    end
  end
  // Read ports — async (combinational), x0 always reads 0
  always_comb begin
    if (read_src1_idx == 0) begin
      read_src1_dat = 0;
    end else if (read_src1_idx == 1) begin
      read_src1_dat = rf_1;
    end else if (read_src1_idx == 2) begin
      read_src1_dat = rf_2;
    end else if (read_src1_idx == 3) begin
      read_src1_dat = rf_3;
    end else if (read_src1_idx == 4) begin
      read_src1_dat = rf_4;
    end else if (read_src1_idx == 5) begin
      read_src1_dat = rf_5;
    end else if (read_src1_idx == 6) begin
      read_src1_dat = rf_6;
    end else if (read_src1_idx == 7) begin
      read_src1_dat = rf_7;
    end else if (read_src1_idx == 8) begin
      read_src1_dat = rf_8;
    end else if (read_src1_idx == 9) begin
      read_src1_dat = rf_9;
    end else if (read_src1_idx == 10) begin
      read_src1_dat = rf_10;
    end else if (read_src1_idx == 11) begin
      read_src1_dat = rf_11;
    end else if (read_src1_idx == 12) begin
      read_src1_dat = rf_12;
    end else if (read_src1_idx == 13) begin
      read_src1_dat = rf_13;
    end else if (read_src1_idx == 14) begin
      read_src1_dat = rf_14;
    end else if (read_src1_idx == 15) begin
      read_src1_dat = rf_15;
    end else if (read_src1_idx == 16) begin
      read_src1_dat = rf_16;
    end else if (read_src1_idx == 17) begin
      read_src1_dat = rf_17;
    end else if (read_src1_idx == 18) begin
      read_src1_dat = rf_18;
    end else if (read_src1_idx == 19) begin
      read_src1_dat = rf_19;
    end else if (read_src1_idx == 20) begin
      read_src1_dat = rf_20;
    end else if (read_src1_idx == 21) begin
      read_src1_dat = rf_21;
    end else if (read_src1_idx == 22) begin
      read_src1_dat = rf_22;
    end else if (read_src1_idx == 23) begin
      read_src1_dat = rf_23;
    end else if (read_src1_idx == 24) begin
      read_src1_dat = rf_24;
    end else if (read_src1_idx == 25) begin
      read_src1_dat = rf_25;
    end else if (read_src1_idx == 26) begin
      read_src1_dat = rf_26;
    end else if (read_src1_idx == 27) begin
      read_src1_dat = rf_27;
    end else if (read_src1_idx == 28) begin
      read_src1_dat = rf_28;
    end else if (read_src1_idx == 29) begin
      read_src1_dat = rf_29;
    end else if (read_src1_idx == 30) begin
      read_src1_dat = rf_30;
    end else begin
      read_src1_dat = rf_31;
    end
    if (read_src2_idx == 0) begin
      read_src2_dat = 0;
    end else if (read_src2_idx == 1) begin
      read_src2_dat = rf_1;
    end else if (read_src2_idx == 2) begin
      read_src2_dat = rf_2;
    end else if (read_src2_idx == 3) begin
      read_src2_dat = rf_3;
    end else if (read_src2_idx == 4) begin
      read_src2_dat = rf_4;
    end else if (read_src2_idx == 5) begin
      read_src2_dat = rf_5;
    end else if (read_src2_idx == 6) begin
      read_src2_dat = rf_6;
    end else if (read_src2_idx == 7) begin
      read_src2_dat = rf_7;
    end else if (read_src2_idx == 8) begin
      read_src2_dat = rf_8;
    end else if (read_src2_idx == 9) begin
      read_src2_dat = rf_9;
    end else if (read_src2_idx == 10) begin
      read_src2_dat = rf_10;
    end else if (read_src2_idx == 11) begin
      read_src2_dat = rf_11;
    end else if (read_src2_idx == 12) begin
      read_src2_dat = rf_12;
    end else if (read_src2_idx == 13) begin
      read_src2_dat = rf_13;
    end else if (read_src2_idx == 14) begin
      read_src2_dat = rf_14;
    end else if (read_src2_idx == 15) begin
      read_src2_dat = rf_15;
    end else if (read_src2_idx == 16) begin
      read_src2_dat = rf_16;
    end else if (read_src2_idx == 17) begin
      read_src2_dat = rf_17;
    end else if (read_src2_idx == 18) begin
      read_src2_dat = rf_18;
    end else if (read_src2_idx == 19) begin
      read_src2_dat = rf_19;
    end else if (read_src2_idx == 20) begin
      read_src2_dat = rf_20;
    end else if (read_src2_idx == 21) begin
      read_src2_dat = rf_21;
    end else if (read_src2_idx == 22) begin
      read_src2_dat = rf_22;
    end else if (read_src2_idx == 23) begin
      read_src2_dat = rf_23;
    end else if (read_src2_idx == 24) begin
      read_src2_dat = rf_24;
    end else if (read_src2_idx == 25) begin
      read_src2_dat = rf_25;
    end else if (read_src2_idx == 26) begin
      read_src2_dat = rf_26;
    end else if (read_src2_idx == 27) begin
      read_src2_dat = rf_27;
    end else if (read_src2_idx == 28) begin
      read_src2_dat = rf_28;
    end else if (read_src2_idx == 29) begin
      read_src2_dat = rf_29;
    end else if (read_src2_idx == 30) begin
      read_src2_dat = rf_30;
    end else begin
      read_src2_dat = rf_31;
    end
    // x1 (ra) direct output
    x1_r = rf_1;
  end

endmodule

// E203 Execution Unit Integration Module
// Integrates: Decode -> Dispatch -> ALU/OITF -> LongpWbck -> Wbck -> Commit
// Plus: CSR register file, integer register file
// 2-stage pipeline: IFU input -> decode -> dispatch -> ALU/OITF -> writeback -> commit
module e203_exu #(
  parameter int XLEN = 32
) (
  input logic clk,
  input logic rst_n,
  input logic i_valid,
  output logic i_ready,
  input logic [31:0] i_ir,
  input logic [31:0] i_pc,
  input logic i_pc_vld,
  input logic i_misalgn,
  input logic i_buserr,
  input logic i_prdt_taken,
  input logic i_muldiv_b2b,
  input logic [4:0] i_rs1idx,
  input logic [4:0] i_rs2idx,
  output logic pipe_flush_req,
  input logic pipe_flush_ack,
  output logic [31:0] pipe_flush_add_op1,
  output logic [31:0] pipe_flush_add_op2,
  output logic [31:0] pipe_flush_pc,
  input logic lsu_o_valid,
  output logic lsu_o_ready,
  input logic [31:0] lsu_o_wbck_wdat,
  input logic [4:0] lsu_o_wbck_itag,
  input logic lsu_o_wbck_err,
  input logic lsu_o_cmt_ld,
  input logic lsu_o_cmt_st,
  input logic [31:0] lsu_o_cmt_badaddr,
  input logic lsu_o_cmt_buserr,
  output logic agu_icb_cmd_valid,
  input logic agu_icb_cmd_ready,
  output logic [31:0] agu_icb_cmd_addr,
  output logic agu_icb_cmd_read,
  output logic [31:0] agu_icb_cmd_wdata,
  output logic [3:0] agu_icb_cmd_wmask,
  output logic agu_icb_cmd_lock,
  output logic agu_icb_cmd_excl,
  output logic [1:0] agu_icb_cmd_size,
  output logic agu_icb_cmd_back2agu,
  output logic agu_icb_cmd_usign,
  output logic agu_icb_cmd_itag,
  input logic agu_icb_rsp_valid,
  output logic agu_icb_rsp_ready,
  input logic [31:0] agu_icb_rsp_rdata,
  input logic agu_icb_rsp_err,
  input logic agu_icb_rsp_excl_ok,
  input logic dbg_mode,
  input logic dbg_halt_r,
  input logic dbg_step_r,
  input logic dbg_ebreakm_r,
  input logic dbg_stopcycle,
  input logic dbg_irq_r,
  input logic lcl_irq_r,
  input logic evt_r,
  input logic ext_irq_r,
  input logic sft_irq_r,
  input logic tmr_irq_r,
  output logic [31:0] cmt_dpc,
  output logic cmt_dpc_ena,
  output logic [2:0] cmt_dcause,
  output logic cmt_dcause_ena,
  output logic wr_dcsr_ena,
  output logic wr_dpc_ena,
  output logic wr_dscratch_ena,
  output logic [31:0] wr_csr_nxt,
  input logic [31:0] dcsr_r,
  input logic [31:0] dpc_r,
  input logic [31:0] dscratch_r,
  output logic wfi_halt_ifu_req,
  input logic wfi_halt_ifu_ack,
  output logic core_wfi,
  output logic [31:0] rf2ifu_x1,
  output logic [31:0] rf2ifu_rs1,
  output logic dec2ifu_rden,
  output logic dec2ifu_rs1en,
  output logic [4:0] dec2ifu_rdidx,
  output logic dec2ifu_mulhsu,
  output logic dec2ifu_div,
  output logic dec2ifu_rem,
  output logic dec2ifu_divu,
  output logic dec2ifu_remu,
  output logic oitf_empty,
  output logic exu_active,
  output logic excp_active,
  output logic commit_mret,
  output logic commit_trap,
  input logic [31:0] core_mhartid,
  output logic tm_stop,
  output logic itcm_nohold,
  output logic core_cgstop,
  output logic tcm_cgstop,
  output logic nice_req_valid,
  input logic nice_req_ready,
  output logic [31:0] nice_req_inst,
  output logic [31:0] nice_req_rs1,
  output logic [31:0] nice_req_rs2,
  input logic nice_rsp_multicyc_valid,
  output logic nice_rsp_multicyc_ready,
  input logic [31:0] nice_rsp_multicyc_dat,
  input logic nice_rsp_multicyc_err,
  input logic test_mode,
  input logic clk_aon
);

  // ══════════════════════════════════════════════════════════════════════
  // IFU interface
  // ══════════════════════════════════════════════════════════════════════
  // ══════════════════════════════════════════════════════════════════════
  // Pipe flush interface
  // ══════════════════════════════════════════════════════════════════════
  // ══════════════════════════════════════════════════════════════════════
  // LSU writeback interface
  // ══════════════════════════════════════════════════════════════════════
  // ══════════════════════════════════════════════════════════════════════
  // AGU ICB command interface
  // ══════════════════════════════════════════════════════════════════════
  // AGU ICB response interface
  // ══════════════════════════════════════════════════════════════════════
  // Debug signals
  // ══════════════════════════════════════════════════════════════════════
  // ══════════════════════════════════════════════════════════════════════
  // IRQ inputs
  // ══════════════════════════════════════════════════════════════════════
  // ══════════════════════════════════════════════════════════════════════
  // CSR debug interface
  // ══════════════════════════════════════════════════════════════════════
  // ══════════════════════════════════════════════════════════════════════
  // WFI signals
  // ══════════════════════════════════════════════════════════════════════
  // ══════════════════════════════════════════════════════════════════════
  // Regfile to IFU
  // ══════════════════════════════════════════════════════════════════════
  // ══════════════════════════════════════════════════════════════════════
  // Misc status/control
  // ══════════════════════════════════════════════════════════════════════
  // NICE coprocessor interface
  // ════════════════════════════════════════════════════════════════════
  // Internal wires -- Decode outputs
  // ════════════════════════════════════════════════════════════════════
  logic [4:0] dec_rs1_idx;
  logic [4:0] dec_rs2_idx;
  logic [4:0] dec_rd_idx;
  logic [31:0] dec_imm;
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
  logic dec_mul;
  logic dec_mulh;
  logic dec_mulhsu;
  logic dec_mulhu;
  logic dec_div;
  logic dec_divu;
  logic dec_rem;
  logic dec_remu;
  logic dec_load;
  logic dec_store;
  logic dec_rs1_en;
  logic dec_rs2_en;
  logic dec_rd_en;
  logic dec_rs1x0;
  logic dec_rs2x0;
  logic [31:0] dec_info;
  logic [31:0] dec_pc_out;
  logic dec_misalgn;
  logic dec_buserr_out;
  logic dec_ilegl;
  logic dec_nice;
  logic dec_nice_cmt_off_ilgl;
  logic dec_rv32;
  logic dec_jal;
  logic dec_jalr;
  logic dec_bxx;
  logic [4:0] dec_jalr_rs1idx;
  logic [31:0] dec_bjp_imm;
  // ════════════════════════════════════════════════════════════════════
  // Internal wires -- Regfile read data
  // ════════════════════════════════════════════════════════════════════
  logic [31:0] rf_rs1_data;
  logic [31:0] rf_rs2_data;
  // ════════════════════════════════════════════════════════════════════
  // Internal wires -- Dispatch outputs
  // ════════════════════════════════════════════════════════════════════
  logic disp_rdy;
  logic disp_wfi_halt_exu_ack;
  // Dispatch -> ALU
  logic disp_alu_valid;
  logic [31:0] disp_alu_rs1;
  logic [31:0] disp_alu_rs2;
  logic [31:0] disp_alu_pc;
  logic [31:0] disp_alu_imm;
  logic [4:0] disp_alu_rdidx;
  logic disp_alu_rdwen;
  logic [31:0] disp_alu_info;
  logic [0:0] disp_alu_itag;
  logic disp_alu_misalgn;
  logic disp_alu_buserr;
  logic disp_alu_ilegl;
  // Dispatch -> OITF
  logic disp_oitf_rs1fpu_w;
  logic disp_oitf_rs2fpu_w;
  logic disp_oitf_rs3fpu_w;
  logic disp_oitf_rdfpu_w;
  logic disp_oitf_rs1en_w;
  logic disp_oitf_rs2en_w;
  logic disp_oitf_rs3en_w;
  logic disp_oitf_rdwen_w;
  logic [4:0] disp_oitf_rs1idx_w;
  logic [4:0] disp_oitf_rs2idx_w;
  logic [4:0] disp_oitf_rs3idx_w;
  logic [4:0] disp_oitf_rdidx_w;
  logic [31:0] disp_oitf_pc_w;
  // ════════════════════════════════════════════════════════════════════
  // Internal wires -- OITF
  // ════════════════════════════════════════════════════════════════════
  logic oitf_dis_ready;
  logic [0:0] oitf_dis_ptr;
  logic [0:0] oitf_ret_ptr;
  logic [4:0] oitf_ret_rdidx;
  logic oitf_ret_rdwen;
  logic oitf_ret_rdfpu;
  logic [31:0] oitf_ret_pc;
  logic oitf_match_rs1;
  logic oitf_match_rs2;
  logic oitf_match_rs3;
  logic oitf_match_rd;
  logic oitf_is_empty;
  // ════════════════════════════════════════════════════════════════════
  // Internal wires -- ALU outputs
  // ════════════════════════════════════════════════════════════════════
  logic alu_o_ready;
  logic alu_longpipe;
  logic [31:0] alu_wdat;
  logic [4:0] alu_rdidx;
  logic alu_wbck_valid;
  // ALU commit outputs
  logic alu_cmt_valid;
  logic alu_cmt_pc_vld;
  logic [31:0] alu_cmt_pc;
  logic [31:0] alu_cmt_instr;
  logic [31:0] alu_cmt_imm;
  logic alu_cmt_rv32;
  logic alu_cmt_bjp;
  logic alu_cmt_mret;
  logic alu_cmt_dret;
  logic alu_cmt_ecall;
  logic alu_cmt_ebreak;
  logic alu_cmt_fencei;
  logic alu_cmt_wfi;
  logic alu_cmt_ifu_misalgn;
  logic alu_cmt_ifu_buserr;
  logic alu_cmt_ifu_ilegl;
  logic alu_cmt_bjp_prdt;
  logic alu_cmt_bjp_rslv;
  logic alu_cmt_misalgn;
  logic alu_cmt_ld;
  logic alu_cmt_stamo;
  logic alu_cmt_buserr;
  logic [31:0] alu_cmt_badaddr;
  // ALU CSR interface
  logic alu_csr_ena;
  logic alu_csr_wr_en;
  logic alu_csr_rd_en;
  logic [11:0] alu_csr_idx;
  logic [31:0] alu_wbck_csr_dat;
  // ALU NICE
  logic alu_nice_longp_wbck_valid;
  logic alu_nice_longp_wbck_ready;
  logic [0:0] alu_nice_o_itag;
  // ALU misc wires
  logic amo_wait_w;
  logic oitf_empty_w;
  logic pipe_flush_pulse_w;
  logic [31:0] read_csr_dat_w;
  // Glue wires - ALU outputs to wbck input
  logic alu_done_valid;
  // ════════════════════════════════════════════════════════════════════
  // Internal wires -- LongpWbck outputs
  // ════════════════════════════════════════════════════════════════════
  logic longp_wbck_valid;
  logic [31:0] longp_wbck_wdat;
  logic [4:0] longp_wbck_rdidx;
  logic [4:0] longp_wbck_flags;
  logic longp_wbck_rdfpu;
  logic longp_lsu_ready;
  logic longp_nice_ready;
  // LongpWbck exception outputs
  logic longp_excp_valid;
  logic longp_excp_insterr;
  logic longp_excp_ld;
  logic longp_excp_st;
  logic longp_excp_buserr;
  logic [31:0] longp_excp_badaddr;
  logic [31:0] longp_excp_pc;
  logic longp_excp_ready_from_commit;
  // ════════════════════════════════════════════════════════════════════
  // Internal wires -- Wbck outputs
  // ════════════════════════════════════════════════════════════════════
  logic wbck_alu_ready;
  logic wbck_longp_ready;
  logic wbck_rf_ena;
  logic [31:0] wbck_rf_wdat;
  logic [4:0] wbck_rf_rdidx;
  // ════════════════════════════════════════════════════════════════════
  // Internal wires -- Commit outputs
  // ════════════════════════════════════════════════════════════════════
  logic commit_alu_ready;
  logic commit_mret_w;
  logic commit_trap_w;
  logic core_wfi_w;
  logic nonflush_cmt_ena_w;
  logic excp_active_w;
  logic wfi_halt_ifu_req_w;
  logic wfi_halt_exu_req_w;
  logic [31:0] cmt_badaddr_w;
  logic cmt_badaddr_ena_w;
  logic [31:0] cmt_epc_w;
  logic cmt_epc_ena_w;
  logic [31:0] cmt_cause_w;
  logic cmt_cause_ena_w;
  logic cmt_instret_ena_w;
  logic cmt_status_ena_w;
  logic [31:0] cmt_dpc_w;
  logic cmt_dpc_ena_w;
  logic [2:0] cmt_dcause_w;
  logic cmt_dcause_ena_w;
  logic cmt_mret_ena_w;
  logic flush_pulse_w;
  logic flush_req_w;
  logic pipe_flush_req_w;
  logic [31:0] pipe_flush_add_op1_w;
  logic [31:0] pipe_flush_add_op2_w;
  logic [31:0] pipe_flush_pc_w;
  // ════════════════════════════════════════════════════════════════════
  // Internal wires -- CSR outputs
  // ════════════════════════════════════════════════════════════════════
  logic [31:0] csr_rdata;
  logic [31:0] csr_mtvec_val;
  logic [31:0] csr_mepc_val;
  logic [31:0] csr_dpc_val;
  logic csr_access_ilgl_w;
  logic csr_nice_xs_off;
  logic csr_tm_stop;
  logic csr_core_cgstop;
  logic csr_tcm_cgstop;
  logic csr_itcm_nohold;
  logic csr_mdv_nob2b;
  logic csr_status_mie;
  logic csr_mtie;
  logic csr_msie;
  logic csr_meie;
  logic csr_wr_dcsr_ena;
  logic csr_wr_dpc_ena;
  logic csr_wr_dscratch_ena;
  logic [31:0] csr_wr_csr_nxt;
  logic csr_u_mode;
  logic csr_s_mode;
  logic csr_h_mode;
  logic csr_m_mode;
  // ════════════════════════════════════════════════════════════════════
  // Internal wires -- Regfile
  // ════════════════════════════════════════════════════════════════════
  logic [31:0] rf_x1_r;
  // ════════════════════════════════════════════════════════════════════
  // Internal wires -- Glue
  // ════════════════════════════════════════════════════════════════════
  logic disp_valid_gated;
  logic oitf_dis_ena;
  logic oitf_ret_ena;
  // ════════════════════════════════════════════════════════════════════
  // 1. Decode
  // ════════════════════════════════════════════════════════════════════
  e203_exu_decode dec (
    .i_instr(i_ir),
    .i_pc(i_pc),
    .i_prdt_taken(i_prdt_taken),
    .i_misalgn(i_misalgn),
    .i_buserr(i_buserr),
    .i_muldiv_b2b(i_muldiv_b2b),
    .dbg_mode(dbg_mode),
    .nice_xs_off(0),
    .dec_rs1idx(dec_rs1_idx),
    .dec_rs2idx(dec_rs2_idx),
    .dec_rdidx(dec_rd_idx),
    .dec_imm(dec_imm),
    .dec_bjp(dec_bjp),
    .dec_rs1en(dec_rs1_en),
    .dec_rs2en(dec_rs2_en),
    .dec_rdwen(dec_rd_en),
    .dec_mul(dec_mul),
    .dec_mulhsu(dec_mulhsu),
    .dec_div(dec_div),
    .dec_divu(dec_divu),
    .dec_rem(dec_rem),
    .dec_remu(dec_remu),
    .dec_rs1x0(dec_rs1x0),
    .dec_rs2x0(dec_rs2x0),
    .dec_info(dec_info),
    .dec_pc(dec_pc_out),
    .dec_misalgn(dec_misalgn),
    .dec_buserr(dec_buserr_out),
    .dec_ilegl(dec_ilegl),
    .dec_nice(dec_nice),
    .nice_cmt_off_ilgl_o(dec_nice_cmt_off_ilgl),
    .dec_rv32(dec_rv32),
    .dec_jal(dec_jal),
    .dec_jalr(dec_jalr),
    .dec_bxx(dec_bxx),
    .dec_jalr_rs1idx(dec_jalr_rs1idx),
    .dec_bjp_imm(dec_bjp_imm),
    .o_alu(dec_alu),
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
    .o_mulh(dec_mulh),
    .o_mulhu(dec_mulhu),
    .o_load(dec_load),
    .o_store(dec_store)
  );
  // Inputs
  // Reference-design outputs
  // Simplified control outputs
  // ════════════════════════════════════════════════════════════════════
  // 2. Dispatch
  // ════════════════════════════════════════════════════════════════════
  e203_exu_disp disp (
    .clk(clk),
    .rst_n(rst_n),
    .wfi_halt_exu_req(wfi_halt_exu_req_w),
    .wfi_halt_exu_ack(disp_wfi_halt_exu_ack),
    .oitf_empty(oitf_is_empty),
    .amo_wait(amo_wait_w),
    .disp_i_valid(disp_valid_gated),
    .disp_i_ready(disp_rdy),
    .disp_i_rs1x0(dec_rs1x0),
    .disp_i_rs2x0(dec_rs2x0),
    .disp_i_rs1en(dec_rs1_en),
    .disp_i_rs2en(dec_rs2_en),
    .disp_i_rs1idx(i_rs1idx),
    .disp_i_rs2idx(i_rs2idx),
    .disp_i_rs1(rf_rs1_data),
    .disp_i_rs2(rf_rs2_data),
    .disp_i_rdwen(dec_rd_en),
    .disp_i_rdidx(dec_rd_idx),
    .disp_i_info(dec_info),
    .disp_i_imm(dec_imm),
    .disp_i_pc(dec_pc_out),
    .disp_i_misalgn(dec_misalgn),
    .disp_i_buserr(dec_buserr_out),
    .disp_i_ilegl(dec_ilegl),
    .disp_o_alu_valid(disp_alu_valid),
    .disp_o_alu_ready(alu_o_ready),
    .disp_o_alu_longpipe(alu_longpipe),
    .disp_o_alu_rs1(disp_alu_rs1),
    .disp_o_alu_rs2(disp_alu_rs2),
    .disp_o_alu_rdwen(disp_alu_rdwen),
    .disp_o_alu_rdidx(disp_alu_rdidx),
    .disp_o_alu_info(disp_alu_info),
    .disp_o_alu_imm(disp_alu_imm),
    .disp_o_alu_pc(disp_alu_pc),
    .disp_o_alu_itag(disp_alu_itag),
    .disp_o_alu_misalgn(disp_alu_misalgn),
    .disp_o_alu_buserr(disp_alu_buserr),
    .disp_o_alu_ilegl(disp_alu_ilegl),
    .oitfrd_match_disprs1(oitf_match_rs1),
    .oitfrd_match_disprs2(oitf_match_rs2),
    .oitfrd_match_disprs3(oitf_match_rs3),
    .oitfrd_match_disprd(oitf_match_rd),
    .disp_oitf_ptr(oitf_dis_ptr),
    .disp_oitf_ena(oitf_dis_ena),
    .disp_oitf_ready(oitf_dis_ready),
    .disp_oitf_rs1fpu(disp_oitf_rs1fpu_w),
    .disp_oitf_rs2fpu(disp_oitf_rs2fpu_w),
    .disp_oitf_rs3fpu(disp_oitf_rs3fpu_w),
    .disp_oitf_rdfpu(disp_oitf_rdfpu_w),
    .disp_oitf_rs1en(disp_oitf_rs1en_w),
    .disp_oitf_rs2en(disp_oitf_rs2en_w),
    .disp_oitf_rs3en(disp_oitf_rs3en_w),
    .disp_oitf_rdwen(disp_oitf_rdwen_w),
    .disp_oitf_rs1idx(disp_oitf_rs1idx_w),
    .disp_oitf_rs2idx(disp_oitf_rs2idx_w),
    .disp_oitf_rs3idx(disp_oitf_rs3idx_w),
    .disp_oitf_rdidx(disp_oitf_rdidx_w),
    .disp_oitf_pc(disp_oitf_pc_w)
  );
  // WFI halt interface
  // OITF status
  // Dispatch input (from decode)
  // ALU dispatch output
  // OITF hazard check inputs
  // OITF dispatch interface
  // ════════════════════════════════════════════════════════════════════
  // 3. OITF (Outstanding Instruction Track FIFO)
  // ════════════════════════════════════════════════════════════════════
  e203_exu_oitf oitf_u (
    .clk(clk),
    .rst_n(rst_n),
    .dis_ready(oitf_dis_ready),
    .dis_ena(oitf_dis_ena),
    .ret_ena(oitf_ret_ena),
    .dis_ptr(oitf_dis_ptr),
    .ret_ptr(oitf_ret_ptr),
    .ret_rdidx(oitf_ret_rdidx),
    .ret_rdwen(oitf_ret_rdwen),
    .ret_rdfpu(oitf_ret_rdfpu),
    .ret_pc(oitf_ret_pc),
    .disp_i_rs1en(disp_oitf_rs1en_w),
    .disp_i_rs2en(disp_oitf_rs2en_w),
    .disp_i_rs3en(disp_oitf_rs3en_w),
    .disp_i_rdwen(disp_oitf_rdwen_w),
    .disp_i_rs1fpu(disp_oitf_rs1fpu_w),
    .disp_i_rs2fpu(disp_oitf_rs2fpu_w),
    .disp_i_rs3fpu(disp_oitf_rs3fpu_w),
    .disp_i_rdfpu(disp_oitf_rdfpu_w),
    .disp_i_rs1idx(disp_oitf_rs1idx_w),
    .disp_i_rs2idx(disp_oitf_rs2idx_w),
    .disp_i_rs3idx(disp_oitf_rs3idx_w),
    .disp_i_rdidx(disp_oitf_rdidx_w),
    .disp_i_pc(disp_oitf_pc_w),
    .oitfrd_match_disprs1(oitf_match_rs1),
    .oitfrd_match_disprs2(oitf_match_rs2),
    .oitfrd_match_disprs3(oitf_match_rs3),
    .oitfrd_match_disprd(oitf_match_rd),
    .oitf_empty(oitf_is_empty)
  );
  // ════════════════════════════════════════════════════════════════════
  // 4. ALU execution
  // ════════════════════════════════════════════════════════════════════
  e203_exu_alu alu_u (
    .clk(clk),
    .rst_n(rst_n),
    .i_valid(disp_alu_valid),
    .i_ready(alu_o_ready),
    .i_longpipe(alu_longpipe),
    .i_rs1(disp_alu_rs1),
    .i_rs2(disp_alu_rs2),
    .i_imm(disp_alu_imm),
    .i_info(disp_alu_info),
    .i_pc(disp_alu_pc),
    .i_instr(i_ir),
    .i_pc_vld(i_pc_vld),
    .i_rdidx(disp_alu_rdidx),
    .i_rdwen(disp_alu_rdwen),
    .i_itag(disp_alu_itag),
    .i_ilegl(disp_alu_ilegl),
    .i_buserr(disp_alu_buserr),
    .i_misalgn(disp_alu_misalgn),
    .nice_xs_off(csr_nice_xs_off),
    .amo_wait(amo_wait_w),
    .oitf_empty(oitf_empty_w),
    .flush_req(flush_req_w),
    .flush_pulse(pipe_flush_pulse_w),
    .mdv_nob2b(csr_mdv_nob2b),
    .i_nice_cmt_off_ilgl(1'b0),
    .cmt_o_valid(alu_cmt_valid),
    .cmt_o_ready(commit_alu_ready),
    .cmt_o_pc_vld(alu_cmt_pc_vld),
    .cmt_o_pc(alu_cmt_pc),
    .cmt_o_instr(alu_cmt_instr),
    .cmt_o_imm(alu_cmt_imm),
    .cmt_o_rv32(alu_cmt_rv32),
    .cmt_o_bjp(alu_cmt_bjp),
    .cmt_o_mret(alu_cmt_mret),
    .cmt_o_dret(alu_cmt_dret),
    .cmt_o_ecall(alu_cmt_ecall),
    .cmt_o_ebreak(alu_cmt_ebreak),
    .cmt_o_fencei(alu_cmt_fencei),
    .cmt_o_wfi(alu_cmt_wfi),
    .cmt_o_ifu_misalgn(alu_cmt_ifu_misalgn),
    .cmt_o_ifu_buserr(alu_cmt_ifu_buserr),
    .cmt_o_ifu_ilegl(alu_cmt_ifu_ilegl),
    .cmt_o_bjp_prdt(alu_cmt_bjp_prdt),
    .cmt_o_bjp_rslv(alu_cmt_bjp_rslv),
    .cmt_o_misalgn(alu_cmt_misalgn),
    .cmt_o_ld(alu_cmt_ld),
    .cmt_o_stamo(alu_cmt_stamo),
    .cmt_o_buserr(alu_cmt_buserr),
    .cmt_o_badaddr(alu_cmt_badaddr),
    .wbck_o_valid(alu_wbck_valid),
    .wbck_o_ready(wbck_alu_ready),
    .wbck_o_wdat(alu_wdat),
    .wbck_o_rdidx(alu_rdidx),
    .csr_ena(alu_csr_ena),
    .csr_wr_en(alu_csr_wr_en),
    .csr_rd_en(alu_csr_rd_en),
    .csr_idx(alu_csr_idx),
    .nonflush_cmt_ena(nonflush_cmt_ena_w),
    .csr_access_ilgl(csr_access_ilgl_w),
    .read_csr_dat(read_csr_dat_w),
    .wbck_csr_dat(alu_wbck_csr_dat),
    .agu_icb_cmd_valid(agu_icb_cmd_valid),
    .agu_icb_cmd_ready(agu_icb_cmd_ready),
    .agu_icb_cmd_addr(agu_icb_cmd_addr),
    .agu_icb_cmd_read(agu_icb_cmd_read),
    .agu_icb_cmd_wdata(agu_icb_cmd_wdata),
    .agu_icb_cmd_wmask(agu_icb_cmd_wmask),
    .agu_icb_cmd_lock(agu_icb_cmd_lock),
    .agu_icb_cmd_excl(agu_icb_cmd_excl),
    .agu_icb_cmd_size(agu_icb_cmd_size),
    .agu_icb_cmd_back2agu(agu_icb_cmd_back2agu),
    .agu_icb_cmd_usign(agu_icb_cmd_usign),
    .agu_icb_cmd_itag(agu_icb_cmd_itag),
    .agu_icb_rsp_valid(agu_icb_rsp_valid),
    .agu_icb_rsp_ready(agu_icb_rsp_ready),
    .agu_icb_rsp_err(agu_icb_rsp_err),
    .agu_icb_rsp_excl_ok(agu_icb_rsp_excl_ok),
    .agu_icb_rsp_rdata(agu_icb_rsp_rdata),
    .nice_req_valid(nice_req_valid),
    .nice_req_ready(nice_req_ready),
    .nice_req_instr(nice_req_inst),
    .nice_req_rs1(nice_req_rs1),
    .nice_req_rs2(nice_req_rs2),
    .nice_rsp_multicyc_valid(nice_rsp_multicyc_valid),
    .nice_rsp_multicyc_ready(nice_rsp_multicyc_ready),
    .nice_longp_wbck_valid(alu_nice_longp_wbck_valid),
    .nice_longp_wbck_ready(alu_nice_longp_wbck_ready),
    .nice_o_itag(alu_nice_o_itag)
  );
  // Dispatch inputs
  // Control
  // Commit outputs
  // Writeback outputs
  // CSR interface
  // AGU ICB
  // NICE
  // ════════════════════════════════════════════════════════════════════
  // 5. Long-pipe writeback collector (LSU + MulDiv -> single port)
  // ════════════════════════════════════════════════════════════════════
  e203_exu_longpwbck longp_u (
    .clk(clk),
    .rst_n(rst_n),
    .lsu_wbck_i_valid(lsu_o_valid),
    .lsu_wbck_i_ready(longp_lsu_ready),
    .lsu_wbck_i_wdat(lsu_o_wbck_wdat),
    .lsu_wbck_i_itag(1'(lsu_o_wbck_itag)),
    .lsu_wbck_i_err(lsu_o_wbck_err),
    .lsu_cmt_i_buserr(lsu_o_cmt_buserr),
    .lsu_cmt_i_badaddr(lsu_o_cmt_badaddr),
    .lsu_cmt_i_ld(lsu_o_cmt_ld),
    .lsu_cmt_i_st(lsu_o_cmt_st),
    .longp_wbck_o_valid(longp_wbck_valid),
    .longp_wbck_o_ready(wbck_longp_ready),
    .longp_wbck_o_wdat(longp_wbck_wdat),
    .longp_wbck_o_flags(longp_wbck_flags),
    .longp_wbck_o_rdidx(longp_wbck_rdidx),
    .longp_wbck_o_rdfpu(longp_wbck_rdfpu),
    .longp_excp_o_valid(longp_excp_valid),
    .longp_excp_o_ready(longp_excp_ready_from_commit),
    .longp_excp_o_insterr(longp_excp_insterr),
    .longp_excp_o_ld(longp_excp_ld),
    .longp_excp_o_st(longp_excp_st),
    .longp_excp_o_buserr(longp_excp_buserr),
    .longp_excp_o_badaddr(longp_excp_badaddr),
    .longp_excp_o_pc(longp_excp_pc),
    .oitf_empty(oitf_is_empty),
    .oitf_ret_ptr(oitf_ret_ptr),
    .oitf_ret_rdidx(oitf_ret_rdidx),
    .oitf_ret_pc(oitf_ret_pc),
    .oitf_ret_rdwen(oitf_ret_rdwen),
    .oitf_ret_rdfpu(oitf_ret_rdfpu),
    .oitf_ret_ena(oitf_ret_ena),
    .nice_longp_wbck_i_valid(1'b0),
    .nice_longp_wbck_i_ready(longp_nice_ready),
    .nice_longp_wbck_i_wdat(0),
    .nice_longp_wbck_i_itag(0),
    .nice_longp_wbck_i_err(1'b0)
  );
  // LSU writeback input
  // LSU commit info
  // Merged writeback output
  // Exception output
  // OITF interface
  // NICE writeback input (stub)
  // ════════════════════════════════════════════════════════════════════
  // 6. Writeback arbiter (ALU vs long-pipe -> regfile)
  // ════════════════════════════════════════════════════════════════════
  e203_exu_wbck wbck_u (
    .clk(clk),
    .rst_n(rst_n),
    .alu_wbck_i_valid(alu_done_valid),
    .alu_wbck_i_ready(wbck_alu_ready),
    .alu_wbck_i_wdat(alu_wdat),
    .alu_wbck_i_rdidx(alu_rdidx),
    .longp_wbck_i_valid(longp_wbck_valid),
    .longp_wbck_i_ready(wbck_longp_ready),
    .longp_wbck_i_wdat(longp_wbck_wdat),
    .longp_wbck_i_flags(0),
    .longp_wbck_i_rdidx(longp_wbck_rdidx),
    .longp_wbck_i_rdfpu(1'b0),
    .rf_wbck_o_ena(wbck_rf_ena),
    .rf_wbck_o_wdat(wbck_rf_wdat),
    .rf_wbck_o_rdidx(wbck_rf_rdidx)
  );
  // ════════════════════════════════════════════════════════════════════
  // 7. Commit stage
  // ════════════════════════════════════════════════════════════════════
  e203_exu_commit commit_u (
    .clk(clk),
    .rst_n(rst_n),
    .commit_mret(commit_mret_w),
    .commit_trap(commit_trap_w),
    .core_wfi(core_wfi_w),
    .nonflush_cmt_ena(nonflush_cmt_ena_w),
    .excp_active(excp_active_w),
    .amo_wait(amo_wait_w),
    .wfi_halt_ifu_req(wfi_halt_ifu_req_w),
    .wfi_halt_exu_req(wfi_halt_exu_req_w),
    .wfi_halt_ifu_ack(wfi_halt_ifu_ack),
    .wfi_halt_exu_ack(disp_wfi_halt_exu_ack),
    .dbg_irq_r(dbg_irq_r),
    .lcl_irq_r(lcl_irq_r),
    .ext_irq_r(ext_irq_r),
    .sft_irq_r(sft_irq_r),
    .tmr_irq_r(tmr_irq_r),
    .evt_r(evt_r),
    .status_mie_r(csr_status_mie),
    .mtie_r(csr_mtie),
    .msie_r(csr_msie),
    .meie_r(csr_meie),
    .alu_cmt_i_valid(alu_cmt_valid),
    .alu_cmt_i_ready(commit_alu_ready),
    .alu_cmt_i_pc(alu_cmt_pc),
    .alu_cmt_i_instr(alu_cmt_instr),
    .alu_cmt_i_pc_vld(alu_cmt_pc_vld),
    .alu_cmt_i_imm(alu_cmt_imm),
    .alu_cmt_i_rv32(alu_cmt_rv32),
    .alu_cmt_i_bjp(alu_cmt_bjp),
    .alu_cmt_i_wfi(alu_cmt_wfi),
    .alu_cmt_i_fencei(alu_cmt_fencei),
    .alu_cmt_i_mret(alu_cmt_mret),
    .alu_cmt_i_dret(alu_cmt_dret),
    .alu_cmt_i_ecall(alu_cmt_ecall),
    .alu_cmt_i_ebreak(alu_cmt_ebreak),
    .alu_cmt_i_ifu_misalgn(alu_cmt_ifu_misalgn),
    .alu_cmt_i_ifu_buserr(alu_cmt_ifu_buserr),
    .alu_cmt_i_ifu_ilegl(alu_cmt_ifu_ilegl),
    .alu_cmt_i_bjp_prdt(alu_cmt_bjp_prdt),
    .alu_cmt_i_bjp_rslv(alu_cmt_bjp_rslv),
    .alu_cmt_i_misalgn(alu_cmt_misalgn),
    .alu_cmt_i_ld(alu_cmt_ld),
    .alu_cmt_i_stamo(alu_cmt_stamo),
    .alu_cmt_i_buserr(alu_cmt_buserr),
    .alu_cmt_i_badaddr(alu_cmt_badaddr),
    .cmt_badaddr(cmt_badaddr_w),
    .cmt_badaddr_ena(cmt_badaddr_ena_w),
    .cmt_epc(cmt_epc_w),
    .cmt_epc_ena(cmt_epc_ena_w),
    .cmt_cause(cmt_cause_w),
    .cmt_cause_ena(cmt_cause_ena_w),
    .cmt_instret_ena(cmt_instret_ena_w),
    .cmt_status_ena(cmt_status_ena_w),
    .cmt_dpc(cmt_dpc_w),
    .cmt_dpc_ena(cmt_dpc_ena_w),
    .cmt_dcause(cmt_dcause_w),
    .cmt_dcause_ena(cmt_dcause_ena_w),
    .cmt_mret_ena(cmt_mret_ena_w),
    .csr_epc_r(csr_mepc_val),
    .csr_dpc_r(csr_dpc_val),
    .csr_mtvec_r(csr_mtvec_val),
    .dbg_mode(dbg_mode),
    .dbg_halt_r(dbg_halt_r),
    .dbg_step_r(dbg_step_r),
    .dbg_ebreakm_r(dbg_ebreakm_r),
    .oitf_empty(oitf_is_empty),
    .u_mode(csr_u_mode),
    .s_mode(csr_s_mode),
    .h_mode(csr_h_mode),
    .m_mode(csr_m_mode),
    .longp_excp_i_ready(longp_excp_ready_from_commit),
    .longp_excp_i_valid(longp_excp_valid),
    .longp_excp_i_ld(longp_excp_ld),
    .longp_excp_i_st(longp_excp_st),
    .longp_excp_i_buserr(longp_excp_buserr),
    .longp_excp_i_badaddr(longp_excp_badaddr),
    .longp_excp_i_insterr(longp_excp_insterr),
    .longp_excp_i_pc(longp_excp_pc),
    .flush_pulse(flush_pulse_w),
    .flush_req(flush_req_w),
    .pipe_flush_ack(pipe_flush_ack),
    .pipe_flush_req(pipe_flush_req_w),
    .pipe_flush_add_op1(pipe_flush_add_op1_w),
    .pipe_flush_add_op2(pipe_flush_add_op2_w),
    .pipe_flush_pc(pipe_flush_pc_w)
  );
  // Commit status outputs
  // AMO wait
  // WFI halt interface
  // Interrupt inputs
  // ALU commit input channel (wired from ALU cmt_o_* outputs)
  // CSR commit outputs
  // CSR read inputs
  // Debug mode inputs
  // Privilege mode inputs
  // Long-pipe exception input
  // Flush outputs
  // ════════════════════════════════════════════════════════════════════
  // 8. CSR register file
  // ════════════════════════════════════════════════════════════════════
  e203_exu_csr csr_u (
    .clk(clk),
    .rst_n(rst_n),
    .clk_aon(clk_aon),
    .nonflush_cmt_ena(nonflush_cmt_ena_w),
    .csr_ena(alu_csr_ena),
    .csr_wr_en(alu_csr_wr_en),
    .csr_rd_en(alu_csr_rd_en),
    .csr_idx(alu_csr_idx),
    .csr_access_ilgl(csr_access_ilgl_w),
    .read_csr_dat(csr_rdata),
    .wbck_csr_dat(alu_wbck_csr_dat),
    .nice_xs_off(csr_nice_xs_off),
    .tm_stop(csr_tm_stop),
    .core_cgstop(csr_core_cgstop),
    .tcm_cgstop(csr_tcm_cgstop),
    .itcm_nohold(csr_itcm_nohold),
    .mdv_nob2b(csr_mdv_nob2b),
    .core_mhartid(core_mhartid[0:0]),
    .ext_irq_r(ext_irq_r),
    .sft_irq_r(sft_irq_r),
    .tmr_irq_r(tmr_irq_r),
    .status_mie_r(csr_status_mie),
    .mtie_r(csr_mtie),
    .msie_r(csr_msie),
    .meie_r(csr_meie),
    .wr_dcsr_ena(csr_wr_dcsr_ena),
    .wr_dpc_ena(csr_wr_dpc_ena),
    .wr_dscratch_ena(csr_wr_dscratch_ena),
    .dcsr_r(dcsr_r),
    .dpc_r(dpc_r),
    .dscratch_r(dscratch_r),
    .wr_csr_nxt(csr_wr_csr_nxt),
    .dbg_mode(dbg_mode),
    .dbg_stopcycle(dbg_stopcycle),
    .u_mode(csr_u_mode),
    .s_mode(csr_s_mode),
    .h_mode(csr_h_mode),
    .m_mode(csr_m_mode),
    .cmt_badaddr(cmt_badaddr_w),
    .cmt_badaddr_ena(cmt_badaddr_ena_w),
    .cmt_epc(cmt_epc_w),
    .cmt_epc_ena(cmt_epc_ena_w),
    .cmt_cause(cmt_cause_w),
    .cmt_cause_ena(cmt_cause_ena_w),
    .cmt_status_ena(cmt_status_ena_w),
    .cmt_instret_ena(cmt_instret_ena_w),
    .cmt_mret_ena(cmt_mret_ena_w),
    .csr_epc_r(csr_mepc_val),
    .csr_dpc_r(csr_dpc_val),
    .csr_mtvec_r(csr_mtvec_val)
  );
  // CSR access interface
  // Control outputs
  // Hart ID
  // Interrupt status
  // Debug CSR interface
  // Debug mode
  // Privilege mode outputs
  // Commit inputs
  // CSR vector outputs
  // ════════════════════════════════════════════════════════════════════
  // 9. Integer register file (2R1W)
  // ════════════════════════════════════════════════════════════════════
  e203_exu_regfile rf_u (
    .clk(clk),
    .rst_n(rst_n),
    .test_mode(test_mode),
    .read_src1_idx(dec_rs1_idx),
    .read_src1_dat(rf_rs1_data),
    .read_src2_idx(dec_rs2_idx),
    .read_src2_dat(rf_rs2_data),
    .wbck_dest_wen(wbck_rf_ena),
    .wbck_dest_idx(wbck_rf_rdidx),
    .wbck_dest_dat(wbck_rf_wdat),
    .x1_r(rf_x1_r)
  );
  // ════════════════════════════════════════════════════════════════════
  // Glue logic
  // ════════════════════════════════════════════════════════════════════
  assign disp_valid_gated = i_valid;
  assign i_ready = disp_rdy;
  assign pipe_flush_req = pipe_flush_req_w;
  assign pipe_flush_add_op1 = pipe_flush_add_op1_w;
  assign pipe_flush_add_op2 = pipe_flush_add_op2_w;
  assign pipe_flush_pc = pipe_flush_pc_w;
  assign lsu_o_ready = longp_lsu_ready;
  assign oitf_empty = oitf_is_empty;
  assign rf2ifu_x1 = rf_x1_r;
  assign rf2ifu_rs1 = rf_rs1_data;
  assign dec2ifu_rden = dec_rd_en & ~disp_oitf_rdfpu_w;
  assign dec2ifu_rs1en = dec_rs1_en & ~disp_oitf_rs1fpu_w;
  assign dec2ifu_rdidx = dec_rd_idx;
  assign dec2ifu_mulhsu = dec_mulhsu;
  assign dec2ifu_div = dec_div;
  assign dec2ifu_rem = dec_rem;
  assign dec2ifu_divu = dec_divu;
  assign dec2ifu_remu = dec_remu;
  assign exu_active = ~oitf_is_empty | i_valid | excp_active_w;
  assign excp_active = excp_active_w;
  assign commit_mret = commit_mret_w;
  assign commit_trap = commit_trap_w;
  assign core_wfi = core_wfi_w;
  assign wfi_halt_ifu_req = wfi_halt_ifu_req_w;
  assign tm_stop = csr_tm_stop;
  assign itcm_nohold = csr_itcm_nohold;
  assign core_cgstop = csr_core_cgstop;
  assign tcm_cgstop = csr_tcm_cgstop;
  assign cmt_dpc = cmt_dpc_w;
  assign cmt_dpc_ena = cmt_dpc_ena_w;
  assign cmt_dcause = cmt_dcause_w;
  assign cmt_dcause_ena = cmt_dcause_ena_w;
  assign wr_dcsr_ena = csr_wr_dcsr_ena;
  assign wr_dpc_ena = csr_wr_dpc_ena;
  assign wr_dscratch_ena = csr_wr_dscratch_ena;
  assign wr_csr_nxt = csr_wr_csr_nxt;
  assign alu_done_valid = alu_wbck_valid;
  assign oitf_empty_w = oitf_is_empty;
  assign pipe_flush_pulse_w = flush_pulse_w;
  assign read_csr_dat_w = csr_rdata;

endmodule

// Gate dispatch valid
// Pipe flush: from commit unit
// LSU writeback ready
// OITF empty status
// Regfile to IFU
// Decode to IFU feedback
// Status outputs from commit
// CSR control outputs
// CSR debug outputs
// Glue: ALU outputs to wbck input
// Glue: misc wires
