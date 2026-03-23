// E203 HBirdv2 Branch/Jump Unit
// Computes: branch target address, link address (PC+4), and branch comparison
// result.  Purely combinational.
//
// The caller muxes op1/op2 appropriately:
//   Branch : op1 = PC,  op2 = sign-extended offset
//   JAL    : op1 = PC,  op2 = sign-extended 21-bit offset
//   JALR   : op1 = rs1, op2 = sign-extended 12-bit offset (JALR clears bit 0)
module BjpUnit #(
  parameter int XLEN = 32
) (
  input logic [32-1:0] i_tgt_op1,
  input logic [32-1:0] i_tgt_op2,
  input logic [32-1:0] i_cmp_rs1,
  input logic [32-1:0] i_cmp_rs2,
  input logic i_beq,
  input logic i_bne,
  input logic i_blt,
  input logic i_bge,
  input logic i_bltu,
  input logic i_bgeu,
  input logic i_jump,
  input logic [32-1:0] i_lnk_pc,
  output logic o_taken,
  output logic [32-1:0] o_tgt,
  output logic [32-1:0] o_lnk,
  output logic o_cmp_res
);

  // ── Target address operands ────────────────────────────────────────────
  // target = op1 + op2 (with bit[0] forced to 0 by caller for JALR)
  // ── Comparison operands ────────────────────────────────────────────────
  // ── Branch type selectors ──────────────────────────────────────────────
  // ── Unconditional jump (JAL/JALR — seq taken) ───────────────────────
  // ── Link address: input PC → output PC+4 ──────────────────────────────
  // ── Outputs ────────────────────────────────────────────────────────────
  // branch/jump taken
  // target address
  // link address (i_lnk_pc + 4)
  // raw comparison result (independent of jump)
  // ── Target address: op1 + op2, keep lower 32 bits ─────────────────────
  logic [33-1:0] tgt_sum;
  assign tgt_sum = 33'((33'($unsigned(i_tgt_op1)) + 33'($unsigned(i_tgt_op2))));
  logic [32-1:0] tgt_addr;
  assign tgt_addr = 32'(tgt_sum);
  // ── Link address: PC + 4 ──────────────────────────────────────────────
  logic [33-1:0] lnk_sum;
  assign lnk_sum = 33'((33'($unsigned(i_lnk_pc)) + 33'($unsigned(32'($unsigned(4))))));
  logic [32-1:0] lnk_addr;
  assign lnk_addr = 32'(lnk_sum);
  // ── Comparison logic ───────────────────────────────────────────────────
  // XOR-based equality
  logic [32-1:0] xor_res;
  assign xor_res = (i_cmp_rs1 ^ i_cmp_rs2);
  logic xor_zero;
  assign xor_zero = (xor_res == 0);
  // Subtraction via two's-complement: rs1 + ~rs2 + 1
  logic [32-1:0] cmp_op2_inv;
  assign cmp_op2_inv = (~i_cmp_rs2);
  logic [33-1:0] sub_res;
  assign sub_res = 33'(((33'($unsigned(i_cmp_rs1)) + 33'($unsigned(cmp_op2_inv))) + 33'($unsigned(1'b1))));
  // carry=1 → rs1 >= rs2 unsigned (no borrow)
  logic sub_carry;
  assign sub_carry = ((sub_res >> 32) != 0);
  // Signed less-than via SInt cast
  logic signed_lt;
  assign signed_lt = ($signed(i_cmp_rs1) < $signed(i_cmp_rs2));
  // ── Comparison result mux ──────────────────────────────────────────────
  logic cmp_result;
  assign cmp_result = (i_beq) ? (xor_zero) : ((i_bne) ? ((~xor_zero)) : ((i_blt) ? (signed_lt) : ((i_bge) ? ((~signed_lt)) : ((i_bltu) ? ((~sub_carry)) : ((i_bgeu) ? (sub_carry) : (1'b0))))));
  // ── Drive outputs ──────────────────────────────────────────────────────
  assign o_cmp_res = cmp_result;
  assign o_taken = (i_jump | cmp_result);
  assign o_tgt = tgt_addr;
  assign o_lnk = lnk_addr;

endmodule

