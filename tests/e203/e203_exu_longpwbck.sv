// E203 Long-Pipe Writeback Collector
// Collects results from LSU and NICE coprocessor, arbitrates into
// a single writeback port and exception port.
// Matches RealBench port interface.
module e203_exu_longpwbck (
  input logic clk,
  input logic rst_n,
  input logic lsu_wbck_i_valid,
  output logic lsu_wbck_i_ready,
  input logic [32-1:0] lsu_wbck_i_wdat,
  input logic [1-1:0] lsu_wbck_i_itag,
  input logic lsu_wbck_i_err,
  input logic lsu_cmt_i_buserr,
  input logic [32-1:0] lsu_cmt_i_badaddr,
  input logic lsu_cmt_i_ld,
  input logic lsu_cmt_i_st,
  output logic longp_wbck_o_valid,
  input logic longp_wbck_o_ready,
  output logic [32-1:0] longp_wbck_o_wdat,
  output logic [5-1:0] longp_wbck_o_flags,
  output logic [5-1:0] longp_wbck_o_rdidx,
  output logic longp_wbck_o_rdfpu,
  output logic longp_excp_o_valid,
  input logic longp_excp_o_ready,
  output logic longp_excp_o_insterr,
  output logic longp_excp_o_ld,
  output logic longp_excp_o_st,
  output logic longp_excp_o_buserr,
  output logic [32-1:0] longp_excp_o_badaddr,
  output logic [32-1:0] longp_excp_o_pc,
  input logic oitf_empty,
  input logic [1-1:0] oitf_ret_ptr,
  input logic [5-1:0] oitf_ret_rdidx,
  input logic [32-1:0] oitf_ret_pc,
  input logic oitf_ret_rdwen,
  input logic oitf_ret_rdfpu,
  output logic oitf_ret_ena,
  input logic nice_longp_wbck_i_valid,
  output logic nice_longp_wbck_i_ready,
  input logic [32-1:0] nice_longp_wbck_i_wdat,
  input logic [1-1:0] nice_longp_wbck_i_itag,
  input logic nice_longp_wbck_i_err
);

  // ── LSU writeback input ───────────────────────────────────────────
  // ── LSU commit info ───────────────────────────────────────────────
  // ── Merged writeback output ───────────────────────────────────────
  // ── Exception output ──────────────────────────────────────────────
  // ── OITF interface ────────────────────────────────────────────────
  // ── NICE writeback input ──────────────────────────────────────────
  // LSU has priority over NICE
  logic lsu_win;
  assign lsu_win = lsu_wbck_i_valid;
  logic sel_valid;
  assign sel_valid = lsu_wbck_i_valid | nice_longp_wbck_i_valid;
  logic [32-1:0] sel_wdat;
  assign sel_wdat = lsu_win ? lsu_wbck_i_wdat : nice_longp_wbck_i_wdat;
  logic sel_err;
  assign sel_err = lsu_win ? lsu_wbck_i_err : nice_longp_wbck_i_err;
  assign longp_wbck_o_valid = sel_valid & ~sel_err;
  assign longp_wbck_o_wdat = sel_wdat;
  assign longp_wbck_o_rdidx = oitf_ret_rdidx;
  assign longp_wbck_o_rdfpu = oitf_ret_rdfpu;
  assign longp_wbck_o_flags = 0;
  assign longp_excp_o_valid = sel_valid & sel_err;
  assign longp_excp_o_insterr = 1'b0;
  assign longp_excp_o_ld = lsu_cmt_i_ld;
  assign longp_excp_o_st = lsu_cmt_i_st;
  assign longp_excp_o_buserr = lsu_cmt_i_buserr;
  assign longp_excp_o_badaddr = lsu_cmt_i_badaddr;
  assign longp_excp_o_pc = oitf_ret_pc;
  assign lsu_wbck_i_ready = lsu_win & (longp_wbck_o_ready | sel_err);
  assign nice_longp_wbck_i_ready = ~lsu_win & (longp_wbck_o_ready | sel_err);
  assign oitf_ret_ena = sel_valid & (longp_wbck_o_ready | sel_err);

endmodule

// Writeback output
// Exception output
// Handshake: grant to winner, retire OITF entry
