// LZ4 block decompressor — byte-stream I/O, 256-byte history window.
//
// Implements the core LZ4 block format (RFC-compatible subset):
//   Token byte  → literal_len (high nibble) + match_len_nibble (low nibble)
//   Literal run → literal_len bytes copied verbatim to output + history
//   Match offset → 2 bytes little-endian (only low byte used; history ≤ 256 B)
//   Match copy  → (match_len_nibble + 4) bytes copied from history
//
// Limitations (v1): extended lengths (nibble == 15) are not supported.
// The last sequence in a block ends on a literal byte with in_last asserted.
//
// This is the ARCH implementation of the CAST LZ4SNP-D IP core function.
// domain SysDomain
//   freq_mhz: 100

// ── History buffer RAM ────────────────────────────────────────────────────────
// 256-byte simple-dual-port history ring buffer.
// Read and write ports are independent; latency-1 read.
module Lz4HistBuf #(
  parameter int DEPTH = 256,
  parameter int DATA_WIDTH = 8
) (
  input logic clk,
  input logic rd_port_en,
  input logic [7:0] rd_port_addr,
  output logic [7:0] rd_port_rdata,
  input logic wr_port_en,
  input logic [7:0] wr_port_addr,
  input logic [7:0] wr_port_wdata
);

  logic [DATA_WIDTH-1:0] mem [0:DEPTH-1];
  logic [DATA_WIDTH-1:0] rd_port_rdata_r;
  
  always_ff @(posedge clk) begin
    if (wr_port_en)
      mem[wr_port_addr] <= wr_port_wdata;
    if (rd_port_en)
      rd_port_rdata_r <= mem[rd_port_addr];
  end
  assign rd_port_rdata = rd_port_rdata_r;

endmodule

// ── Decoder state enum ────────────────────────────────────────────────────────
typedef enum logic [2:0] {
  TOKEN = 3'd0,
  LIT = 3'd1,
  OFF_LO = 3'd2,
  OFF_HI = 3'd3,
  MATCH_REQ = 3'd4,
  MATCH_RECV = 3'd5,
  DONE = 3'd6
} Lz4DecSt;

// ── LZ4 block decompressor top module ────────────────────────────────────────
module Lz4Dec (
  input logic clk,
  input logic rst,
  input logic in_valid,
  input logic [7:0] in_data,
  input logic in_last,
  output logic in_ready,
  output logic out_valid = 1'b0,
  output logic [7:0] out_data = 0,
  output logic busy,
  output logic done
);

  logic hist_rd_en;
  logic [7:0] hist_rd_addr;
  logic hist_wr_lit;
  logic hist_wr_match;
  logic hist_wr_en;
  logic [7:0] hist_wr_addr;
  logic [7:0] hist_wr_data;
  // Input byte stream (AXI-stream-like, byte-wide)
  // assert on last literal of last sequence
  // Output byte stream (registered: valid one cycle after the byte is consumed)
  // Status
  // ── Decode registers ──────────────────────────────────────────────────────
  Lz4DecSt state_r = TOKEN;
  logic [7:0] llen_r = 0;
  logic [7:0] mlen_r = 0;
  logic [7:0] off_lo_r = 0;
  logic [7:0] wp_r = 0;
  logic [7:0] rp_r = 0;
  // ── History RAM output (latency-1 registered read data) ──────────────────
  logic [7:0] hist_rdata_r = 0;
  // ── History RAM input signals (combinational, driven via let) ─────────────
  // Read port: active only in MATCH_REQ so the latched data lands in MATCH_RECV
  assign hist_rd_en = state_r == MATCH_REQ;
  assign hist_rd_addr = rp_r;
  // Write port: literal bytes (LIT state) or match bytes (MATCH_RECV state)
  assign hist_wr_lit = state_r == LIT && in_valid;
  assign hist_wr_match = state_r == MATCH_RECV;
  assign hist_wr_en = hist_wr_lit || hist_wr_match;
  assign hist_wr_addr = wp_r;
  assign hist_wr_data = state_r == LIT ? in_data : hist_rdata_r;
  // ── History buffer instance ───────────────────────────────────────────────
  Lz4HistBuf hist (
    .clk(clk),
    .rd_port_en(hist_rd_en),
    .rd_port_addr(hist_rd_addr),
    .rd_port_rdata(hist_rdata_r),
    .wr_port_en(hist_wr_en),
    .wr_port_addr(hist_wr_addr),
    .wr_port_wdata(hist_wr_data)
  );
  // ── Combinational outputs ─────────────────────────────────────────────────
  always_comb begin
    in_ready = 1'b0;
    busy = 1'b0;
    done = 1'b0;
    if (state_r == TOKEN) begin
      in_ready = 1'b1;
    end else if (state_r == LIT) begin
      in_ready = 1'b1;
    end else if (state_r == OFF_LO) begin
      in_ready = 1'b1;
    end else if (state_r == OFF_HI) begin
      in_ready = 1'b1;
    end else if (state_r == MATCH_REQ) begin
      busy = 1'b1;
    end else if (state_r == MATCH_RECV) begin
      busy = 1'b1;
    end else if (state_r == DONE) begin
      done = 1'b1;
    end
  end
  // ── Sequential state machine ──────────────────────────────────────────────
  always_ff @(posedge clk) begin
    if (rst) begin
      llen_r <= 0;
      mlen_r <= 0;
      off_lo_r <= 0;
      out_valid <= 1'b0;
      rp_r <= 0;
      state_r <= TOKEN;
      wp_r <= 0;
    end else begin
      if (state_r == TOKEN) begin
        if (in_valid) begin
          llen_r <= 8'($unsigned(in_data[7:4]));
          mlen_r <= 8'(8'($unsigned(in_data[3:0])) + 8'd4);
          if (in_data[7:4] == 4'd0) begin
            if (in_last) begin
              state_r <= DONE;
            end else begin
              state_r <= OFF_LO;
            end
          end else begin
            state_r <= LIT;
          end
        end
      end else if (state_r == LIT) begin
        if (in_valid) begin
          wp_r <= 8'(wp_r + 8'd1);
          llen_r <= 8'(llen_r - 8'd1);
          if (in_last) begin
            state_r <= DONE;
          end else if (llen_r == 8'd1) begin
            state_r <= OFF_LO;
          end
        end
      end else if (state_r == OFF_LO) begin
        if (in_valid) begin
          off_lo_r <= in_data;
          state_r <= OFF_HI;
        end
      end else if (state_r == OFF_HI) begin
        if (in_valid) begin
          // Offset is little-endian 16-bit; only low byte used for 256-byte window.
          // rp initialised to (wp - offset) so match starts at the correct position.
          rp_r <= 8'(wp_r - off_lo_r);
          state_r <= MATCH_REQ;
        end
      end else if (state_r == MATCH_REQ) begin
        // Issue RAM read for rp_r; advance rp for the NEXT potential read.
        rp_r <= 8'(rp_r + 8'd1);
        state_r <= MATCH_RECV;
      end else if (state_r == MATCH_RECV) begin
        // hist_rdata_r holds the byte read in the previous MATCH_REQ cycle.
        wp_r <= 8'(wp_r + 8'd1);
        mlen_r <= 8'(mlen_r - 8'd1);
        if (mlen_r == 8'd1) begin
          state_r <= TOKEN;
        end else begin
          state_r <= MATCH_REQ;
        end
      end
      // Registered output: asserts for one cycle after each output byte is consumed.
      // Evaluated against the current (pre-transition) state so the byte is captured
      // at the same posedge as the FSM consumes it, appearing at the port one cycle later.
      if (state_r == LIT && in_valid) begin
        out_valid <= 1'b1;
      end else if (state_r == MATCH_RECV) begin
        out_valid <= 1'b1;
      end else begin
        out_valid <= 1'b0;
      end
    end
  end
  always_ff @(posedge clk) begin
    if (state_r == LIT && in_valid) begin
      out_data <= in_data;
    end else if (state_r == MATCH_RECV) begin
      out_data <= hist_rdata_r;
    end
  end

endmodule

