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
  input logic [32-1:0] i_rs1,
  input logic [32-1:0] i_rs2,
  input logic [32-1:0] i_imm,
  input logic [32-1:0] i_info,
  input logic [32-1:0] i_pc,
  input logic [32-1:0] i_instr,
  input logic i_pc_vld,
  input logic [5-1:0] i_rdidx,
  input logic i_rdwen,
  input logic i_ilegl,
  input logic i_buserr,
  input logic i_misalgn,
  input logic flush_req,
  input logic flush_pulse,
  output logic cmt_o_valid,
  input logic cmt_o_ready,
  output logic cmt_o_pc_vld,
  output logic [32-1:0] cmt_o_pc,
  output logic [32-1:0] cmt_o_instr,
  output logic [32-1:0] cmt_o_imm,
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
  output logic [32-1:0] cmt_o_badaddr,
  output logic wbck_o_valid,
  input logic wbck_o_ready,
  output logic [32-1:0] wbck_o_wdat,
  output logic [5-1:0] wbck_o_rdidx,
  input logic mdv_nob2b,
  output logic csr_ena,
  output logic csr_wr_en,
  output logic csr_rd_en,
  output logic [12-1:0] csr_idx,
  input logic nonflush_cmt_ena,
  input logic csr_access_ilgl,
  input logic [32-1:0] read_csr_dat,
  output logic [32-1:0] wbck_csr_dat,
  output logic agu_icb_cmd_valid,
  input logic agu_icb_cmd_ready,
  output logic [32-1:0] agu_icb_cmd_addr,
  output logic agu_icb_cmd_read,
  output logic [32-1:0] agu_icb_cmd_wdata,
  output logic [4-1:0] agu_icb_cmd_wmask,
  output logic agu_icb_cmd_lock,
  output logic agu_icb_cmd_excl,
  output logic [2-1:0] agu_icb_cmd_size,
  output logic agu_icb_cmd_back2agu,
  output logic agu_icb_cmd_usign,
  output logic agu_icb_cmd_itag,
  input logic agu_icb_rsp_valid,
  output logic agu_icb_rsp_ready,
  input logic agu_icb_rsp_err,
  input logic agu_icb_rsp_excl_ok,
  input logic [32-1:0] agu_icb_rsp_rdata,
  output logic nice_req_valid,
  input logic nice_req_ready,
  output logic [32-1:0] nice_req_instr,
  output logic [32-1:0] nice_req_rs1,
  output logic [32-1:0] nice_req_rs2,
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
  assign alu_add = is_alu & i_info[13:13] != 0;
  logic alu_sub;
  assign alu_sub = is_alu & i_info[14:14] != 0;
  logic alu_xor;
  assign alu_xor = is_alu & i_info[15:15] != 0;
  logic alu_sll;
  assign alu_sll = is_alu & i_info[16:16] != 0;
  logic alu_srl;
  assign alu_srl = is_alu & i_info[17:17] != 0;
  logic alu_sra;
  assign alu_sra = is_alu & i_info[18:18] != 0;
  logic alu_or;
  assign alu_or = is_alu & i_info[19:19] != 0;
  logic alu_and;
  assign alu_and = is_alu & i_info[20:20] != 0;
  logic alu_slt;
  assign alu_slt = is_alu & i_info[21:21] != 0;
  logic alu_sltu;
  assign alu_sltu = is_alu & i_info[22:22] != 0;
  logic alu_lui;
  assign alu_lui = is_alu & i_info[23:23] != 0;
  // ── Simple ALU result ────────────────────────────────────────────────────
  logic [32-1:0] alu_result;
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
  logic [32-1:0] bjp_add_res;
  assign bjp_add_res = 32'(i_pc + i_imm);
  logic [32-1:0] bjp_link;
  assign bjp_link = 32'(i_pc + 4);
  logic cmp_eq;
  assign cmp_eq = i_rs1 == i_rs2;
  logic cmp_lt;
  assign cmp_lt = $signed(i_rs1) < $signed(i_rs2);
  logic cmp_ltu;
  assign cmp_ltu = i_rs1 < i_rs2;
  // BJP sub-op from i_info
  logic bjp_beq;
  assign bjp_beq = is_bjp & i_info[13:13] != 0;
  logic bjp_bne;
  assign bjp_bne = is_bjp & i_info[14:14] != 0;
  logic bjp_blt;
  assign bjp_blt = is_bjp & i_info[15:15] != 0;
  logic bjp_bge;
  assign bjp_bge = is_bjp & i_info[16:16] != 0;
  logic bjp_bltu;
  assign bjp_bltu = is_bjp & i_info[17:17] != 0;
  logic bjp_bgeu;
  assign bjp_bgeu = is_bjp & i_info[18:18] != 0;
  logic bjp_jump;
  assign bjp_jump = is_bjp & i_info[19:19] != 0;
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
  logic [12-1:0] csr_imm;
  assign csr_imm = i_imm[11:0];
  // ── AGU: address generation ──────────────────────────────────────────────
  logic [32-1:0] agu_addr;
  assign agu_addr = 32'(i_rs1 + i_imm);
  // AGU sub-ops from i_info
  logic agu_load;
  assign agu_load = is_agu & i_info[13:13] != 0;
  logic agu_store;
  assign agu_store = is_agu & i_info[14:14] != 0;
  logic agu_amo;
  assign agu_amo = is_agu & i_info[15:15] != 0;
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

