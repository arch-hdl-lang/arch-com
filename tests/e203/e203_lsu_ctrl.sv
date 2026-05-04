// E203 HBirdv2 Load-Store Unit Controller
// Routes AGU and NICE ICB commands to DTCM, ITCM, or BIU based on address region.
// Handles commit events (mret, trap) and generates write-back/commit signals.
module e203_lsu_ctrl (
  input logic clk,
  input logic rst_n,
  input logic commit_mret,
  input logic commit_trap,
  output logic lsu_ctrl_active,
  input logic [31:0] itcm_region_indic,
  input logic [31:0] dtcm_region_indic,
  output logic lsu_o_valid,
  input logic lsu_o_ready,
  output logic [31:0] lsu_o_wbck_wdat,
  output logic lsu_o_wbck_itag,
  output logic lsu_o_wbck_err,
  output logic lsu_o_cmt_buserr,
  output logic [31:0] lsu_o_cmt_badaddr,
  output logic lsu_o_cmt_ld,
  output logic lsu_o_cmt_st,
  input logic agu_icb_cmd_valid,
  output logic agu_icb_cmd_ready,
  input logic [31:0] agu_icb_cmd_addr,
  input logic agu_icb_cmd_read,
  input logic [31:0] agu_icb_cmd_wdata,
  input logic [3:0] agu_icb_cmd_wmask,
  input logic agu_icb_cmd_lock,
  input logic agu_icb_cmd_excl,
  input logic [1:0] agu_icb_cmd_size,
  input logic agu_icb_cmd_back2agu,
  input logic agu_icb_cmd_usign,
  input logic agu_icb_cmd_itag,
  output logic agu_icb_rsp_valid,
  input logic agu_icb_rsp_ready,
  output logic agu_icb_rsp_err,
  output logic agu_icb_rsp_excl_ok,
  output logic [31:0] agu_icb_rsp_rdata,
  input logic nice_mem_holdup,
  input logic nice_icb_cmd_valid,
  output logic nice_icb_cmd_ready,
  input logic [31:0] nice_icb_cmd_addr,
  input logic nice_icb_cmd_read,
  input logic [31:0] nice_icb_cmd_wdata,
  input logic [3:0] nice_icb_cmd_wmask,
  input logic nice_icb_cmd_lock,
  input logic nice_icb_cmd_excl,
  input logic [1:0] nice_icb_cmd_size,
  output logic nice_icb_rsp_valid,
  input logic nice_icb_rsp_ready,
  output logic nice_icb_rsp_err,
  output logic nice_icb_rsp_excl_ok,
  output logic [31:0] nice_icb_rsp_rdata,
  output logic dtcm_icb_cmd_valid,
  input logic dtcm_icb_cmd_ready,
  output logic [15:0] dtcm_icb_cmd_addr,
  output logic dtcm_icb_cmd_read,
  output logic [31:0] dtcm_icb_cmd_wdata,
  output logic [3:0] dtcm_icb_cmd_wmask,
  output logic dtcm_icb_cmd_lock,
  output logic dtcm_icb_cmd_excl,
  output logic [1:0] dtcm_icb_cmd_size,
  input logic dtcm_icb_rsp_valid,
  output logic dtcm_icb_rsp_ready,
  input logic dtcm_icb_rsp_err,
  input logic dtcm_icb_rsp_excl_ok,
  input logic [31:0] dtcm_icb_rsp_rdata,
  output logic itcm_icb_cmd_valid,
  input logic itcm_icb_cmd_ready,
  output logic [15:0] itcm_icb_cmd_addr,
  output logic itcm_icb_cmd_read,
  output logic [31:0] itcm_icb_cmd_wdata,
  output logic [3:0] itcm_icb_cmd_wmask,
  output logic itcm_icb_cmd_lock,
  output logic itcm_icb_cmd_excl,
  output logic [1:0] itcm_icb_cmd_size,
  input logic itcm_icb_rsp_valid,
  output logic itcm_icb_rsp_ready,
  input logic itcm_icb_rsp_err,
  input logic itcm_icb_rsp_excl_ok,
  input logic [31:0] itcm_icb_rsp_rdata,
  output logic biu_icb_cmd_valid,
  input logic biu_icb_cmd_ready,
  output logic [31:0] biu_icb_cmd_addr,
  output logic biu_icb_cmd_read,
  output logic [31:0] biu_icb_cmd_wdata,
  output logic [3:0] biu_icb_cmd_wmask,
  output logic biu_icb_cmd_lock,
  output logic biu_icb_cmd_excl,
  output logic [1:0] biu_icb_cmd_size,
  input logic biu_icb_rsp_valid,
  output logic biu_icb_rsp_ready,
  input logic biu_icb_rsp_err,
  input logic biu_icb_rsp_excl_ok,
  input logic [31:0] biu_icb_rsp_rdata
);

  // Commit signals
  // Active status
  // Region indicators
  // LSU output (write-back)
  // AGU ICB command input
  // AGU ICB response output
  // NICE ICB command input
  // NICE ICB response output
  // DTCM ICB master
  // ITCM ICB master
  // BIU ICB master
  // ── Arbiter between AGU and NICE ─────────────────────────────────
  // AGU has priority over NICE; NICE only accepted when AGU not requesting
  // or when nice_mem_holdup forces it
  logic arb_cmd_valid;
  logic [31:0] arb_cmd_addr;
  logic arb_cmd_read;
  logic [31:0] arb_cmd_wdata;
  logic [3:0] arb_cmd_wmask;
  logic arb_cmd_lock;
  logic arb_cmd_excl;
  logic [1:0] arb_cmd_size;
  logic arb_cmd_back2agu;
  logic arb_cmd_usign;
  logic arb_cmd_itag;
  logic arb_cmd_ready;
  logic arb_is_nice;
  always_comb begin
    if (nice_mem_holdup & nice_icb_cmd_valid) begin
      arb_is_nice = 1'b1;
      arb_cmd_valid = nice_icb_cmd_valid;
      arb_cmd_addr = nice_icb_cmd_addr;
      arb_cmd_read = nice_icb_cmd_read;
      arb_cmd_wdata = nice_icb_cmd_wdata;
      arb_cmd_wmask = nice_icb_cmd_wmask;
      arb_cmd_lock = nice_icb_cmd_lock;
      arb_cmd_excl = nice_icb_cmd_excl;
      arb_cmd_size = nice_icb_cmd_size;
      arb_cmd_back2agu = 1'b0;
      arb_cmd_usign = 1'b0;
      arb_cmd_itag = 1'b0;
      nice_icb_cmd_ready = arb_cmd_ready;
      agu_icb_cmd_ready = 1'b0;
    end else if (agu_icb_cmd_valid) begin
      arb_is_nice = 1'b0;
      arb_cmd_valid = agu_icb_cmd_valid;
      arb_cmd_addr = agu_icb_cmd_addr;
      arb_cmd_read = agu_icb_cmd_read;
      arb_cmd_wdata = agu_icb_cmd_wdata;
      arb_cmd_wmask = agu_icb_cmd_wmask;
      arb_cmd_lock = agu_icb_cmd_lock;
      arb_cmd_excl = agu_icb_cmd_excl;
      arb_cmd_size = agu_icb_cmd_size;
      arb_cmd_back2agu = agu_icb_cmd_back2agu;
      arb_cmd_usign = agu_icb_cmd_usign;
      arb_cmd_itag = agu_icb_cmd_itag;
      agu_icb_cmd_ready = arb_cmd_ready;
      nice_icb_cmd_ready = 1'b0;
    end else if (nice_icb_cmd_valid) begin
      arb_is_nice = 1'b1;
      arb_cmd_valid = nice_icb_cmd_valid;
      arb_cmd_addr = nice_icb_cmd_addr;
      arb_cmd_read = nice_icb_cmd_read;
      arb_cmd_wdata = nice_icb_cmd_wdata;
      arb_cmd_wmask = nice_icb_cmd_wmask;
      arb_cmd_lock = nice_icb_cmd_lock;
      arb_cmd_excl = nice_icb_cmd_excl;
      arb_cmd_size = nice_icb_cmd_size;
      arb_cmd_back2agu = 1'b0;
      arb_cmd_usign = 1'b0;
      arb_cmd_itag = 1'b0;
      nice_icb_cmd_ready = arb_cmd_ready;
      agu_icb_cmd_ready = 1'b0;
    end else begin
      arb_is_nice = 1'b0;
      arb_cmd_valid = 1'b0;
      arb_cmd_addr = 0;
      arb_cmd_read = 1'b0;
      arb_cmd_wdata = 0;
      arb_cmd_wmask = 0;
      arb_cmd_lock = 1'b0;
      arb_cmd_excl = 1'b0;
      arb_cmd_size = 0;
      arb_cmd_back2agu = 1'b0;
      arb_cmd_usign = 1'b0;
      arb_cmd_itag = 1'b0;
      agu_icb_cmd_ready = 1'b0;
      nice_icb_cmd_ready = 1'b0;
    end
  end
  // ── Address region decode ────────────────────────────────────────
  logic [15:0] addr_region;
  assign addr_region = arb_cmd_addr[31:16];
  logic is_dtcm;
  assign is_dtcm = addr_region == dtcm_region_indic[31:16];
  logic is_itcm;
  assign is_itcm = addr_region == itcm_region_indic[31:16];
  logic is_biu;
  assign is_biu = ~is_dtcm & ~is_itcm;
  // ── Command routing ──────────────────────────────────────────────
  assign dtcm_icb_cmd_valid = arb_cmd_valid & is_dtcm;
  assign dtcm_icb_cmd_addr = arb_cmd_addr[15:0];
  assign dtcm_icb_cmd_read = arb_cmd_read;
  assign dtcm_icb_cmd_wdata = arb_cmd_wdata;
  assign dtcm_icb_cmd_wmask = arb_cmd_wmask;
  assign dtcm_icb_cmd_lock = arb_cmd_lock;
  assign dtcm_icb_cmd_excl = arb_cmd_excl;
  assign dtcm_icb_cmd_size = arb_cmd_size;
  assign itcm_icb_cmd_valid = arb_cmd_valid & is_itcm;
  assign itcm_icb_cmd_addr = arb_cmd_addr[15:0];
  assign itcm_icb_cmd_read = arb_cmd_read;
  assign itcm_icb_cmd_wdata = arb_cmd_wdata;
  assign itcm_icb_cmd_wmask = arb_cmd_wmask;
  assign itcm_icb_cmd_lock = arb_cmd_lock;
  assign itcm_icb_cmd_excl = arb_cmd_excl;
  assign itcm_icb_cmd_size = arb_cmd_size;
  assign biu_icb_cmd_valid = arb_cmd_valid & is_biu;
  assign biu_icb_cmd_addr = arb_cmd_addr;
  assign biu_icb_cmd_read = arb_cmd_read;
  assign biu_icb_cmd_wdata = arb_cmd_wdata;
  assign biu_icb_cmd_wmask = arb_cmd_wmask;
  assign biu_icb_cmd_lock = arb_cmd_lock;
  assign biu_icb_cmd_excl = arb_cmd_excl;
  assign biu_icb_cmd_size = arb_cmd_size;
  assign arb_cmd_ready = (is_dtcm & dtcm_icb_cmd_ready) | (is_itcm & itcm_icb_cmd_ready) | (is_biu & biu_icb_cmd_ready);
  // DTCM
  // ITCM
  // BIU
  // Ready back to arbiter
  // ── Track outstanding request target for response routing ────────
  logic rsp_target_dtcm = 0;
  logic rsp_target_itcm = 0;
  logic rsp_target_biu = 0;
  logic rsp_is_nice = 0;
  logic rsp_back2agu = 0;
  logic rsp_usign = 0;
  logic rsp_itag = 0;
  logic rsp_read = 0;
  logic [31:0] rsp_addr = 0;
  logic cmd_fire;
  assign cmd_fire = arb_cmd_valid & arb_cmd_ready;
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      rsp_addr <= 0;
      rsp_back2agu <= 0;
      rsp_is_nice <= 0;
      rsp_itag <= 0;
      rsp_read <= 0;
      rsp_target_biu <= 0;
      rsp_target_dtcm <= 0;
      rsp_target_itcm <= 0;
      rsp_usign <= 0;
    end else begin
      if (cmd_fire) begin
        rsp_target_dtcm <= is_dtcm;
        rsp_target_itcm <= is_itcm;
        rsp_target_biu <= is_biu;
        rsp_is_nice <= arb_is_nice;
        rsp_back2agu <= arb_cmd_back2agu;
        rsp_usign <= arb_cmd_usign;
        rsp_itag <= arb_cmd_itag;
        rsp_read <= arb_cmd_read;
        rsp_addr <= arb_cmd_addr;
      end
    end
  end
  // ── Response mux ─────────────────────────────────────────────────
  logic rsp_valid_raw;
  logic rsp_err_raw;
  logic rsp_excl_ok_raw;
  logic [31:0] rsp_rdata_raw;
  always_comb begin
    if (rsp_target_dtcm) begin
      rsp_valid_raw = dtcm_icb_rsp_valid;
      rsp_err_raw = dtcm_icb_rsp_err;
      rsp_excl_ok_raw = dtcm_icb_rsp_excl_ok;
      rsp_rdata_raw = dtcm_icb_rsp_rdata;
      dtcm_icb_rsp_ready = agu_icb_rsp_ready | (rsp_is_nice & nice_icb_rsp_ready);
      itcm_icb_rsp_ready = 1'b0;
      biu_icb_rsp_ready = 1'b0;
    end else if (rsp_target_itcm) begin
      rsp_valid_raw = itcm_icb_rsp_valid;
      rsp_err_raw = itcm_icb_rsp_err;
      rsp_excl_ok_raw = itcm_icb_rsp_excl_ok;
      rsp_rdata_raw = itcm_icb_rsp_rdata;
      itcm_icb_rsp_ready = agu_icb_rsp_ready | (rsp_is_nice & nice_icb_rsp_ready);
      dtcm_icb_rsp_ready = 1'b0;
      biu_icb_rsp_ready = 1'b0;
    end else if (rsp_target_biu) begin
      rsp_valid_raw = biu_icb_rsp_valid;
      rsp_err_raw = biu_icb_rsp_err;
      rsp_excl_ok_raw = biu_icb_rsp_excl_ok;
      rsp_rdata_raw = biu_icb_rsp_rdata;
      biu_icb_rsp_ready = agu_icb_rsp_ready | (rsp_is_nice & nice_icb_rsp_ready);
      dtcm_icb_rsp_ready = 1'b0;
      itcm_icb_rsp_ready = 1'b0;
    end else begin
      rsp_valid_raw = 1'b0;
      rsp_err_raw = 1'b0;
      rsp_excl_ok_raw = 1'b0;
      rsp_rdata_raw = 0;
      dtcm_icb_rsp_ready = 1'b0;
      itcm_icb_rsp_ready = 1'b0;
      biu_icb_rsp_ready = 1'b0;
    end
  end
  // ── Route response to AGU or NICE ────────────────────────────────
  always_comb begin
    if (rsp_is_nice) begin
      nice_icb_rsp_valid = rsp_valid_raw;
      nice_icb_rsp_err = rsp_err_raw;
      nice_icb_rsp_excl_ok = rsp_excl_ok_raw;
      nice_icb_rsp_rdata = rsp_rdata_raw;
      agu_icb_rsp_valid = 1'b0;
      agu_icb_rsp_err = 1'b0;
      agu_icb_rsp_excl_ok = 1'b0;
      agu_icb_rsp_rdata = 0;
    end else begin
      agu_icb_rsp_valid = rsp_valid_raw;
      agu_icb_rsp_err = rsp_err_raw;
      agu_icb_rsp_excl_ok = rsp_excl_ok_raw;
      agu_icb_rsp_rdata = rsp_rdata_raw;
      nice_icb_rsp_valid = 1'b0;
      nice_icb_rsp_err = 1'b0;
      nice_icb_rsp_excl_ok = 1'b0;
      nice_icb_rsp_rdata = 0;
    end
  end
  // ── LSU output: write-back and commit ────────────────────────────
  assign lsu_o_valid = agu_icb_rsp_valid & ~rsp_is_nice;
  assign lsu_o_wbck_wdat = agu_icb_rsp_rdata;
  assign lsu_o_wbck_itag = rsp_itag;
  assign lsu_o_wbck_err = agu_icb_rsp_err;
  assign lsu_o_cmt_buserr = agu_icb_rsp_err;
  assign lsu_o_cmt_badaddr = rsp_addr;
  assign lsu_o_cmt_ld = rsp_read;
  assign lsu_o_cmt_st = ~rsp_read;
  assign lsu_ctrl_active = agu_icb_cmd_valid | nice_icb_cmd_valid | rsp_target_dtcm | rsp_target_itcm | rsp_target_biu;

endmodule

