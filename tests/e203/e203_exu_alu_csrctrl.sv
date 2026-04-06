// E203 CSR Control Sub-unit
// Pure combinational: executes CSRRW/CSRRS/CSRRC instructions.
// Computes new CSR value from set/clear/write semantics.
module e203_exu_alu_csrctrl (
  input logic clk,
  input logic rst_n,
  input logic csr_i_valid,
  output logic csr_i_ready,
  input logic [32-1:0] csr_i_rs1,
  input logic [26-1:0] csr_i_info,
  input logic csr_i_rdwen,
  output logic csr_ena,
  output logic csr_wr_en,
  output logic csr_rd_en,
  output logic [12-1:0] csr_idx,
  input logic csr_access_ilgl,
  input logic [32-1:0] read_csr_dat,
  output logic [32-1:0] wbck_csr_dat,
  output logic csr_o_valid,
  input logic csr_o_ready,
  output logic [32-1:0] csr_o_wbck_wdat,
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
  logic [5-1:0] zimm;
  assign zimm = csr_i_info[12:8];
  logic rs1is0;
  assign rs1is0 = csr_i_info[13:13];
  logic [12-1:0] csridx;
  assign csridx = csr_i_info[25:14];
  // Operand: zero-extended zimm or rs1
  logic [32-1:0] csr_op1;
  assign csr_op1 = rs1imm ? 32'($unsigned(zimm)) : csr_i_rs1;
  assign csr_o_valid = csr_i_valid;
  assign csr_i_ready = csr_o_ready;
  assign csr_o_wbck_wdat = read_csr_dat;
  assign csr_o_wbck_err = csr_access_ilgl;
  assign csr_idx = csridx;
  assign csr_ena = csr_o_valid & csr_o_ready;
  assign csr_rd_en = csr_i_valid & (csrrw & csr_i_rdwen | csrrs | csrrc);
  assign csr_wr_en = csr_i_valid & (csrrw | (csrrs | csrrc) & ~rs1is0);
  assign wbck_csr_dat = csr_op1 & {32{csrrw}} | (csr_op1 | read_csr_dat) & {32{csrrs}} | ~csr_op1 & read_csr_dat & {32{csrrc}};

endmodule

// Pass-through handshake
// Writeback to register file = CSR read data
// CSR index passthrough
// CSR enable: fire when handshake completes
// Read enable: CSRRW reads only if rd written; CSRRS/CSRRC always read
// Write enable: CSRRW always writes; CSRRS/CSRRC write only if rs1 != x0
// Write data: CSRRW=direct, CSRRS=set bits, CSRRC=clear bits
