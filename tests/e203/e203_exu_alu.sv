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

// E203 HBirdv2 ALU Top-Level
// Orchestrates the shared ALU datapath (AluDpath) and branch/jump unit (BjpUnit).
// Receives dispatched instructions, decodes operation type, routes operands to
// sub-units, and presents results to the write-back path.
//
// Three sub-unit request types share the AluDpath adder:
//   ALU — register-register / register-immediate arithmetic
//   BJP — branch comparison + target/link address computation
//   AGU — load/store/AMO effective address + atomic ALU ops
//
// Purely combinational top-level; registers live only inside AluDpath (shared buffers).
module ExuAlu #(
  parameter int XLEN = 32
) (
  input logic clk,
  input logic rst_n,
  input logic i_valid,
  output logic i_ready,
  input logic [32-1:0] i_rs1,
  input logic [32-1:0] i_rs2,
  input logic [32-1:0] i_pc,
  input logic [32-1:0] i_imm,
  input logic [5-1:0] i_rdidx,
  input logic i_alu,
  input logic i_alu_add,
  input logic i_alu_sub,
  input logic i_alu_xor,
  input logic i_alu_sll,
  input logic i_alu_srl,
  input logic i_alu_sra,
  input logic i_alu_or,
  input logic i_alu_and,
  input logic i_alu_slt,
  input logic i_alu_sltu,
  input logic i_alu_lui,
  input logic i_bjp,
  input logic i_beq,
  input logic i_bne,
  input logic i_blt,
  input logic i_bge,
  input logic i_bltu,
  input logic i_bgeu,
  input logic i_jump,
  input logic i_agu,
  input logic i_agu_swap,
  input logic i_agu_add,
  input logic i_agu_and,
  input logic i_agu_or,
  input logic i_agu_xor,
  input logic i_agu_max,
  input logic i_agu_min,
  input logic i_agu_maxu,
  input logic i_agu_minu,
  input logic i_agu_sbf_0_ena,
  input logic [32-1:0] i_agu_sbf_0_nxt,
  input logic i_agu_sbf_1_ena,
  input logic [32-1:0] i_agu_sbf_1_nxt,
  output logic o_valid,
  input logic o_ready,
  output logic [32-1:0] o_wdat,
  output logic [5-1:0] o_rdidx,
  output logic o_bjp_taken,
  output logic [32-1:0] o_bjp_tgt,
  output logic [32-1:0] o_bjp_lnk
);

  // ── Dispatch interface ───────────────────────────────────────────────────
  // ── ALU operation decode (one-hot) ───────────────────────────────────────
  // ── BJP operation decode ─────────────────────────────────────────────────
  // ── AGU operation decode ─────────────────────────────────────────────────
  // ── Write-back output ────────────────────────────────────────────────────
  // ── BJP result output (to PC update logic) ───────────────────────────────
  // ── Instantiate AluDpath ─────────────────────────────────────────────────
  logic [32-1:0] dpath_alu_res;
  logic [32-1:0] dpath_bjp_add_res;
  logic dpath_bjp_cmp_res;
  logic [32-1:0] dpath_agu_res;
  AluDpath dpath (
    .clk(clk),
    .rst_n(rst_n),
    .alu_req_alu(i_alu),
    .alu_req_alu_add(i_alu_add),
    .alu_req_alu_sub(i_alu_sub),
    .alu_req_alu_xor(i_alu_xor),
    .alu_req_alu_sll(i_alu_sll),
    .alu_req_alu_srl(i_alu_srl),
    .alu_req_alu_sra(i_alu_sra),
    .alu_req_alu_or(i_alu_or),
    .alu_req_alu_and(i_alu_and),
    .alu_req_alu_slt(i_alu_slt),
    .alu_req_alu_sltu(i_alu_sltu),
    .alu_req_alu_lui(i_alu_lui),
    .alu_req_alu_op1(i_rs1),
    .alu_req_alu_op2(i_rs2),
    .alu_req_alu_res(dpath_alu_res),
    .bjp_req_alu(i_bjp),
    .bjp_req_alu_op1(i_rs1),
    .bjp_req_alu_op2(i_rs2),
    .bjp_req_alu_cmp_eq(i_beq),
    .bjp_req_alu_cmp_ne(i_bne),
    .bjp_req_alu_cmp_lt(i_blt),
    .bjp_req_alu_cmp_gt(i_bge),
    .bjp_req_alu_cmp_ltu(i_bltu),
    .bjp_req_alu_cmp_gtu(i_bgeu),
    .bjp_req_alu_add(i_bjp),
    .bjp_req_alu_add_res(dpath_bjp_add_res),
    .bjp_req_alu_cmp_res(dpath_bjp_cmp_res),
    .agu_req_alu(i_agu),
    .agu_req_alu_op1(i_rs1),
    .agu_req_alu_op2(i_imm),
    .agu_req_alu_swap(i_agu_swap),
    .agu_req_alu_add(i_agu_add),
    .agu_req_alu_and(i_agu_and),
    .agu_req_alu_or(i_agu_or),
    .agu_req_alu_xor(i_agu_xor),
    .agu_req_alu_max(i_agu_max),
    .agu_req_alu_min(i_agu_min),
    .agu_req_alu_maxu(i_agu_maxu),
    .agu_req_alu_minu(i_agu_minu),
    .agu_req_alu_res(dpath_agu_res),
    .agu_sbf_0_ena(i_agu_sbf_0_ena),
    .agu_sbf_0_nxt(i_agu_sbf_0_nxt),
    .agu_sbf_1_ena(i_agu_sbf_1_ena),
    .agu_sbf_1_nxt(i_agu_sbf_1_nxt)
  );
  // ── Instantiate BjpUnit ──────────────────────────────────────────────────
  logic bjp_taken;
  logic [32-1:0] bjp_tgt;
  logic [32-1:0] bjp_lnk;
  logic bjp_cmp_res;
  BjpUnit bjp_unit (
    .i_tgt_op1(i_pc),
    .i_tgt_op2(i_imm),
    .i_cmp_rs1(i_rs1),
    .i_cmp_rs2(i_rs2),
    .i_beq(i_beq),
    .i_bne(i_bne),
    .i_blt(i_blt),
    .i_bge(i_bge),
    .i_bltu(i_bltu),
    .i_bgeu(i_bgeu),
    .i_jump(i_jump),
    .i_lnk_pc(i_pc),
    .o_taken(bjp_taken),
    .o_tgt(bjp_tgt),
    .o_lnk(bjp_lnk),
    .o_cmp_res(bjp_cmp_res)
  );
  // ── Result mux and output logic ──────────────────────────────────────────
  always_comb begin
    i_ready = o_ready;
    o_valid = i_valid;
    o_rdidx = i_rdidx;
    o_bjp_taken = (bjp_taken & i_bjp);
    o_bjp_tgt = bjp_tgt;
    o_bjp_lnk = bjp_lnk;
    if (i_bjp) begin
      o_wdat = bjp_lnk;
    end else if (i_agu) begin
      o_wdat = dpath_agu_res;
    end else begin
      o_wdat = dpath_alu_res;
    end
  end

endmodule

