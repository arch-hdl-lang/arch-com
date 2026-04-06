// E203 Load/Store Unit (Wrapper)
// Passes all ports through to e203_lsu_ctrl.
// lsu_active = ctrl_active | excp_active.
module e203_lsu (
  input logic clk,
  input logic rst_n,
  input logic commit_mret,
  input logic commit_trap,
  input logic excp_active,
  output logic lsu_active,
  input logic [32-1:0] itcm_region_indic,
  input logic [32-1:0] dtcm_region_indic,
  output logic lsu_o_valid,
  input logic lsu_o_ready,
  output logic [32-1:0] lsu_o_wbck_wdat,
  output logic [1-1:0] lsu_o_wbck_itag,
  output logic lsu_o_wbck_err,
  output logic lsu_o_cmt_ld,
  output logic lsu_o_cmt_st,
  output logic [32-1:0] lsu_o_cmt_badaddr,
  output logic lsu_o_cmt_buserr,
  input logic agu_icb_cmd_valid,
  output logic agu_icb_cmd_ready,
  input logic [32-1:0] agu_icb_cmd_addr,
  input logic agu_icb_cmd_read,
  input logic [32-1:0] agu_icb_cmd_wdata,
  input logic [4-1:0] agu_icb_cmd_wmask,
  input logic agu_icb_cmd_lock,
  input logic agu_icb_cmd_excl,
  input logic [2-1:0] agu_icb_cmd_size,
  input logic agu_icb_cmd_back2agu,
  input logic agu_icb_cmd_usign,
  input logic [1-1:0] agu_icb_cmd_itag,
  output logic agu_icb_rsp_valid,
  input logic agu_icb_rsp_ready,
  output logic agu_icb_rsp_err,
  output logic agu_icb_rsp_excl_ok,
  output logic [32-1:0] agu_icb_rsp_rdata,
  output logic itcm_icb_cmd_valid,
  input logic itcm_icb_cmd_ready,
  output logic [16-1:0] itcm_icb_cmd_addr,
  output logic itcm_icb_cmd_read,
  output logic [32-1:0] itcm_icb_cmd_wdata,
  output logic [4-1:0] itcm_icb_cmd_wmask,
  output logic itcm_icb_cmd_lock,
  output logic itcm_icb_cmd_excl,
  output logic [2-1:0] itcm_icb_cmd_size,
  input logic itcm_icb_rsp_valid,
  output logic itcm_icb_rsp_ready,
  input logic itcm_icb_rsp_err,
  input logic itcm_icb_rsp_excl_ok,
  input logic [32-1:0] itcm_icb_rsp_rdata,
  output logic dtcm_icb_cmd_valid,
  input logic dtcm_icb_cmd_ready,
  output logic [16-1:0] dtcm_icb_cmd_addr,
  output logic dtcm_icb_cmd_read,
  output logic [32-1:0] dtcm_icb_cmd_wdata,
  output logic [4-1:0] dtcm_icb_cmd_wmask,
  output logic dtcm_icb_cmd_lock,
  output logic dtcm_icb_cmd_excl,
  output logic [2-1:0] dtcm_icb_cmd_size,
  input logic dtcm_icb_rsp_valid,
  output logic dtcm_icb_rsp_ready,
  input logic dtcm_icb_rsp_err,
  input logic dtcm_icb_rsp_excl_ok,
  input logic [32-1:0] dtcm_icb_rsp_rdata,
  output logic biu_icb_cmd_valid,
  input logic biu_icb_cmd_ready,
  output logic [32-1:0] biu_icb_cmd_addr,
  output logic biu_icb_cmd_read,
  output logic [32-1:0] biu_icb_cmd_wdata,
  output logic [4-1:0] biu_icb_cmd_wmask,
  output logic biu_icb_cmd_lock,
  output logic biu_icb_cmd_excl,
  output logic [2-1:0] biu_icb_cmd_size,
  input logic biu_icb_rsp_valid,
  output logic biu_icb_rsp_ready,
  input logic biu_icb_rsp_err,
  input logic biu_icb_rsp_excl_ok,
  input logic [32-1:0] biu_icb_rsp_rdata,
  input logic nice_mem_holdup,
  input logic nice_icb_cmd_valid,
  output logic nice_icb_cmd_ready,
  input logic [32-1:0] nice_icb_cmd_addr,
  input logic nice_icb_cmd_read,
  input logic [32-1:0] nice_icb_cmd_wdata,
  input logic [4-1:0] nice_icb_cmd_wmask,
  input logic nice_icb_cmd_lock,
  input logic nice_icb_cmd_excl,
  input logic [2-1:0] nice_icb_cmd_size,
  output logic nice_icb_rsp_valid,
  input logic nice_icb_rsp_ready,
  output logic nice_icb_rsp_err,
  output logic nice_icb_rsp_excl_ok,
  output logic [32-1:0] nice_icb_rsp_rdata
);

  // Memory region indicators
  // LSU writeback
  // AGU ICB command
  // AGU ICB response
  // ITCM ICB
  // DTCM ICB
  // BIU ICB
  // NICE ICB passthrough
  // Address decode: route AGU ICB to ITCM, DTCM, or BIU
  logic addr_in_itcm;
  assign addr_in_itcm = agu_icb_cmd_addr[31:16] == itcm_region_indic[31:16];
  logic addr_in_dtcm;
  assign addr_in_dtcm = agu_icb_cmd_addr[31:16] == dtcm_region_indic[31:16];
  // Response tracking: remember which target was selected
  logic [2-1:0] rsp_sel_r;
  // 0=BIU, 1=ITCM, 2=DTCM
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      rsp_sel_r <= 0;
    end else begin
      if (agu_icb_cmd_valid & agu_icb_cmd_ready) begin
        rsp_sel_r <= addr_in_itcm ? 1 : addr_in_dtcm ? 2 : 0;
      end
    end
  end
  assign itcm_icb_cmd_valid = agu_icb_cmd_valid & addr_in_itcm;
  assign dtcm_icb_cmd_valid = agu_icb_cmd_valid & addr_in_dtcm;
  assign biu_icb_cmd_valid = agu_icb_cmd_valid & ~addr_in_itcm & ~addr_in_dtcm;
  assign agu_icb_cmd_ready = addr_in_itcm ? itcm_icb_cmd_ready : addr_in_dtcm ? dtcm_icb_cmd_ready : biu_icb_cmd_ready;
  assign itcm_icb_cmd_addr = agu_icb_cmd_addr[15:0];
  assign itcm_icb_cmd_read = agu_icb_cmd_read;
  assign itcm_icb_cmd_wdata = agu_icb_cmd_wdata;
  assign itcm_icb_cmd_wmask = agu_icb_cmd_wmask;
  assign itcm_icb_cmd_lock = agu_icb_cmd_lock;
  assign itcm_icb_cmd_excl = agu_icb_cmd_excl;
  assign itcm_icb_cmd_size = agu_icb_cmd_size;
  assign dtcm_icb_cmd_addr = agu_icb_cmd_addr[15:0];
  assign dtcm_icb_cmd_read = agu_icb_cmd_read;
  assign dtcm_icb_cmd_wdata = agu_icb_cmd_wdata;
  assign dtcm_icb_cmd_wmask = agu_icb_cmd_wmask;
  assign dtcm_icb_cmd_lock = agu_icb_cmd_lock;
  assign dtcm_icb_cmd_excl = agu_icb_cmd_excl;
  assign dtcm_icb_cmd_size = agu_icb_cmd_size;
  assign biu_icb_cmd_addr = agu_icb_cmd_addr;
  assign biu_icb_cmd_read = agu_icb_cmd_read;
  assign biu_icb_cmd_wdata = agu_icb_cmd_wdata;
  assign biu_icb_cmd_wmask = agu_icb_cmd_wmask;
  assign biu_icb_cmd_lock = agu_icb_cmd_lock;
  assign biu_icb_cmd_excl = agu_icb_cmd_excl;
  assign biu_icb_cmd_size = agu_icb_cmd_size;
  assign agu_icb_rsp_valid = rsp_sel_r == 1 ? itcm_icb_rsp_valid : rsp_sel_r == 2 ? dtcm_icb_rsp_valid : biu_icb_rsp_valid;
  assign agu_icb_rsp_err = rsp_sel_r == 1 ? itcm_icb_rsp_err : rsp_sel_r == 2 ? dtcm_icb_rsp_err : biu_icb_rsp_err;
  assign agu_icb_rsp_excl_ok = rsp_sel_r == 1 ? itcm_icb_rsp_excl_ok : rsp_sel_r == 2 ? dtcm_icb_rsp_excl_ok : biu_icb_rsp_excl_ok;
  assign agu_icb_rsp_rdata = rsp_sel_r == 1 ? itcm_icb_rsp_rdata : rsp_sel_r == 2 ? dtcm_icb_rsp_rdata : biu_icb_rsp_rdata;
  assign itcm_icb_rsp_ready = agu_icb_rsp_ready;
  assign dtcm_icb_rsp_ready = agu_icb_rsp_ready;
  assign biu_icb_rsp_ready = agu_icb_rsp_ready;
  assign lsu_o_valid = agu_icb_rsp_valid & agu_icb_cmd_back2agu;
  assign lsu_o_wbck_wdat = agu_icb_rsp_rdata;
  assign lsu_o_wbck_itag = agu_icb_cmd_itag;
  assign lsu_o_wbck_err = agu_icb_rsp_err;
  assign lsu_o_cmt_ld = 1'b0;
  assign lsu_o_cmt_st = 1'b0;
  assign lsu_o_cmt_badaddr = 0;
  assign lsu_o_cmt_buserr = 1'b0;
  assign nice_icb_cmd_ready = 1'b0;
  assign nice_icb_rsp_valid = 1'b0;
  assign nice_icb_rsp_err = 1'b0;
  assign nice_icb_rsp_excl_ok = 1'b0;
  assign nice_icb_rsp_rdata = 0;
  assign lsu_active = agu_icb_cmd_valid | rsp_sel_r != 0 | excp_active;

endmodule

// Command routing
// Shared command fields
// Response mux
// Writeback: passthrough from AGU response (load sign-extension in lsu_ctrl)
// NICE ICB passthrough (stub: tie off)
