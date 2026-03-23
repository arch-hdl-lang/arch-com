// E203 HBirdv2 Bus Interface Unit (BIU)
// Routes memory transactions from LSU to ITCM, DTCM, or external bus
// based on address decoding.
//   ITCM: 0x80000000 - 0x8000FFFF (64KB)
//   DTCM: 0x90000000 - 0x9000FFFF (64KB)
// Pure combinational address decode + mux.
module Biu #(
  parameter int ITCM_BASE = 'h80000000,
  parameter int DTCM_BASE = 'h90000000,
  parameter int TCM_SIZE = 'h10000
) (
  input logic [32-1:0] lsu_addr,
  input logic [32-1:0] lsu_wdata,
  input logic [4-1:0] lsu_wstrb,
  input logic lsu_wen,
  input logic lsu_ren,
  output logic [32-1:0] lsu_rdata,
  output logic itcm_rd_en,
  output logic [14-1:0] itcm_rd_addr,
  input logic [32-1:0] itcm_rd_data,
  output logic itcm_wr_en,
  output logic [14-1:0] itcm_wr_addr,
  output logic [32-1:0] itcm_wr_data,
  output logic dtcm_rd_en,
  output logic [14-1:0] dtcm_rd_addr,
  input logic [32-1:0] dtcm_rd_data,
  output logic dtcm_wr_en,
  output logic [14-1:0] dtcm_wr_addr,
  output logic [32-1:0] dtcm_wr_data,
  output logic [4-1:0] dtcm_wr_be
);

  // LSU interface (from core)
  // ITCM interface
  // DTCM interface
  // Address decode
  logic [16-1:0] addr_top;
  assign addr_top = lsu_addr[31:16];
  logic is_itcm;
  assign is_itcm = (addr_top == ITCM_BASE[31:16]);
  logic is_dtcm;
  assign is_dtcm = (addr_top == DTCM_BASE[31:16]);
  logic [14-1:0] word_addr;
  assign word_addr = lsu_addr[15:2];
  always_comb begin
    itcm_rd_en = (is_itcm & lsu_ren);
    itcm_rd_addr = word_addr;
    itcm_wr_en = (is_itcm & lsu_wen);
    itcm_wr_addr = word_addr;
    itcm_wr_data = lsu_wdata;
    dtcm_rd_en = (is_dtcm & lsu_ren);
    dtcm_rd_addr = word_addr;
    dtcm_wr_en = (is_dtcm & lsu_wen);
    dtcm_wr_addr = word_addr;
    dtcm_wr_data = lsu_wdata;
    dtcm_wr_be = lsu_wstrb;
    if (is_itcm) begin
      lsu_rdata = itcm_rd_data;
    end else if (is_dtcm) begin
      lsu_rdata = dtcm_rd_data;
    end else begin
      lsu_rdata = 0;
    end
  end

endmodule

// ITCM: read-only from BIU perspective (writes go through IFU path)
// But allow store to ITCM for self-modifying code / boot loading
// DTCM: primary data memory
// Read data mux
