// E203 Address Generation Unit
// Computes load/store effective address: base (rs1) + offset (imm).
// Also handles AMO (atomic) operations with the shared ALU datapath.
// In E203, AGU uses the shared adder in AluDpath via agu_req_alu signals.
// This module wraps the address calculation and generates ICB cmd signals.
module ExuAgu #(
  parameter int XLEN = 32
) (
  input logic i_valid,
  output logic i_ready,
  input logic [32-1:0] i_rs1,
  input logic [32-1:0] i_rs2,
  input logic [32-1:0] i_imm,
  input logic i_load,
  input logic i_store,
  input logic [5-1:0] i_rd_idx,
  input logic i_rd_en,
  input logic [3-1:0] i_funct3,
  output logic icb_cmd_valid,
  input logic icb_cmd_ready,
  output logic [32-1:0] icb_cmd_addr,
  output logic [32-1:0] icb_cmd_wdata,
  output logic [4-1:0] icb_cmd_wmask,
  output logic icb_cmd_read,
  input logic icb_rsp_valid,
  output logic icb_rsp_ready,
  input logic [32-1:0] icb_rsp_rdata,
  output logic o_valid,
  input logic o_ready,
  output logic [32-1:0] o_wdat,
  output logic [5-1:0] o_rd_idx,
  output logic o_rd_en
);

  // From dispatch
  // base address (register)
  // store data
  // offset (sign-extended)
  // byte/half/word
  // Memory ICB command
  // Memory ICB response
  // Writeback
  // ── Address calculation ─────────────────────────────────────────
  logic [32-1:0] eff_addr;
  assign eff_addr = 32'((i_rs1 + i_imm));
  logic [2-1:0] byte_off;
  assign byte_off = eff_addr[1:0];
  logic [32-1:0] word_addr;
  assign word_addr = {eff_addr[31:2], {2{1'b0}}};
  // ── funct3 decode ───────────────────────────────────────────────
  logic is_byte;
  assign is_byte = (i_funct3[1:0] == 0);
  logic is_half;
  assign is_half = (i_funct3[1:0] == 1);
  logic is_unsigned;
  assign is_unsigned = (i_funct3[2:2] != 0);
  // ── Store byte-enable ───────────────────────────────────────────
  logic [4-1:0] wstrb_byte;
  assign wstrb_byte = ((byte_off == 0)) ? ('h1) : (((byte_off == 1)) ? ('h2) : (((byte_off == 2)) ? ('h4) : ('h8)));
  logic [4-1:0] wstrb_half;
  assign wstrb_half = ((byte_off[1:1] == 0)) ? ('h3) : ('hC);
  logic [4-1:0] wmask_sel;
  assign wmask_sel = (is_byte) ? (wstrb_byte) : ((is_half) ? (wstrb_half) : ('hF));
  // ── Store data alignment ────────────────────────────────────────
  logic [8-1:0] wdata_byte;
  assign wdata_byte = i_rs2[7:0];
  logic [16-1:0] wdata_half;
  assign wdata_half = i_rs2[15:0];
  logic [32-1:0] store_aligned;
  assign store_aligned = (is_byte) ? (32'((32'($unsigned(wdata_byte)) << 32'((32'($unsigned(byte_off)) << 3))))) : ((is_half) ? (32'((32'($unsigned(wdata_half)) << 32'((32'($unsigned(byte_off[1:1])) << 4))))) : (i_rs2));
  // ── Load result alignment ───────────────────────────────────────
  logic [32-1:0] rdata_shifted;
  assign rdata_shifted = 32'((icb_rsp_rdata >> 32'((32'($unsigned(byte_off)) << 3))));
  logic [8-1:0] load_byte;
  assign load_byte = rdata_shifted[7:0];
  logic [16-1:0] load_half;
  assign load_half = rdata_shifted[15:0];
  logic byte_sign;
  assign byte_sign = (load_byte[7:7] != 0);
  logic half_sign;
  assign half_sign = (load_half[15:15] != 0);
  logic [32-1:0] lb_result;
  assign lb_result = {{24{byte_sign}}, load_byte};
  logic [32-1:0] lbu_result;
  assign lbu_result = {{24{1'b0}}, load_byte};
  logic [32-1:0] lh_result;
  assign lh_result = {{16{half_sign}}, load_half};
  logic [32-1:0] lhu_result;
  assign lhu_result = {{16{1'b0}}, load_half};
  logic [32-1:0] load_val;
  assign load_val = (is_byte) ? ((is_unsigned) ? (lbu_result) : (lb_result)) : ((is_half) ? ((is_unsigned) ? (lhu_result) : (lh_result)) : (icb_rsp_rdata));
  assign icb_cmd_valid = (i_valid & (i_load | i_store));
  assign icb_cmd_addr = word_addr;
  assign icb_cmd_wdata = store_aligned;
  assign icb_cmd_wmask = (i_store) ? (wmask_sel) : (0);
  assign icb_cmd_read = i_load;
  assign icb_rsp_ready = o_ready;
  assign i_ready = icb_cmd_ready;
  assign o_valid = (icb_rsp_valid & i_load);
  assign o_wdat = load_val;
  assign o_rd_idx = i_rd_idx;
  assign o_rd_en = (i_rd_en & i_load);

endmodule

// ICB command
// ICB response
// Dispatch handshake
// Writeback (loads write to rd)
