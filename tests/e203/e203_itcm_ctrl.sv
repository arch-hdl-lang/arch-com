// E203 ITCM Controller
// Arbitrates IFU (64-bit), LSU (32-bit, width-converted), and EXT (32-bit) access to ITCM SRAM.
// Priority: EXT > LSU > IFU.  1-cycle SRAM latency, response tagged with requester ID.
//
// Pipeline: LSU/EXT → n2w(addr[2] pipe) → arbt(priority, rspid pipe) → sram_ctrl(bypbuf+usr pipe) → RAM
// Response: RAM → usr pipe → demux(IFU vs arbt) → rspid demux(EXT vs LSU) → n2w extract → output
module e203_itcm_ctrl (
  input logic clk,
  input logic rst_n,
  input logic test_mode,
  input logic tcm_cgstop,
  output logic itcm_active,
  input logic ifu2itcm_icb_cmd_valid,
  output logic ifu2itcm_icb_cmd_ready,
  input logic [15:0] ifu2itcm_icb_cmd_addr,
  input logic ifu2itcm_icb_cmd_read,
  input logic [63:0] ifu2itcm_icb_cmd_wdata,
  input logic [7:0] ifu2itcm_icb_cmd_wmask,
  output logic ifu2itcm_icb_rsp_valid,
  input logic ifu2itcm_icb_rsp_ready,
  output logic ifu2itcm_icb_rsp_err,
  output logic [63:0] ifu2itcm_icb_rsp_rdata,
  output logic ifu2itcm_holdup,
  input logic lsu2itcm_icb_cmd_valid,
  output logic lsu2itcm_icb_cmd_ready,
  input logic [15:0] lsu2itcm_icb_cmd_addr,
  input logic lsu2itcm_icb_cmd_read,
  input logic [31:0] lsu2itcm_icb_cmd_wdata,
  input logic [3:0] lsu2itcm_icb_cmd_wmask,
  output logic lsu2itcm_icb_rsp_valid,
  input logic lsu2itcm_icb_rsp_ready,
  output logic lsu2itcm_icb_rsp_err,
  output logic [31:0] lsu2itcm_icb_rsp_rdata,
  input logic ext2itcm_icb_cmd_valid,
  output logic ext2itcm_icb_cmd_ready,
  input logic [15:0] ext2itcm_icb_cmd_addr,
  input logic ext2itcm_icb_cmd_read,
  input logic [31:0] ext2itcm_icb_cmd_wdata,
  input logic [3:0] ext2itcm_icb_cmd_wmask,
  output logic ext2itcm_icb_rsp_valid,
  input logic ext2itcm_icb_rsp_ready,
  output logic ext2itcm_icb_rsp_err,
  output logic [31:0] ext2itcm_icb_rsp_rdata,
  output logic itcm_ram_cs,
  output logic itcm_ram_we,
  output logic [12:0] itcm_ram_addr,
  output logic [7:0] itcm_ram_wem,
  output logic [63:0] itcm_ram_din,
  input logic [63:0] itcm_ram_dout,
  output logic clk_itcm_ram
);

  // IFU ICB (64-bit)
  // LSU ICB (32-bit)
  // External debug ICB command
  // External debug ICB response
  // SRAM interface (64-bit)
  // === Pipeline registers ===
  // Stage 0: n2w addr[2] pipe (DP=1, CUT_READY=0)
  logic lsu_n2w_vld_r;
  logic lsu_n2w_lo_r;
  logic ext_n2w_vld_r;
  logic ext_n2w_lo_r;
  // Stage 1: arbt rspid pipe (DP=1, CUT_READY=0). arbt_rspid_r: 0=EXT, 1=LSU
  logic arbt_rspid_vld_r;
  logic arbt_rspid_r;
  // Stage 2: bypbuf internal FIFO (DP=1, CUT_READY=1)
  logic byp_vld_r;
  logic byp_read_r;
  logic byp_ifu_r;
  logic [15:0] byp_addr_r;
  logic [63:0] byp_wdata_r;
  logic [7:0] byp_wmask_r;
  // Stage 3: usr pipe (DP=1, CUT_READY=0) for 1-cycle SRAM response
  logic usr_vld_r;
  logic usr_ifu_r;
  logic usr_read_r;
  // IFU holdup tracking
  logic ifu_holdup_r;
  // Latch-based clock gate model: enb captured when clk=0
  logic clkgate_enb_r;
  // === LSU n2w: 32→64 width conversion ===
  // wdata duplicated across both halves (reference sirv_gnrl_icb_n2w line 629)
  logic [63:0] lsu_n2w_wdata;
  assign lsu_n2w_wdata = {lsu2itcm_icb_cmd_wdata, lsu2itcm_icb_cmd_wdata};
  // wmask shifted based on addr[2] (reference line 630)
  logic [7:0] lsu_n2w_wmask;
  assign lsu_n2w_wmask = lsu2itcm_icb_cmd_addr[2] ? {lsu2itcm_icb_cmd_wmask, 4'd0} : {4'd0, lsu2itcm_icb_cmd_wmask};
  // n2w pipe full: can't accept when vld_r=1 and not popping
  logic lsu_n2w_rsp_hs;
  assign lsu_n2w_rsp_hs = lsu_n2w_vld_r & lsu2itcm_icb_rsp_valid & lsu2itcm_icb_rsp_ready;
  logic lsu_n2w_full;
  assign lsu_n2w_full = lsu_n2w_vld_r & ~lsu_n2w_rsp_hs;
  // n2w output valid (combinational pass-through when not full)
  logic lsu_n2w_cmd_valid;
  assign lsu_n2w_cmd_valid = ~lsu_n2w_full & lsu2itcm_icb_cmd_valid;
  // lsu_n2w cmd_ready comes from arbt back-pressure
  logic lsu_n2w_cmd_ready;
  // === EXT n2w: same pattern ===
  logic [63:0] ext_n2w_wdata;
  assign ext_n2w_wdata = {ext2itcm_icb_cmd_wdata, ext2itcm_icb_cmd_wdata};
  logic [7:0] ext_n2w_wmask;
  assign ext_n2w_wmask = ext2itcm_icb_cmd_addr[2] ? {ext2itcm_icb_cmd_wmask, 4'd0} : {4'd0, ext2itcm_icb_cmd_wmask};
  logic ext_n2w_rsp_hs;
  assign ext_n2w_rsp_hs = ext_n2w_vld_r & ext2itcm_icb_rsp_valid & ext2itcm_icb_rsp_ready;
  logic ext_n2w_full;
  assign ext_n2w_full = ext_n2w_vld_r & ~ext_n2w_rsp_hs;
  logic ext_n2w_cmd_valid;
  assign ext_n2w_cmd_valid = ~ext_n2w_full & ext2itcm_icb_cmd_valid;
  logic ext_n2w_cmd_ready;
  // === Arbiter: priority EXT(port0) > LSU(port1), ALLOW_0CYCL_RSP=0 ===
  // Priority grants (reference sirv_gnrl_icb_arbt lines 201-207)
  logic arbt_ext_grt;
  assign arbt_ext_grt = 1'b1;
  logic arbt_lsu_grt;
  assign arbt_lsu_grt = ~ext_n2w_cmd_valid;
  // arbt rspid pipe full (DP=1, CUT_READY=0)
  // rspid pops when: arbt sees rsp valid, rspid has entry, selected port ready
  logic arbt_rspid_ren;
  assign arbt_rspid_ren = usr_vld_r & ~usr_ifu_r & arbt_rspid_vld_r & (arbt_rspid_r ? lsu2itcm_icb_rsp_ready : ext2itcm_icb_rsp_ready);
  logic arbt_rspid_full;
  assign arbt_rspid_full = arbt_rspid_vld_r & ~arbt_rspid_ren;
  // arbt cmd output (gated by rspid_full)
  logic arbt_cmd_valid_real;
  assign arbt_cmd_valid_real = ext_n2w_cmd_valid | lsu_n2w_cmd_valid;
  logic arbt_cmd_valid;
  assign arbt_cmd_valid = arbt_cmd_valid_real & ~arbt_rspid_full;
  // arbt selected port ID (for rspid write)
  logic arbt_sel_port;
  assign arbt_sel_port = ext_n2w_cmd_valid ? 1'b0 : 1'b1;
  // 0=EXT, 1=LSU
  // arbt cmd mux outputs
  logic arbt_cmd_read;
  assign arbt_cmd_read = ext_n2w_cmd_valid ? ext2itcm_icb_cmd_read : lsu2itcm_icb_cmd_read;
  logic [15:0] arbt_cmd_addr;
  assign arbt_cmd_addr = ext_n2w_cmd_valid ? ext2itcm_icb_cmd_addr : lsu2itcm_icb_cmd_addr;
  logic [63:0] arbt_cmd_wdata;
  assign arbt_cmd_wdata = ext_n2w_cmd_valid ? ext_n2w_wdata : lsu_n2w_wdata;
  logic [7:0] arbt_cmd_wmask;
  assign arbt_cmd_wmask = ext_n2w_cmd_valid ? ext_n2w_wmask : lsu_n2w_wmask;
  // === IFU vs arbt selection (reference ref_e203_itcm_ctrl lines 426-459) ===
  // IFU only proceeds when arbt output is NOT valid
  logic sram_ready2ifu;
  assign sram_ready2ifu = ~arbt_cmd_valid;
  logic sram_ready2arbt;
  assign sram_ready2arbt = 1'b1;
  logic sram_sel_ifu;
  assign sram_sel_ifu = sram_ready2ifu & ifu2itcm_icb_cmd_valid;
  logic sram_sel_arbt;
  assign sram_sel_arbt = sram_ready2arbt & arbt_cmd_valid;
  // Selected SRAM command
  logic sram_cmd_valid;
  assign sram_cmd_valid = (sram_sel_ifu & ifu2itcm_icb_cmd_valid) | (sram_sel_arbt & arbt_cmd_valid);
  logic sram_cmd_ifu;
  assign sram_cmd_ifu = sram_sel_ifu;
  logic sram_cmd_read;
  assign sram_cmd_read = sram_sel_ifu ? ifu2itcm_icb_cmd_read : arbt_cmd_read;
  logic [15:0] sram_cmd_addr;
  assign sram_cmd_addr = sram_sel_ifu ? ifu2itcm_icb_cmd_addr : arbt_cmd_addr;
  logic [63:0] sram_cmd_wdata;
  assign sram_cmd_wdata = sram_sel_ifu ? ifu2itcm_icb_cmd_wdata : arbt_cmd_wdata;
  logic [7:0] sram_cmd_wmask;
  assign sram_cmd_wmask = sram_sel_ifu ? ifu2itcm_icb_cmd_wmask : arbt_cmd_wmask;
  // === Bypbuf (DP=1, CUT_READY=1 internal): bypass when empty and downstream ready ===
  // bypbuf i_rdy = fifo_i_rdy = ~byp_vld_r (CUT_READY=1 for DP=1)
  logic byp_i_rdy;
  assign byp_i_rdy = ~byp_vld_r;
  // usr pipe i_rdy (DP=1, CUT_READY=0)
  // usr_rsp_hs is the response handshake that pops the usr pipe
  logic usr_rsp_hs;
  assign usr_rsp_hs = usr_vld_r & (usr_ifu_r ? ifu2itcm_icb_rsp_ready : arbt_rspid_vld_r & (arbt_rspid_r ? lsu2itcm_icb_rsp_ready : ext2itcm_icb_rsp_ready));
  logic usr_i_rdy;
  assign usr_i_rdy = ~usr_vld_r | usr_rsp_hs;
  // bypbuf bypass: i_vld & o_rdy & ~fifo_o_vld
  logic byp_bypass;
  assign byp_bypass = sram_cmd_valid & usr_i_rdy & ~byp_vld_r;
  // bypbuf o_vld
  logic byp_o_vld;
  assign byp_o_vld = byp_vld_r | sram_cmd_valid;
  // Selected cmd for usr pipe input
  logic uop_cmd_valid;
  assign uop_cmd_valid = byp_o_vld;
  logic uop_cmd_read;
  assign uop_cmd_read = byp_vld_r ? byp_read_r : sram_cmd_read;
  logic uop_cmd_ifu;
  assign uop_cmd_ifu = byp_vld_r ? byp_ifu_r : sram_cmd_ifu;
  logic [15:0] uop_cmd_addr;
  assign uop_cmd_addr = byp_vld_r ? byp_addr_r : sram_cmd_addr;
  logic [63:0] uop_cmd_wdata;
  assign uop_cmd_wdata = byp_vld_r ? byp_wdata_r : sram_cmd_wdata;
  logic [7:0] uop_cmd_wmask;
  assign uop_cmd_wmask = byp_vld_r ? byp_wmask_r : sram_cmd_wmask;
  // === RAM interface (reference sirv_1cyc_sram_ctrl) ===
  logic ram_cs;
  assign ram_cs = uop_cmd_valid & usr_i_rdy;
  logic ram_we;
  assign ram_we = ~uop_cmd_read;
  // === Clock gate: latch model (reference e203_clkgate) ===
  // Latch captures (clock_en | test_mode) when clk=0; clk_out = enb & clk.
  // Modeled as registered enable (1-cycle delay from latch behavior at posedge).
  // === Response data demux ===
  // LSU rsp data: extract 32-bit half based on stored addr[2] (reference n2w line 635)
  logic [31:0] lsu_rsp_rdata;
  assign lsu_rsp_rdata = lsu_n2w_lo_r ? itcm_ram_dout[63:32] : itcm_ram_dout[31:0];
  logic [31:0] ext_rsp_rdata;
  assign ext_rsp_rdata = ext_n2w_lo_r ? itcm_ram_dout[63:32] : itcm_ram_dout[31:0];
  // === IFU holdup (reference ref_e203_itcm_ctrl lines 553-559) ===
  logic ifu_holdup_set;
  assign ifu_holdup_set = sram_cmd_ifu & ram_cs;
  logic ifu_holdup_clr;
  assign ifu_holdup_clr = ~sram_cmd_ifu & ram_cs;
  // === Activity (reference line 563) ===
  logic sram_ctrl_active;
  assign sram_ctrl_active = sram_cmd_valid | byp_vld_r | (uop_cmd_valid & usr_i_rdy) | usr_vld_r;
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      arbt_rspid_r <= 1'b0;
      arbt_rspid_vld_r <= 1'b0;
      byp_addr_r <= 0;
      byp_ifu_r <= 1'b0;
      byp_read_r <= 1'b0;
      byp_vld_r <= 1'b0;
      byp_wdata_r <= 0;
      byp_wmask_r <= 0;
      clkgate_enb_r <= 1'b0;
      ext_n2w_lo_r <= 1'b0;
      ext_n2w_vld_r <= 1'b0;
      ifu_holdup_r <= 1'b0;
      lsu_n2w_lo_r <= 1'b0;
      lsu_n2w_vld_r <= 1'b0;
      usr_ifu_r <= 1'b0;
      usr_read_r <= 1'b0;
      usr_vld_r <= 1'b0;
    end else begin
      // --- LSU n2w pipe: store addr[2] on cmd handshake, clear on rsp handshake ---
      if (lsu_n2w_cmd_valid & lsu_n2w_cmd_ready) begin
        lsu_n2w_vld_r <= 1'b1;
        lsu_n2w_lo_r <= lsu2itcm_icb_cmd_addr[2];
      end else if (lsu_n2w_rsp_hs) begin
        lsu_n2w_vld_r <= 1'b0;
      end
      // --- EXT n2w pipe ---
      if (ext_n2w_cmd_valid & ext_n2w_cmd_ready) begin
        ext_n2w_vld_r <= 1'b1;
        ext_n2w_lo_r <= ext2itcm_icb_cmd_addr[2];
      end else if (ext_n2w_rsp_hs) begin
        ext_n2w_vld_r <= 1'b0;
      end
      // --- Arbt rspid pipe: store port_id on cmd handshake, clear on rsp ---
      if (arbt_cmd_valid & byp_i_rdy & sram_sel_arbt) begin
        arbt_rspid_vld_r <= 1'b1;
        arbt_rspid_r <= arbt_sel_port;
      end else if (arbt_rspid_ren) begin
        arbt_rspid_vld_r <= 1'b0;
      end
      // --- Bypbuf FIFO: store cmd when not bypassing ---
      if (sram_cmd_valid & byp_i_rdy & ~byp_bypass) begin
        byp_vld_r <= 1'b1;
        byp_read_r <= sram_cmd_read;
        byp_ifu_r <= sram_cmd_ifu;
        byp_addr_r <= sram_cmd_addr;
        byp_wdata_r <= sram_cmd_wdata;
        byp_wmask_r <= sram_cmd_wmask;
      end else if (byp_vld_r & usr_i_rdy & ~byp_bypass) begin
        // Pop bypbuf when not bypassing and downstream accepts
        byp_vld_r <= 1'b0;
      end
      // --- Usr pipe: 1-cycle SRAM response delay ---
      if (uop_cmd_valid & usr_i_rdy) begin
        usr_vld_r <= 1'b1;
        usr_ifu_r <= uop_cmd_ifu;
        usr_read_r <= uop_cmd_read;
      end else if (usr_rsp_hs) begin
        usr_vld_r <= 1'b0;
      end
      // --- IFU holdup ---
      if (ifu_holdup_set | ifu_holdup_clr) begin
        ifu_holdup_r <= ifu_holdup_set & ~ifu_holdup_clr;
      end
      // --- Clock gate latch model: capture enable when clk=0 ---
      clkgate_enb_r <= ram_cs | tcm_cgstop | test_mode;
    end
  end
  // arbt cmd_ready_real: what the arbt passes back to selected input port
  logic arbt_cmd_ready_real;
  assign arbt_cmd_ready_real = byp_i_rdy & ~arbt_rspid_full;
  assign lsu_n2w_cmd_ready = arbt_lsu_grt & arbt_cmd_ready_real;
  assign ext_n2w_cmd_ready = arbt_ext_grt & arbt_cmd_ready_real;
  assign lsu2itcm_icb_cmd_ready = ~lsu_n2w_full & lsu_n2w_cmd_ready;
  assign ext2itcm_icb_cmd_ready = ~ext_n2w_full & ext_n2w_cmd_ready;
  assign ifu2itcm_icb_cmd_ready = sram_ready2ifu & byp_i_rdy;
  assign itcm_ram_cs = ram_cs;
  assign itcm_ram_we = ram_we;
  assign itcm_ram_addr = uop_cmd_addr[15:3];
  assign itcm_ram_wem = uop_cmd_wmask;
  assign itcm_ram_din = uop_cmd_wdata;
  assign clk_itcm_ram = clkgate_enb_r;
  assign ifu2itcm_icb_rsp_valid = usr_vld_r & usr_ifu_r;
  assign ifu2itcm_icb_rsp_err = 1'b0;
  assign ifu2itcm_icb_rsp_rdata = itcm_ram_dout;
  assign lsu2itcm_icb_rsp_valid = arbt_rspid_vld_r & usr_vld_r & ~usr_ifu_r & arbt_rspid_r;
  assign lsu2itcm_icb_rsp_err = 1'b0;
  assign lsu2itcm_icb_rsp_rdata = lsu_rsp_rdata;
  assign ext2itcm_icb_rsp_valid = arbt_rspid_vld_r & usr_vld_r & ~usr_ifu_r & ~arbt_rspid_r;
  assign ext2itcm_icb_rsp_err = 1'b0;
  assign ext2itcm_icb_rsp_rdata = ext_rsp_rdata;
  assign ifu2itcm_holdup = ifu_holdup_r;
  assign itcm_active = ifu2itcm_icb_cmd_valid | lsu2itcm_icb_cmd_valid | ext2itcm_icb_cmd_valid | sram_ctrl_active;

endmodule

// --- Input cmd_ready ---
// --- RAM outputs ---
// --- Clock output (latched: 1-cycle delayed enable) ---
// --- Response outputs ---
// IFU response (direct)
// LSU response (through arbt demux + n2w extract)
// arbt_rsp_valid_pre = (~rspid_empty) & o_icb_rsp_valid = arbt_rspid_vld_r & usr_vld_r & ~usr_ifu_r
// EXT response
// IFU holdup
// Activity
