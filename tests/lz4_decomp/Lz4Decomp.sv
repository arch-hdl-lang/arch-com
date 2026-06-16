// LZ4 block decompressor — top-level module.
//
// Integrates Lz4Ctrl (state-machine) with HistRam (64KB sliding-window
// history buffer).
//
// Ports
// ─────
//   clk / rst      : standard clock and synchronous active-high reset
//   in_*           : AXI-S byte-stream input  (compressed LZ4 block)
//   out_*          : AXI-S byte-stream output (decompressed bytes)
//   busy           : high while a block is being decompressed; falls for
//                    exactly one cycle (Idle) between blocks
//
// Usage
// ─────
//   1. Deassert rst.  busy falls for one cycle, then rises (TkWait).
//   2. Stream the compressed block byte-by-byte on in_*.  Assert in_last
//      on the final compressed byte.
//   3. Collect decompressed bytes from out_* while asserting out_ready.
//      out_last is set on the final decompressed byte of the block.
//   4. After out_last (or busy=0), the decompressor returns to TkWait
//      ready for the next block.
module Lz4Decomp (
  input logic clk,
  input logic rst,
  input logic in_valid,
  output logic in_ready,
  input logic [7:0] in_data,
  input logic in_last,
  output logic out_valid,
  input logic out_ready,
  output logic [7:0] out_data,
  output logic out_last,
  output logic busy
);

  // AXI-S compressed input
  // AXI-S decompressed output
  // ── Internal wires ────────────────────────────────────────────────────────
  logic in_ready_w;
  logic out_valid_w;
  logic [7:0] out_data_w;
  logic out_last_w;
  logic busy_w;
  logic hist_rd_en_w;
  logic [15:0] hist_rd_addr_w;
  logic [7:0] hist_rd_rdata_w;
  logic hist_wr_en_w;
  logic [15:0] hist_wr_addr_w;
  logic [7:0] hist_wr_wdata_w;
  // ── History RAM ───────────────────────────────────────────────────────────
  HistRam hist (
    .clk(clk),
    .rd_en(hist_rd_en_w),
    .rd_addr(hist_rd_addr_w),
    .rd_rdata(hist_rd_rdata_w),
    .wr_en(hist_wr_en_w),
    .wr_addr(hist_wr_addr_w),
    .wr_wdata(hist_wr_wdata_w)
  );
  // ── Control FSM ───────────────────────────────────────────────────────────
  Lz4Ctrl ctrl (
    .clk(clk),
    .rst(rst),
    .in_valid(in_valid),
    .in_ready(in_ready_w),
    .in_data(in_data),
    .in_last(in_last),
    .out_valid(out_valid_w),
    .out_ready(out_ready),
    .out_data(out_data_w),
    .out_last(out_last_w),
    .busy(busy_w),
    .hist_rd_en(hist_rd_en_w),
    .hist_rd_addr(hist_rd_addr_w),
    .hist_rd_rdata(hist_rd_rdata_w),
    .hist_wr_en(hist_wr_en_w),
    .hist_wr_addr(hist_wr_addr_w),
    .hist_wr_wdata(hist_wr_wdata_w)
  );
  // ── Route wires to top-level output ports ─────────────────────────────────
  assign in_ready = in_ready_w;
  assign out_valid = out_valid_w;
  assign out_data = out_data_w;
  assign out_last = out_last_w;
  assign busy = busy_w;

endmodule

