// E203 HBirdv2 Instruction Decode Unit
// Pure combinational RV32IMC decoder matching the RealBench port interface.
// Decodes instruction into register indices, immediate, info bus, and
// control flags. Passes through PC, misalign, and bus error from IFU.
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
  // ── Simplified control outputs for e203_exu integration ─────────────
  // ── Instruction field extraction ────────────────────────────────────
  logic [6:0] opcode;
  assign opcode = i_instr[6:0];
  logic [4:0] rd_field;
  assign rd_field = i_instr[11:7];
  logic [2:0] funct3;
  assign funct3 = i_instr[14:12];
  logic [4:0] rs1_field;
  assign rs1_field = i_instr[19:15];
  logic [4:0] rs2_field;
  assign rs2_field = i_instr[24:20];
  logic [6:0] funct7;
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
  assign is_ecall = is_system & (i_instr[31:20] == 'h0);
  logic is_ebreak;
  assign is_ebreak = is_system & (i_instr[31:20] == 'h1);
  logic is_mret;
  assign is_mret = is_system & (i_instr[31:20] == 'h302);
  logic is_wfi;
  assign is_wfi = is_system & (i_instr[31:20] == 'h105);
  logic is_csr;
  assign is_csr = is_system & (funct3 != 0);
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
  logic [31:0] imm_i;
  assign imm_i = 32'($unsigned({{(32-$bits(i_instr[31:20])){i_instr[31:20][$bits(i_instr[31:20])-1]}}, i_instr[31:20]}));
  // S-type
  logic [11:0] imm_s_hi;
  assign imm_s_hi = 12'($unsigned(funct7)) << 5;
  logic [11:0] imm_s_raw;
  assign imm_s_raw = imm_s_hi | 12'($unsigned(rd_field));
  logic [31:0] imm_s;
  assign imm_s = 32'($unsigned({{(32-$bits(imm_s_raw)){imm_s_raw[$bits(imm_s_raw)-1]}}, imm_s_raw}));
  // B-type
  logic [31:0] imm_b_12;
  assign imm_b_12 = 32'($unsigned(i_instr[31:31])) << 12;
  logic [31:0] imm_b_11;
  assign imm_b_11 = 32'($unsigned(i_instr[7:7])) << 11;
  logic [31:0] imm_b_10_5;
  assign imm_b_10_5 = 32'($unsigned(i_instr[30:25])) << 5;
  logic [31:0] imm_b_4_1;
  assign imm_b_4_1 = 32'($unsigned(i_instr[11:8])) << 1;
  logic [12:0] imm_b_raw;
  assign imm_b_raw = 13'(imm_b_12 | imm_b_11 | imm_b_10_5 | imm_b_4_1);
  logic [31:0] imm_b;
  assign imm_b = 32'($unsigned({{(32-$bits(imm_b_raw)){imm_b_raw[$bits(imm_b_raw)-1]}}, imm_b_raw}));
  // U-type
  logic [31:0] imm_u;
  assign imm_u = 32'($unsigned(20'(i_instr >> 12))) << 12;
  // J-type
  logic [31:0] imm_j_20;
  assign imm_j_20 = 32'($unsigned(i_instr[31:31])) << 20;
  logic [31:0] imm_j_19_12;
  assign imm_j_19_12 = 32'($unsigned(i_instr[19:12])) << 12;
  logic [31:0] imm_j_11;
  assign imm_j_11 = 32'($unsigned(i_instr[20:20])) << 11;
  logic [31:0] imm_j_10_1;
  assign imm_j_10_1 = 32'($unsigned(i_instr[30:21])) << 1;
  logic [20:0] imm_j_raw;
  assign imm_j_raw = 21'(imm_j_20 | imm_j_19_12 | imm_j_11 | imm_j_10_1);
  logic [31:0] imm_j;
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
    o_alu_add = (is_op & f3_add & ~f7_sub) | (is_op_imm & f3_add) | is_auipc;
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
  // ── Hazard detection ──────────────────────────────────────────────
  // Reference does NOT gate on rs1en/rs2en — matches any rs-field hit
  logic raw_dep;
  assign raw_dep = oitfrd_match_disprs1 | oitfrd_match_disprs2 | oitfrd_match_disprs3;
  // Reference matches any rd-field hit regardless of rdwen
  logic waw_dep;
  assign waw_dep = oitfrd_match_disprd;
  logic dep;
  assign dep = raw_dep | waw_dep;
  // Instruction group from info bus (bits [2:0])
  logic [2:0] disp_i_info_grp;
  assign disp_i_info_grp = disp_i_info[2:0];
  // CSR group = 3; FENCE/FENCEI in BJP group (2) with specific bits
  logic disp_csr;
  assign disp_csr = disp_i_info_grp == 3;
  logic disp_fence_fencei;
  assign disp_fence_fencei = (disp_i_info_grp == 2) & (disp_i_info[14:14] | disp_i_info[15:15]);
  // Long-pipe prediction: AGU group (1)
  logic disp_alu_longp_prdt;
  assign disp_alu_longp_prdt = disp_i_info_grp == 1;
  // Dispatch condition matches reference exactly
  logic disp_condition;
  assign disp_condition = (disp_csr ? oitf_empty : 1'b1) & (disp_fence_fencei ? oitf_empty : 1'b1) & ~wfi_halt_exu_req & ~dep & (disp_alu_longp_prdt ? disp_oitf_ready : 1'b1);
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
// Orchestrates the ALU datapath, branch/jump unit, AGU, CSR access, and NICE
// co-processor interface. Receives dispatched instructions via i_info encoding,
// routes operands to sub-units, and presents results to write-back and commit paths.
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
  // ── Decode i_info fields (E203 encoding) ─────────────────────────────────
  // i_info bit assignments (simplified E203 convention):
  //   [0]  = ALU op
  //   [1]  = BJP op
  //   [2]  = CSR op
  //   [3]  = AGU (load/store/AMO)
  //   [4]  = NICE op
  //   [5]  = mret
  //   [6]  = dret
  //   [7]  = ecall
  //   [8]  = ebreak
  //   [9]  = fencei
  //   [10] = wfi
  //   [11] = rv32 flag
  //   [12] = bjp_prdt
  //   [13..31] = sub-op encoding
  logic is_alu;
  assign is_alu = i_info[0:0] != 0;
  logic is_bjp;
  assign is_bjp = i_info[1:1] != 0;
  logic is_csr;
  assign is_csr = i_info[2:2] != 0;
  logic is_agu;
  assign is_agu = i_info[3:3] != 0;
  logic is_nice;
  assign is_nice = i_info[4:4] != 0;
  logic is_mret;
  assign is_mret = i_info[5:5] != 0;
  logic is_dret;
  assign is_dret = i_info[6:6] != 0;
  logic is_ecall;
  assign is_ecall = i_info[7:7] != 0;
  logic is_ebreak;
  assign is_ebreak = i_info[8:8] != 0;
  logic is_fencei;
  assign is_fencei = i_info[9:9] != 0;
  logic is_wfi;
  assign is_wfi = i_info[10:10] != 0;
  logic is_rv32;
  assign is_rv32 = i_info[11:11] != 0;
  logic bjp_prdt;
  assign bjp_prdt = i_info[12:12] != 0;
  // ── ALU sub-operation decode from i_info[31:13] ──────────────────────────
  logic alu_add;
  assign alu_add = is_alu & (i_info[13:13] != 0);
  logic alu_sub;
  assign alu_sub = is_alu & (i_info[14:14] != 0);
  logic alu_xor;
  assign alu_xor = is_alu & (i_info[15:15] != 0);
  logic alu_sll;
  assign alu_sll = is_alu & (i_info[16:16] != 0);
  logic alu_srl;
  assign alu_srl = is_alu & (i_info[17:17] != 0);
  logic alu_sra;
  assign alu_sra = is_alu & (i_info[18:18] != 0);
  logic alu_or;
  assign alu_or = is_alu & (i_info[19:19] != 0);
  logic alu_and;
  assign alu_and = is_alu & (i_info[20:20] != 0);
  logic alu_slt;
  assign alu_slt = is_alu & (i_info[21:21] != 0);
  logic alu_sltu;
  assign alu_sltu = is_alu & (i_info[22:22] != 0);
  logic alu_lui;
  assign alu_lui = is_alu & (i_info[23:23] != 0);
  // ── Simple ALU result ────────────────────────────────────────────────────
  logic [31:0] alu_result;
  always_comb begin
    if (alu_add) begin
      alu_result = 32'(i_rs1 + i_rs2);
    end else if (alu_sub) begin
      alu_result = 32'(i_rs1 - i_rs2);
    end else if (alu_xor) begin
      alu_result = i_rs1 ^ i_rs2;
    end else if (alu_sll) begin
      alu_result = i_rs1 << 32'($unsigned(i_rs2[4:0]));
    end else if (alu_srl) begin
      alu_result = i_rs1 >> 32'($unsigned(i_rs2[4:0]));
    end else if (alu_sra) begin
      alu_result = $unsigned($signed(i_rs1) >>> 32'($unsigned(i_rs2[4:0])));
    end else if (alu_or) begin
      alu_result = i_rs1 | i_rs2;
    end else if (alu_and) begin
      alu_result = i_rs1 & i_rs2;
    end else if (alu_slt) begin
      alu_result = $signed(i_rs1) < $signed(i_rs2) ? 1 : 0;
    end else if (alu_sltu) begin
      alu_result = i_rs1 < i_rs2 ? 1 : 0;
    end else if (alu_lui) begin
      alu_result = i_imm;
    end else begin
      alu_result = 0;
    end
  end
  // ── BJP: branch comparison and target computation ────────────────────────
  logic [31:0] bjp_add_res;
  assign bjp_add_res = 32'(i_pc + i_imm);
  logic [31:0] bjp_link;
  assign bjp_link = 32'(i_pc + 4);
  logic cmp_eq;
  assign cmp_eq = i_rs1 == i_rs2;
  logic cmp_lt;
  assign cmp_lt = $signed(i_rs1) < $signed(i_rs2);
  logic cmp_ltu;
  assign cmp_ltu = i_rs1 < i_rs2;
  // BJP sub-op from i_info
  logic bjp_beq;
  assign bjp_beq = is_bjp & (i_info[13:13] != 0);
  logic bjp_bne;
  assign bjp_bne = is_bjp & (i_info[14:14] != 0);
  logic bjp_blt;
  assign bjp_blt = is_bjp & (i_info[15:15] != 0);
  logic bjp_bge;
  assign bjp_bge = is_bjp & (i_info[16:16] != 0);
  logic bjp_bltu;
  assign bjp_bltu = is_bjp & (i_info[17:17] != 0);
  logic bjp_bgeu;
  assign bjp_bgeu = is_bjp & (i_info[18:18] != 0);
  logic bjp_jump;
  assign bjp_jump = is_bjp & (i_info[19:19] != 0);
  logic bjp_taken;
  always_comb begin
    if (bjp_beq) begin
      bjp_taken = cmp_eq;
    end else if (bjp_bne) begin
      bjp_taken = ~cmp_eq;
    end else if (bjp_blt) begin
      bjp_taken = cmp_lt;
    end else if (bjp_bge) begin
      bjp_taken = ~cmp_lt;
    end else if (bjp_bltu) begin
      bjp_taken = cmp_ltu;
    end else if (bjp_bgeu) begin
      bjp_taken = ~cmp_ltu;
    end else if (bjp_jump) begin
      bjp_taken = 1'b1;
    end else begin
      bjp_taken = 1'b0;
    end
  end
  // ── CSR operations ───────────────────────────────────────────────────────
  logic [11:0] csr_imm;
  assign csr_imm = i_imm[11:0];
  // ── AGU: address generation ──────────────────────────────────────────────
  logic [31:0] agu_addr;
  assign agu_addr = 32'(i_rs1 + i_imm);
  // AGU sub-ops from i_info
  logic agu_load;
  assign agu_load = is_agu & (i_info[13:13] != 0);
  logic agu_store;
  assign agu_store = is_agu & (i_info[14:14] != 0);
  logic agu_amo;
  assign agu_amo = is_agu & (i_info[15:15] != 0);
  // ── NICE interface ───────────────────────────────────────────────────────
  logic nice_longp_r = 0;
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      nice_longp_r <= 0;
    end else begin
      if (flush_pulse) begin
        nice_longp_r <= 1'b0;
      end else if (is_nice & i_valid & nice_req_ready) begin
        nice_longp_r <= 1'b1;
      end else if (nice_rsp_multicyc_valid) begin
        nice_longp_r <= 1'b0;
      end
    end
  end
  // ── Output logic ─────────────────────────────────────────────────────────
  always_comb begin
    // Dispatch ready
    i_ready = wbck_o_ready & cmt_o_ready & (~is_agu | agu_icb_cmd_ready) & (~is_nice | nice_req_ready);
    // Long pipe flag (AGU or NICE can be long-pipe)
    i_longpipe = is_agu | is_nice;
    // AMO wait
    amo_wait = is_agu & agu_amo & ~oitf_empty;
    // ── Write-back ──────────────────────────────────────────────
    wbck_o_valid = i_valid & i_rdwen & ~flush_req;
    wbck_o_rdidx = i_rdidx;
    if (is_bjp) begin
      wbck_o_wdat = bjp_link;
    end else if (is_csr) begin
      wbck_o_wdat = read_csr_dat;
    end else if (is_agu) begin
      wbck_o_wdat = agu_icb_rsp_rdata;
    end else begin
      wbck_o_wdat = alu_result;
    end
    // ── Commit ──────────────────────────────────────────────────
    cmt_o_valid = i_valid & ~flush_req;
    cmt_o_pc_vld = i_pc_vld;
    cmt_o_pc = i_pc;
    cmt_o_instr = i_instr;
    cmt_o_imm = i_imm;
    cmt_o_rv32 = is_rv32;
    cmt_o_bjp = is_bjp;
    cmt_o_mret = is_mret;
    cmt_o_dret = is_dret;
    cmt_o_ecall = is_ecall;
    cmt_o_ebreak = is_ebreak;
    cmt_o_fencei = is_fencei;
    cmt_o_wfi = is_wfi;
    cmt_o_ifu_misalgn = i_misalgn;
    cmt_o_ifu_buserr = i_buserr;
    cmt_o_ifu_ilegl = i_ilegl;
    cmt_o_bjp_prdt = bjp_prdt;
    cmt_o_bjp_rslv = bjp_taken;
    cmt_o_misalgn = 1'b0;
    cmt_o_ld = agu_load;
    cmt_o_stamo = agu_store | agu_amo;
    cmt_o_buserr = agu_icb_rsp_err;
    cmt_o_badaddr = agu_addr;
    // ── CSR interface ───────────────────────────────────────────
    csr_ena = is_csr & i_valid & ~flush_req;
    csr_wr_en = is_csr & i_valid;
    csr_rd_en = is_csr & i_valid;
    csr_idx = csr_imm;
    wbck_csr_dat = i_rs1;
    // ── AGU ICB command ─────────────────────────────────────────
    agu_icb_cmd_valid = is_agu & i_valid & ~flush_req;
    agu_icb_cmd_addr = agu_addr;
    agu_icb_cmd_read = agu_load;
    agu_icb_cmd_wdata = i_rs2;
    agu_icb_cmd_wmask = 'hF;
    agu_icb_cmd_lock = agu_amo;
    agu_icb_cmd_excl = agu_amo;
    agu_icb_cmd_size = i_info[25:24];
    agu_icb_cmd_back2agu = agu_amo;
    agu_icb_cmd_usign = i_info[26:26] != 0;
    agu_icb_cmd_itag = i_itag;
    agu_icb_rsp_ready = wbck_o_ready & cmt_o_ready;
    // ── NICE interface ──────────────────────────────────────────
    nice_req_valid = is_nice & i_valid & ~flush_req;
    nice_req_instr = i_instr;
    nice_req_rs1 = i_rs1;
    nice_req_rs2 = i_rs2;
    nice_rsp_multicyc_ready = 1'b1;
    nice_longp_wbck_valid = nice_longp_r & nice_rsp_multicyc_valid;
    nice_o_itag = i_itag;
  end

endmodule

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
  // LSU has priority over NICE
  logic lsu_win;
  assign lsu_win = lsu_wbck_i_valid;
  logic sel_valid;
  assign sel_valid = lsu_wbck_i_valid | nice_longp_wbck_i_valid;
  logic [31:0] sel_wdat;
  assign sel_wdat = lsu_win ? lsu_wbck_i_wdat : nice_longp_wbck_i_wdat;
  logic sel_err;
  assign sel_err = lsu_win ? lsu_wbck_i_err : nice_longp_wbck_i_err;
  assign longp_wbck_o_valid = sel_valid & ~sel_err;
  assign longp_wbck_o_wdat = sel_wdat;
  assign longp_wbck_o_rdidx = oitf_ret_rdidx;
  assign longp_wbck_o_rdfpu = oitf_ret_rdfpu;
  assign longp_wbck_o_flags = 0;
  assign longp_excp_o_valid = sel_valid & sel_err;
  assign longp_excp_o_insterr = 1'b0;
  assign longp_excp_o_ld = lsu_cmt_i_ld;
  assign longp_excp_o_st = lsu_cmt_i_st;
  assign longp_excp_o_buserr = lsu_cmt_i_buserr;
  assign longp_excp_o_badaddr = lsu_cmt_i_badaddr;
  assign longp_excp_o_pc = oitf_ret_pc;
  assign lsu_wbck_i_ready = lsu_win & (longp_wbck_o_ready | sel_err);
  assign nice_longp_wbck_i_ready = ~lsu_win & (longp_wbck_o_ready | sel_err);
  assign oitf_ret_ena = sel_valid & (longp_wbck_o_ready | sel_err);

endmodule

// Writeback output
// Exception output
// Handshake: grant to winner, retire OITF entry
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
  assign longp_wbck_i_ready = 1;
  assign alu_wbck_i_ready = ~longp_wbck_i_valid;
  assign rf_wbck_o_wdat = longp_wbck_i_valid ? longp_wbck_i_wdat : alu_wbck_i_wdat;
  assign rf_wbck_o_rdidx = longp_wbck_i_valid ? longp_wbck_i_rdidx : alu_wbck_i_rdidx;
  assign rf_wbck_o_ena = (longp_wbck_i_valid & ~longp_wbck_i_rdfpu) | (~longp_wbck_i_valid & alu_wbck_i_valid);

endmodule

// RF is seq ready; longp has unconditional priority.
// Priority mux: longp_valid selects longp, else ALU passthrough
// ena: wbck_valid & ~rdfpu
// wbck_valid = longp_valid | (alu_valid & ~longp_valid)
// rdfpu = longp_valid ? longp_rdfpu : 0
// E203 HBirdv2 Execution Commit Unit
// Handles commit of ALU results, exception processing, trap/mret generation,
// WFI, pipeline flush, and debug mode interactions.
// Matches RealBench port interface.
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
  // ── WFI state register ────────────────────────────────────────────
  logic wfi_flag_r = 0;
  logic flush_req_r = 0;
  // ── Interrupt pending check ───────────────────────────────────────
  logic irq_ext_pend;
  assign irq_ext_pend = ext_irq_r & meie_r & status_mie_r;
  logic irq_sft_pend;
  assign irq_sft_pend = sft_irq_r & msie_r & status_mie_r;
  logic irq_tmr_pend;
  assign irq_tmr_pend = tmr_irq_r & mtie_r & status_mie_r;
  logic any_irq;
  assign any_irq = irq_ext_pend | irq_sft_pend | irq_tmr_pend | dbg_irq_r | lcl_irq_r | evt_r;
  // ── Commit logic ──────────────────────────────────────────────────
  logic cmt_ena;
  assign cmt_ena = alu_cmt_i_valid & ~amo_wait;
  // Exception conditions
  logic has_excp;
  assign has_excp = alu_cmt_i_ifu_misalgn | alu_cmt_i_ifu_buserr | alu_cmt_i_ifu_ilegl | alu_cmt_i_ecall | alu_cmt_i_ebreak | alu_cmt_i_misalgn | alu_cmt_i_buserr;
  logic need_flush;
  assign need_flush = alu_cmt_i_bjp | alu_cmt_i_fencei | alu_cmt_i_mret | alu_cmt_i_dret | alu_cmt_i_wfi | has_excp;
  // BJP misprediction
  logic bjp_mispred;
  assign bjp_mispred = alu_cmt_i_bjp & (alu_cmt_i_bjp_prdt != alu_cmt_i_bjp_rslv);
  // Trap generation
  logic trap_ena;
  assign trap_ena = cmt_ena & has_excp;
  logic mret_ena;
  assign mret_ena = cmt_ena & alu_cmt_i_mret & ~has_excp;
  // PC increment for non-flush commit
  logic [31:0] pc_incr;
  assign pc_incr = alu_cmt_i_rv32 ? 4 : 2;
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      flush_req_r <= 0;
      wfi_flag_r <= 0;
    end else begin
      // WFI flag management
      if (cmt_ena & alu_cmt_i_wfi & ~any_irq) begin
        wfi_flag_r <= 1'b1;
      end else if (any_irq) begin
        wfi_flag_r <= 1'b0;
      end
      // Flush request register
      if (pipe_flush_ack) begin
        flush_req_r <= 1'b0;
      end else if (cmt_ena & need_flush) begin
        flush_req_r <= 1'b1;
      end
    end
  end
  always_comb begin
    // ALU commit ready: always accept (single-cycle)
    alu_cmt_i_ready = ~amo_wait;
    // Commit status
    commit_mret = mret_ena;
    commit_trap = trap_ena;
    core_wfi = wfi_flag_r;
    nonflush_cmt_ena = cmt_ena & ~need_flush;
    excp_active = trap_ena;
    // WFI halt requests
    wfi_halt_ifu_req = wfi_flag_r;
    wfi_halt_exu_req = wfi_flag_r;
    // CSR commit outputs
    cmt_epc_ena = trap_ena;
    cmt_epc = alu_cmt_i_pc;
    cmt_cause_ena = trap_ena;
    cmt_badaddr_ena = trap_ena & (alu_cmt_i_misalgn | alu_cmt_i_buserr);
    cmt_badaddr = alu_cmt_i_badaddr;
    cmt_instret_ena = cmt_ena & ~has_excp;
    cmt_status_ena = trap_ena | mret_ena;
    cmt_mret_ena = mret_ena;
    // Trap cause encoding (simplified)
    if (alu_cmt_i_ifu_misalgn) begin
      cmt_cause = 0;
    end else if (alu_cmt_i_ifu_buserr) begin
      cmt_cause = 1;
    end else if (alu_cmt_i_ifu_ilegl) begin
      cmt_cause = 2;
    end else if (alu_cmt_i_ebreak) begin
      cmt_cause = 3;
    end else if (alu_cmt_i_misalgn & alu_cmt_i_ld) begin
      cmt_cause = 4;
    end else if (alu_cmt_i_buserr & alu_cmt_i_ld) begin
      cmt_cause = 5;
    end else if (alu_cmt_i_misalgn & alu_cmt_i_stamo) begin
      cmt_cause = 6;
    end else if (alu_cmt_i_buserr & alu_cmt_i_stamo) begin
      cmt_cause = 7;
    end else if (alu_cmt_i_ecall & u_mode) begin
      cmt_cause = 8;
    end else if (alu_cmt_i_ecall & m_mode) begin
      cmt_cause = 11;
    end else begin
      cmt_cause = 0;
    end
    // Debug CSR outputs
    cmt_dpc_ena = trap_ena & dbg_mode;
    cmt_dpc = alu_cmt_i_pc;
    cmt_dcause_ena = trap_ena & dbg_mode;
    cmt_dcause = 0;
    // Long-pipe exception handling
    longp_excp_i_ready = ~alu_cmt_i_valid;
    // Flush outputs
    flush_pulse = cmt_ena & need_flush;
    flush_req = flush_req_r;
    pipe_flush_req = flush_req_r;
    // Flush target address
    if (trap_ena) begin
      pipe_flush_add_op1 = csr_mtvec_r;
      pipe_flush_add_op2 = 0;
      pipe_flush_pc = csr_mtvec_r;
    end else if (mret_ena) begin
      pipe_flush_add_op1 = csr_epc_r;
      pipe_flush_add_op2 = 0;
      pipe_flush_pc = csr_epc_r;
    end else if (alu_cmt_i_dret) begin
      pipe_flush_add_op1 = csr_dpc_r;
      pipe_flush_add_op2 = 0;
      pipe_flush_pc = csr_dpc_r;
    end else begin
      pipe_flush_add_op1 = alu_cmt_i_pc;
      pipe_flush_add_op2 = pc_incr;
      pipe_flush_pc = alu_cmt_i_pc;
    end
  end

endmodule

// E203 HBirdv2 CSR Register File
// Machine-mode CSRs for RV32IM with debug support.
// Matches RealBench port interface.
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
  // ── CSR registers ─────────────────────────────────────────────────
  logic [31:0] mstatus_r = 0;
  logic [31:0] mie_r_reg = 0;
  logic [31:0] mtvec_r_reg = 0;
  logic [31:0] mscratch_r = 0;
  logic [31:0] mepc_r = 0;
  logic [31:0] mcause_r = 0;
  logic [31:0] mtval_r = 0;
  logic [31:0] mcycle_lo_r = 0;
  logic [31:0] mcycle_hi_r = 0;
  logic [31:0] minstret_lo_r = 0;
  logic [31:0] minstret_hi_r = 0;
  // mip is read-only
  logic [31:0] mip_val;
  assign mip_val = {{20{1'b0}}, ext_irq_r, {3{1'b0}}, tmr_irq_r, {3{1'b0}}, sft_irq_r, {3{1'b0}}};
  // mstatus fields
  logic mie_bit;
  assign mie_bit = mstatus_r[3:3] != 0;
  logic mpie_bit;
  assign mpie_bit = mstatus_r[7:7] != 0;
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      mcause_r <= 0;
      mcycle_hi_r <= 0;
      mcycle_lo_r <= 0;
      mepc_r <= 0;
      mie_r_reg <= 0;
      minstret_hi_r <= 0;
      minstret_lo_r <= 0;
      mscratch_r <= 0;
      mstatus_r <= 0;
      mtval_r <= 0;
      mtvec_r_reg <= 0;
    end else begin
      // mcycle auto-increment
      if (mcycle_lo_r == 'hFFFFFFFF) begin
        mcycle_lo_r <= 0;
        mcycle_hi_r <= 32'(mcycle_hi_r + 1);
      end else begin
        mcycle_lo_r <= 32'(mcycle_lo_r + 1);
      end
      // minstret increment on commit
      if (cmt_instret_ena) begin
        if (minstret_lo_r == 'hFFFFFFFF) begin
          minstret_lo_r <= 0;
          minstret_hi_r <= 32'(minstret_hi_r + 1);
        end else begin
          minstret_lo_r <= 32'(minstret_lo_r + 1);
        end
      end
      // Trap entry
      if (cmt_epc_ena) begin
        mepc_r <= cmt_epc;
      end
      if (cmt_cause_ena) begin
        mcause_r <= cmt_cause;
      end
      if (cmt_badaddr_ena) begin
        mtval_r <= cmt_badaddr;
      end
      if (cmt_status_ena & ~cmt_mret_ena) begin
        // Save MPIE = MIE, clear MIE
        mstatus_r <= {mstatus_r[31:8], mie_bit, mstatus_r[6:4], 1'b0, mstatus_r[2:0]};
      end else if (cmt_mret_ena) begin
        // Restore MIE = MPIE, MPIE = 1
        mstatus_r <= {mstatus_r[31:8], 1'b1, mstatus_r[6:4], mpie_bit, mstatus_r[2:0]};
      end else if (csr_ena & csr_wr_en & (csr_idx == 'h300)) begin
        mstatus_r <= wbck_csr_dat;
      end
      // CSR writes (non-mstatus)
      if (csr_ena & csr_wr_en) begin
        if (csr_idx == 'h304) begin
          mie_r_reg <= wbck_csr_dat;
        end else if (csr_idx == 'h305) begin
          mtvec_r_reg <= wbck_csr_dat;
        end else if (csr_idx == 'h340) begin
          mscratch_r <= wbck_csr_dat;
        end else if (csr_idx == 'h341) begin
          mepc_r <= wbck_csr_dat;
        end else if (csr_idx == 'h342) begin
          mcause_r <= wbck_csr_dat;
        end else if (csr_idx == 'h343) begin
          mtval_r <= wbck_csr_dat;
        end else if (csr_idx == 'hB00) begin
          mcycle_lo_r <= wbck_csr_dat;
        end else if (csr_idx == 'hB80) begin
          mcycle_hi_r <= wbck_csr_dat;
        end else if (csr_idx == 'hB02) begin
          minstret_lo_r <= wbck_csr_dat;
        end else if (csr_idx == 'hB82) begin
          minstret_hi_r <= wbck_csr_dat;
        end
      end
    end
  end
  always_comb begin
    // CSR read mux
    if (csr_idx == 'h300) begin
      read_csr_dat = mstatus_r;
    end else if (csr_idx == 'h304) begin
      read_csr_dat = mie_r_reg;
    end else if (csr_idx == 'h305) begin
      read_csr_dat = mtvec_r_reg;
    end else if (csr_idx == 'h340) begin
      read_csr_dat = mscratch_r;
    end else if (csr_idx == 'h341) begin
      read_csr_dat = mepc_r;
    end else if (csr_idx == 'h342) begin
      read_csr_dat = mcause_r;
    end else if (csr_idx == 'h343) begin
      read_csr_dat = mtval_r;
    end else if (csr_idx == 'h344) begin
      read_csr_dat = mip_val;
    end else if (csr_idx == 'hB00) begin
      read_csr_dat = mcycle_lo_r;
    end else if (csr_idx == 'hB80) begin
      read_csr_dat = mcycle_hi_r;
    end else if (csr_idx == 'hB02) begin
      read_csr_dat = minstret_lo_r;
    end else if (csr_idx == 'hB82) begin
      read_csr_dat = minstret_hi_r;
    end else if (csr_idx == 'hF11) begin
      read_csr_dat = 0;
    end else if (csr_idx == 'hF14) begin
      read_csr_dat = 32'($unsigned(core_mhartid));
    end else if (csr_idx == 'h7B0) begin
      read_csr_dat = dcsr_r;
    end else if (csr_idx == 'h7B1) begin
      read_csr_dat = dpc_r;
    end else if (csr_idx == 'h7B2) begin
      read_csr_dat = dscratch_r;
    end else begin
      read_csr_dat = 0;
    end
    // CSR output values
    csr_epc_r = mepc_r;
    csr_dpc_r = dpc_r;
    csr_mtvec_r = mtvec_r_reg;
    // Interrupt enables from mie register
    status_mie_r = mie_bit;
    meie_r = mie_r_reg[11:11] != 0;
    mtie_r = mie_r_reg[7:7] != 0;
    msie_r = mie_r_reg[3:3] != 0;
    // Control outputs (simplified)
    nice_xs_off = 1'b0;
    tm_stop = dbg_stopcycle;
    core_cgstop = dbg_stopcycle;
    tcm_cgstop = dbg_stopcycle;
    itcm_nohold = 1'b0;
    mdv_nob2b = 1'b0;
    // Privilege mode (machine mode only in E203)
    m_mode = 1'b1;
    s_mode = 1'b0;
    h_mode = 1'b0;
    u_mode = 1'b0;
    // CSR access illegal check (simplified)
    csr_access_ilgl = 1'b0;
    // Debug CSR write enables
    wr_dcsr_ena = csr_ena & csr_wr_en & (csr_idx == 'h7B0);
    wr_dpc_ena = csr_ena & csr_wr_en & (csr_idx == 'h7B1);
    wr_dscratch_ena = csr_ena & csr_wr_en & (csr_idx == 'h7B2);
    wr_csr_nxt = wbck_csr_dat;
  end

endmodule

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
