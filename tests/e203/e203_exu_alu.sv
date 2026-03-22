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

