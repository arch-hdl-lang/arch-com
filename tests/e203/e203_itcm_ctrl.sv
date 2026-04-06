// E203 ITCM Controller
// Arbitrates IFU (64-bit) and LSU (32-bit, width-converted) access to ITCM SRAM.
// IFU has lower priority (blocked when LSU active).
// 1-cycle SRAM latency, response tagged with requester ID.
module e203_itcm_ctrl (
  input logic clk,
  input logic rst_n,
  input logic test_mode,
  input logic tcm_cgstop,
  output logic itcm_active,
  input logic ifu2itcm_icb_cmd_valid,
  output logic ifu2itcm_icb_cmd_ready,
  input logic [16-1:0] ifu2itcm_icb_cmd_addr,
  input logic ifu2itcm_icb_cmd_read,
  input logic [64-1:0] ifu2itcm_icb_cmd_wdata,
  input logic [8-1:0] ifu2itcm_icb_cmd_wmask,
  output logic ifu2itcm_icb_rsp_valid,
  input logic ifu2itcm_icb_rsp_ready,
  output logic ifu2itcm_icb_rsp_err,
  output logic [64-1:0] ifu2itcm_icb_rsp_rdata,
  output logic ifu2itcm_holdup,
  input logic lsu2itcm_icb_cmd_valid,
  output logic lsu2itcm_icb_cmd_ready,
  input logic [16-1:0] lsu2itcm_icb_cmd_addr,
  input logic lsu2itcm_icb_cmd_read,
  input logic [32-1:0] lsu2itcm_icb_cmd_wdata,
  input logic [4-1:0] lsu2itcm_icb_cmd_wmask,
  output logic lsu2itcm_icb_rsp_valid,
  input logic lsu2itcm_icb_rsp_ready,
  output logic lsu2itcm_icb_rsp_err,
  output logic [32-1:0] lsu2itcm_icb_rsp_rdata,
  input logic ext2itcm_icb_cmd_valid,
  output logic ext2itcm_icb_cmd_ready,
  input logic [16-1:0] ext2itcm_icb_cmd_addr,
  input logic ext2itcm_icb_cmd_read,
  input logic [32-1:0] ext2itcm_icb_cmd_wdata,
  input logic [4-1:0] ext2itcm_icb_cmd_wmask,
  output logic ext2itcm_icb_rsp_valid,
  input logic ext2itcm_icb_rsp_ready,
  output logic ext2itcm_icb_rsp_err,
  output logic [32-1:0] ext2itcm_icb_rsp_rdata,
  output logic itcm_ram_cs,
  output logic itcm_ram_we,
  output logic [13-1:0] itcm_ram_addr,
  output logic [8-1:0] itcm_ram_wem,
  output logic [64-1:0] itcm_ram_din,
  input logic [64-1:0] itcm_ram_dout,
  output logic clk_itcm_ram
);

  // IFU ICB (64-bit)
  // LSU ICB (32-bit)
  // External debug ICB command
  // External debug ICB response
  // SRAM interface (64-bit)
  // Response pipeline: track which requester (IFU vs LSU) and address for LSU 32->64
  logic rsp_valid_r;
  logic rsp_ifu_r;
  logic rsp_read_r;
  logic rsp_lsu_lo_r;
  // LSU accessed low word
  // IFU holdup tracking
  logic ifu_holdup_r;
  // LSU width conversion: expand 32-bit cmd to 64-bit SRAM
  logic lsu_addr_lo;
  assign lsu_addr_lo = ~lsu2itcm_icb_cmd_addr[2:2];
  // Low word if addr[2]=0
  logic [64-1:0] lsu_wdata_64;
  assign lsu_wdata_64 = lsu_addr_lo ? {32'd0, lsu2itcm_icb_cmd_wdata} : {lsu2itcm_icb_cmd_wdata, 32'd0};
  logic [8-1:0] lsu_wmask_8;
  assign lsu_wmask_8 = lsu_addr_lo ? {4'd0, lsu2itcm_icb_cmd_wmask} : {lsu2itcm_icb_cmd_wmask, 4'd0};
  // Arbitration: LSU has priority over IFU
  logic sram_sel_lsu;
  assign sram_sel_lsu = lsu2itcm_icb_cmd_valid;
  logic sram_sel_ifu;
  assign sram_sel_ifu = ifu2itcm_icb_cmd_valid & ~lsu2itcm_icb_cmd_valid;
  logic sram_cmd_valid;
  assign sram_cmd_valid = sram_sel_lsu | sram_sel_ifu;
  logic sram_ready;
  assign sram_ready = ~rsp_valid_r | (rsp_ifu_r ? ifu2itcm_icb_rsp_ready : lsu2itcm_icb_rsp_ready);
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      ifu_holdup_r <= 1'b0;
      rsp_ifu_r <= 1'b0;
      rsp_lsu_lo_r <= 1'b0;
      rsp_read_r <= 1'b0;
      rsp_valid_r <= 1'b0;
    end else begin
      if (sram_cmd_valid & sram_ready) begin
        rsp_valid_r <= 1'b1;
        rsp_ifu_r <= sram_sel_ifu;
        rsp_read_r <= sram_sel_lsu ? lsu2itcm_icb_cmd_read : ifu2itcm_icb_cmd_read;
        rsp_lsu_lo_r <= lsu_addr_lo;
      end else if (rsp_valid_r & (rsp_ifu_r ? ifu2itcm_icb_rsp_ready : lsu2itcm_icb_rsp_ready)) begin
        rsp_valid_r <= 1'b0;
      end
      // IFU holdup: set when IFU accessed SRAM, cleared when non-IFU accesses
      if (sram_cmd_valid & sram_ready) begin
        ifu_holdup_r <= sram_sel_ifu;
      end
    end
  end
  always_comb begin
    // SRAM command
    itcm_ram_cs = sram_cmd_valid & sram_ready;
    clk_itcm_ram = clk;
    if (sram_sel_lsu) begin
      itcm_ram_addr = lsu2itcm_icb_cmd_addr[15:3];
      itcm_ram_we = ~lsu2itcm_icb_cmd_read;
      itcm_ram_din = lsu_wdata_64;
      itcm_ram_wem = lsu_wmask_8;
    end else begin
      itcm_ram_addr = ifu2itcm_icb_cmd_addr[15:3];
      itcm_ram_we = ~ifu2itcm_icb_cmd_read;
      itcm_ram_din = ifu2itcm_icb_cmd_wdata;
      itcm_ram_wem = ifu2itcm_icb_cmd_wmask;
    end
    // Handshake
    lsu2itcm_icb_cmd_ready = sram_ready;
    ifu2itcm_icb_cmd_ready = sram_ready & ~lsu2itcm_icb_cmd_valid;
    // Response demux
    ifu2itcm_icb_rsp_valid = rsp_valid_r & rsp_ifu_r;
    ifu2itcm_icb_rsp_err = 1'b0;
    ifu2itcm_icb_rsp_rdata = itcm_ram_dout;
    lsu2itcm_icb_rsp_valid = rsp_valid_r & ~rsp_ifu_r;
    lsu2itcm_icb_rsp_err = 1'b0;
    lsu2itcm_icb_rsp_rdata = rsp_lsu_lo_r ? itcm_ram_dout[31:0] : itcm_ram_dout[63:32];
    // IFU holdup
    ifu2itcm_holdup = ifu_holdup_r;
    // External debug interface: lowest priority
    ext2itcm_icb_cmd_ready = ~ifu2itcm_icb_cmd_valid & ~lsu2itcm_icb_cmd_valid & sram_ready;
    ext2itcm_icb_rsp_valid = 1'b0;
    ext2itcm_icb_rsp_err = 1'b0;
    ext2itcm_icb_rsp_rdata = itcm_ram_dout[31:0];
    // Active
    itcm_active = ifu2itcm_icb_cmd_valid | lsu2itcm_icb_cmd_valid | ext2itcm_icb_cmd_valid | rsp_valid_r;
  end

endmodule

