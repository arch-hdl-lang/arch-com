// E203 DTCM Controller
// Arbitrates LSU ICB access to DTCM SRAM.
// Single outstanding, 1-cycle latency SRAM access.
// Priority: LSU (only requestor without EXTITF).
module e203_dtcm_ctrl (
  input logic clk,
  input logic rst_n,
  input logic test_mode,
  input logic tcm_cgstop,
  output logic dtcm_active,
  input logic lsu2dtcm_icb_cmd_valid,
  output logic lsu2dtcm_icb_cmd_ready,
  input logic [15:0] lsu2dtcm_icb_cmd_addr,
  input logic lsu2dtcm_icb_cmd_read,
  input logic [31:0] lsu2dtcm_icb_cmd_wdata,
  input logic [3:0] lsu2dtcm_icb_cmd_wmask,
  output logic lsu2dtcm_icb_rsp_valid,
  input logic lsu2dtcm_icb_rsp_ready,
  output logic lsu2dtcm_icb_rsp_err,
  output logic [31:0] lsu2dtcm_icb_rsp_rdata,
  input logic ext2dtcm_icb_cmd_valid,
  output logic ext2dtcm_icb_cmd_ready,
  input logic [15:0] ext2dtcm_icb_cmd_addr,
  input logic ext2dtcm_icb_cmd_read,
  input logic [31:0] ext2dtcm_icb_cmd_wdata,
  input logic [3:0] ext2dtcm_icb_cmd_wmask,
  output logic ext2dtcm_icb_rsp_valid,
  input logic ext2dtcm_icb_rsp_ready,
  output logic ext2dtcm_icb_rsp_err,
  output logic [31:0] ext2dtcm_icb_rsp_rdata,
  output logic dtcm_ram_cs,
  output logic dtcm_ram_we,
  output logic [13:0] dtcm_ram_addr,
  output logic [3:0] dtcm_ram_wem,
  output logic [31:0] dtcm_ram_din,
  input logic [31:0] dtcm_ram_dout,
  output logic clk_dtcm_ram
);

  // LSU ICB command
  // LSU ICB response
  // External debug ICB command
  // External debug ICB response
  // SRAM interface
  // 1-cycle pipeline: cmd fires -> response available next cycle
  logic rsp_valid_r;
  logic rsp_read_r;
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      rsp_read_r <= 1'b0;
      rsp_valid_r <= 1'b0;
    end else begin
      if (lsu2dtcm_icb_cmd_valid & lsu2dtcm_icb_cmd_ready) begin
        rsp_valid_r <= 1'b1;
        rsp_read_r <= lsu2dtcm_icb_cmd_read;
      end else if (lsu2dtcm_icb_rsp_valid & lsu2dtcm_icb_rsp_ready) begin
        rsp_valid_r <= 1'b0;
      end
    end
  end
  assign dtcm_ram_cs = lsu2dtcm_icb_cmd_valid & (~rsp_valid_r | lsu2dtcm_icb_rsp_ready);
  assign dtcm_ram_we = ~lsu2dtcm_icb_cmd_read;
  assign dtcm_ram_addr = lsu2dtcm_icb_cmd_addr[15:2];
  assign dtcm_ram_wem = lsu2dtcm_icb_cmd_wmask;
  assign dtcm_ram_din = lsu2dtcm_icb_cmd_wdata;
  assign clk_dtcm_ram = clk;
  assign lsu2dtcm_icb_cmd_ready = ~rsp_valid_r | lsu2dtcm_icb_rsp_ready;
  assign lsu2dtcm_icb_rsp_valid = rsp_valid_r;
  assign lsu2dtcm_icb_rsp_err = 1'b0;
  assign lsu2dtcm_icb_rsp_rdata = dtcm_ram_dout;
  assign ext2dtcm_icb_cmd_ready = ~lsu2dtcm_icb_cmd_valid & (~rsp_valid_r | ext2dtcm_icb_rsp_ready);
  assign ext2dtcm_icb_rsp_valid = 1'b0;
  assign ext2dtcm_icb_rsp_err = 1'b0;
  assign ext2dtcm_icb_rsp_rdata = dtcm_ram_dout;
  assign dtcm_active = lsu2dtcm_icb_cmd_valid | ext2dtcm_icb_cmd_valid | rsp_valid_r;

endmodule

// SRAM command: pass through when cmd valid and no pending response
// Word-aligned: drop 2 LSBs
// Clock passthrough
// LSU handshake
// Response
// External debug interface: low priority, always not ready when LSU active
// Active
