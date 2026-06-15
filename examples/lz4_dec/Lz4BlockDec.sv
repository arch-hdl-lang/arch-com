/// LZ4 block decompressor — top-level wrapper.
///
/// Instantiates Lz4DecFsm (decode state machine) and Lz4HistBuf
/// (65 536-byte history RAM), wiring the RAM access ports together.
/// Exposes only the clean byte-stream I/O to the outside world.
///
/// This is the direct hardware analogue of the CAST LZ4SNP-D IP
/// (LZ4/Snappy Data Decompressor), restricted to the LZ4 block format.
///
/// Ports
/// ─────
///   clk        100 MHz system clock
///   rst        Synchronous active-high reset
///
///   in_valid   Compressed input byte valid
///   in_data    Compressed input byte data
///   in_ready   Decoder ready to accept (back-pressure to source)
///   in_last    Assert on the last byte of the compressed block
///
///   out_valid  Decompressed output byte valid
///   out_data   Decompressed output byte data
///   out_ready  Downstream ready (back-pressure from sink)
module Lz4BlockDec (
  input logic clk,
  input logic rst,
  input logic in_valid,
  input logic [7:0] in_data,
  output logic in_ready,
  input logic in_last,
  output logic out_valid,
  output logic [7:0] out_data,
  input logic out_ready
);

  // ── Internal connections ─────────────────────────────────────────────────
  // FSM → RAM write port (combinational FSM outputs; use wire)
  logic hist_wr_en_w;
  logic [15:0] hist_wr_addr_w;
  logic [7:0] hist_wr_data_w;
  // FSM → RAM read address (combinational)
  logic hist_rd_en_w;
  logic [15:0] hist_rd_addr_w;
  // RAM → FSM read data (registered output of the RAM; use reg reset none)
  logic [7:0] hist_rd_data_w = 0;
  // FSM → module output ports (combinational; routed through wires + comb)
  logic dec_in_ready_w;
  logic dec_out_valid_w;
  logic [7:0] dec_out_data_w;
  assign in_ready = dec_in_ready_w;
  assign out_valid = dec_out_valid_w;
  assign out_data = dec_out_data_w;
  // ── History RAM ──────────────────────────────────────────────────────────
  Lz4HistBuf hist (
    .clk(clk),
    .wr_en(hist_wr_en_w),
    .wr_addr(hist_wr_addr_w),
    .wr_data(hist_wr_data_w),
    .rd_en(hist_rd_en_w),
    .rd_addr(hist_rd_addr_w),
    .rd_data(hist_rd_data_w)
  );
  // ── Decode FSM ───────────────────────────────────────────────────────────
  Lz4DecFsm dec (
    .clk(clk),
    .rst(rst),
    .in_valid(in_valid),
    .in_data(in_data),
    .in_ready(dec_in_ready_w),
    .in_last(in_last),
    .out_valid(dec_out_valid_w),
    .out_data(dec_out_data_w),
    .out_ready(out_ready),
    .hist_wr_en(hist_wr_en_w),
    .hist_wr_addr(hist_wr_addr_w),
    .hist_wr_data(hist_wr_data_w),
    .hist_rd_en(hist_rd_en_w),
    .hist_rd_addr(hist_rd_addr_w),
    .hist_rd_data(hist_rd_data_w)
  );

endmodule

