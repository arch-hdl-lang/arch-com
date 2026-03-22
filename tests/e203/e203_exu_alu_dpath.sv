// E203 HBirdv2 ALU shared datapath
// Handles ALU, BJP (branch/jump), and AGU (address gen) operation requests.
// Pure combinational except for two shared-buffer registers (no reset).
module AluDpath #(
  parameter int XLEN = 32
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
  input logic [32-1:0] alu_req_alu_op1,
  input logic [32-1:0] alu_req_alu_op2,
  output logic [32-1:0] alu_req_alu_res,
  input logic bjp_req_alu,
  input logic [32-1:0] bjp_req_alu_op1,
  input logic [32-1:0] bjp_req_alu_op2,
  input logic bjp_req_alu_cmp_eq,
  input logic bjp_req_alu_cmp_ne,
  input logic bjp_req_alu_cmp_lt,
  input logic bjp_req_alu_cmp_gt,
  input logic bjp_req_alu_cmp_ltu,
  input logic bjp_req_alu_cmp_gtu,
  input logic bjp_req_alu_add,
  output logic [32-1:0] bjp_req_alu_add_res,
  output logic bjp_req_alu_cmp_res,
  input logic agu_req_alu,
  input logic [32-1:0] agu_req_alu_op1,
  input logic [32-1:0] agu_req_alu_op2,
  input logic agu_req_alu_swap,
  input logic agu_req_alu_add,
  input logic agu_req_alu_and,
  input logic agu_req_alu_or,
  input logic agu_req_alu_xor,
  input logic agu_req_alu_max,
  input logic agu_req_alu_min,
  input logic agu_req_alu_maxu,
  input logic agu_req_alu_minu,
  output logic [32-1:0] agu_req_alu_res,
  input logic agu_sbf_0_ena,
  input logic [32-1:0] agu_sbf_0_nxt,
  input logic agu_sbf_1_ena,
  input logic [32-1:0] agu_sbf_1_nxt
);

  // ── Regular ALU requests ───────────────────────────────────────────────
  // ── Branch/Jump unit requests ──────────────────────────────────────────
  // ── AGU requests (AMO + address calc) ─────────────────────────────────
  // ── Shared-buffer load enables (AGU multi-cycle AMO) ──────────────────
  // ── Shared buffer registers (no reset — hold AMO operands) ────────────
  logic [32-1:0] sbf_0_r = 0;
  logic [32-1:0] sbf_1_r = 0;
  always_ff @(posedge clk) begin
    if (agu_sbf_0_ena) begin
      sbf_0_r <= agu_sbf_0_nxt;
    end
    if (agu_sbf_1_ena) begin
      sbf_1_r <= agu_sbf_1_nxt;
    end
  end
  // ── Shared operand mux: BJP > AGU > ALU ───────────────────────────────
  logic [32-1:0] req_op1;
  assign req_op1 = (bjp_req_alu) ? (bjp_req_alu_op1) : ((agu_req_alu) ? (agu_req_alu_op1) : (alu_req_alu_op1));
  logic [32-1:0] req_op2;
  assign req_op2 = (bjp_req_alu) ? (bjp_req_alu_op2) : ((agu_req_alu) ? (agu_req_alu_op2) : (alu_req_alu_op2));
  // ── Shared adder (add/sub — BJP add, ALU add/sub, AGU add) ────────────
  // Subtraction: op1 + ~op2 + 1 (two's complement); captured in 33 bits
  logic [32-1:0] adder_op2_inv;
  assign adder_op2_inv = (~req_op2);
  logic do_sub;
  assign do_sub = ((((((((((((alu_req_alu_sub | alu_req_alu_slt) | alu_req_alu_sltu) | bjp_req_alu_cmp_eq) | bjp_req_alu_cmp_ne) | bjp_req_alu_cmp_lt) | bjp_req_alu_cmp_gt) | bjp_req_alu_cmp_ltu) | bjp_req_alu_cmp_gtu) | agu_req_alu_max) | agu_req_alu_min) | agu_req_alu_maxu) | agu_req_alu_minu);
  logic [32-1:0] adder_op2_sel;
  assign adder_op2_sel = (do_sub) ? (adder_op2_inv) : (req_op2);
  // 33-bit result: bit 32 = carry-out (unsigned) or sign (signed sext)
  logic [33-1:0] adder_res;
  assign adder_res = 33'(((33'($unsigned(req_op1)) + 33'($unsigned(adder_op2_sel))) + 33'($unsigned(do_sub))));
  logic [32-1:0] adder_res32;
  assign adder_res32 = 32'(adder_res);
  // Unsigned carry-out: bit 32 = 1 → no borrow → op1 >= op2
  logic adder_carry;
  assign adder_carry = ((adder_res >> 32) != 0);
  // ── Signed comparison via SInt cast ───────────────────────────────────
  logic signed_lt;
  assign signed_lt = ($signed(req_op1) < $signed(req_op2));
  // ── Shift amount: lower 5 bits of op2 ─────────────────────────────────
  logic [5-1:0] shamt;
  assign shamt = alu_req_alu_op2[4:0];
  // ── XOR (also used for EQ/NE comparison) ──────────────────────────────
  logic [32-1:0] xor_res;
  assign xor_res = (req_op1 ^ req_op2);
  logic xor_all_zero;
  assign xor_all_zero = (xor_res == 0);
  // ── Combinational outputs ──────────────────────────────────────────────
  always_comb begin
    if (alu_req_alu_add) begin
      alu_req_alu_res = adder_res32;
    end else if (alu_req_alu_sub) begin
      alu_req_alu_res = adder_res32;
    end else if (alu_req_alu_xor) begin
      alu_req_alu_res = xor_res;
    end else if (alu_req_alu_or) begin
      alu_req_alu_res = (alu_req_alu_op1 | alu_req_alu_op2);
    end else if (alu_req_alu_and) begin
      alu_req_alu_res = (alu_req_alu_op1 & alu_req_alu_op2);
    end else if (alu_req_alu_sll) begin
      alu_req_alu_res = 32'((alu_req_alu_op1 << shamt));
    end else if (alu_req_alu_srl) begin
      alu_req_alu_res = (alu_req_alu_op1 >> shamt);
    end else if (alu_req_alu_sra) begin
      alu_req_alu_res = 32'($unsigned(($signed(alu_req_alu_op1) >>> shamt)));
    end else if (alu_req_alu_slt) begin
      alu_req_alu_res = 32'($unsigned(signed_lt));
    end else if (alu_req_alu_sltu) begin
      alu_req_alu_res = 32'($unsigned((~adder_carry)));
    end else if (alu_req_alu_lui) begin
      alu_req_alu_res = alu_req_alu_op2;
    end else begin
      alu_req_alu_res = 0;
    end
    bjp_req_alu_add_res = adder_res32;
    if (bjp_req_alu_cmp_eq) begin
      bjp_req_alu_cmp_res = xor_all_zero;
    end else if (bjp_req_alu_cmp_ne) begin
      bjp_req_alu_cmp_res = (~xor_all_zero);
    end else if (bjp_req_alu_cmp_lt) begin
      bjp_req_alu_cmp_res = signed_lt;
    end else if (bjp_req_alu_cmp_gt) begin
      bjp_req_alu_cmp_res = ((~signed_lt) & (~xor_all_zero));
    end else if (bjp_req_alu_cmp_ltu) begin
      bjp_req_alu_cmp_res = (~adder_carry);
    end else if (bjp_req_alu_cmp_gtu) begin
      bjp_req_alu_cmp_res = (adder_carry & (~xor_all_zero));
    end else begin
      bjp_req_alu_cmp_res = 1'b0;
    end
    if (agu_req_alu_add) begin
      agu_req_alu_res = adder_res32;
    end else if (agu_req_alu_and) begin
      agu_req_alu_res = (agu_req_alu_op1 & agu_req_alu_op2);
    end else if (agu_req_alu_or) begin
      agu_req_alu_res = (agu_req_alu_op1 | agu_req_alu_op2);
    end else if (agu_req_alu_xor) begin
      agu_req_alu_res = (agu_req_alu_op1 ^ agu_req_alu_op2);
    end else if (agu_req_alu_swap) begin
      agu_req_alu_res = agu_req_alu_op2;
    end else if (agu_req_alu_max) begin
      agu_req_alu_res = (signed_lt) ? (agu_req_alu_op2) : (agu_req_alu_op1);
    end else if (agu_req_alu_min) begin
      agu_req_alu_res = (signed_lt) ? (agu_req_alu_op1) : (agu_req_alu_op2);
    end else if (agu_req_alu_maxu) begin
      agu_req_alu_res = ((~adder_carry)) ? (agu_req_alu_op2) : (agu_req_alu_op1);
    end else if (agu_req_alu_minu) begin
      agu_req_alu_res = ((~adder_carry)) ? (agu_req_alu_op1) : (agu_req_alu_op2);
    end else begin
      agu_req_alu_res = 0;
    end
  end

endmodule

// ── ALU result ──────────────────────────────────────────────────────
// ── BJP addition result (JAL/JALR return address = PC + 4/2) ────────
// ── BJP comparison result ────────────────────────────────────────────
// ── AGU result ───────────────────────────────────────────────────────
// SWAP: write mem value (op2) to rd; op1 is the address
