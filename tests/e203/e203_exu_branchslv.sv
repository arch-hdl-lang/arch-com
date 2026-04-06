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
  input logic [32-1:0] cmt_i_pc,
  input logic [32-1:0] cmt_i_imm,
  input logic [32-1:0] csr_epc_r,
  input logic [32-1:0] csr_dpc_r,
  input logic nonalu_excpirq_flush_req_raw,
  input logic brchmis_flush_ack,
  output logic brchmis_flush_req,
  output logic [32-1:0] brchmis_flush_add_op1,
  output logic [32-1:0] brchmis_flush_add_op2,
  output logic [32-1:0] brchmis_flush_pc,
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
  assign need_flush = cmt_i_bjp & (cmt_i_bjp_prdt ^ cmt_i_bjp_rslv) | cmt_i_fencei | cmt_i_mret | cmt_i_dret;
  logic flush_req_pre;
  assign flush_req_pre = cmt_i_valid & need_flush;
  logic flush_ack_pre;
  assign flush_ack_pre = brchmis_flush_ack & ~nonalu_excpirq_flush_req_raw;
  logic [32-1:0] pc_incr;
  assign pc_incr = cmt_i_rv32 ? 4 : 2;
  assign brchmis_flush_req = flush_req_pre & ~nonalu_excpirq_flush_req_raw;
  assign brchmis_flush_add_op1 = cmt_i_dret ? csr_dpc_r : cmt_i_mret ? csr_epc_r : cmt_i_pc;
  assign brchmis_flush_add_op2 = cmt_i_dret ? 0 : cmt_i_mret ? 0 : cmt_i_fencei | cmt_i_bjp_prdt ? pc_incr : cmt_i_imm;
  assign brchmis_flush_pc = cmt_i_fencei | cmt_i_bjp & cmt_i_bjp_prdt ? 32'(cmt_i_pc + pc_incr) : cmt_i_bjp & ~cmt_i_bjp_prdt ? 32'(cmt_i_pc + cmt_i_imm) : cmt_i_dret ? csr_dpc_r : csr_epc_r;
  assign cmt_mret_ena = cmt_i_mret & brchmis_flush_req & brchmis_flush_ack;
  assign cmt_dret_ena = cmt_i_dret & brchmis_flush_req & brchmis_flush_ack;
  assign cmt_fencei_ena = cmt_i_fencei & brchmis_flush_req & brchmis_flush_ack;
  assign cmt_i_ready = ~is_branch | (need_flush ? flush_ack_pre : 1'b1) & ~nonalu_excpirq_flush_req_raw;

endmodule

// Flush target operands (for external adder)
// Pre-computed flush PC (timing boost path, matching reference priority)
// Commit enables: fire on flush handshake (req & ack)
// Ready: non-branch always ready; branch waits for flush ack
