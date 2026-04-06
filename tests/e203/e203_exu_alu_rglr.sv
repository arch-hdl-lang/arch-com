// E203 Regular ALU Sub-unit
// Pure combinational: decodes info bits, selects operands (rs1/pc, rs2/imm),
// routes to shared ALU datapath, passes result back. ecall/ebreak/wfi set error.
module e203_exu_alu_rglr (
  input logic clk,
  input logic rst_n,
  input logic alu_i_valid,
  output logic alu_i_ready,
  input logic [32-1:0] alu_i_rs1,
  input logic [32-1:0] alu_i_rs2,
  input logic [32-1:0] alu_i_imm,
  input logic [32-1:0] alu_i_pc,
  input logic [21-1:0] alu_i_info,
  output logic alu_o_valid,
  input logic alu_o_ready,
  output logic [32-1:0] alu_o_wbck_wdat,
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
  output logic [32-1:0] alu_req_alu_op1,
  output logic [32-1:0] alu_req_alu_op2,
  input logic [32-1:0] alu_req_alu_res
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
