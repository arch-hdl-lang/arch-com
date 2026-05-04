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
