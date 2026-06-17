//! ---
//! tags: [lz4, decompressor, compression, streaming, axi_stream, cast_ip]
//! refs:
//!   - "LZ4 Block Format Description (lz4.org/frame_format.html)"
//!   - "CAST LZ4SNP-D (LZ4/Snappy Block Decompressor)"
//! ---
//!
//! LZ4 block decompressor — streaming AXI-Stream byte-wide input to output.
//! Accepts a single LZ4 block on the in_* ports and emits raw bytes on the
//! out_* ports. `done` pulses high (combinationally) when the last output
//! byte has been accepted; `error` pulses for a zero match offset.
//!
//! LZ4 block format (one sequence):
//!   token | (lit_len_extra)* | literals | offset_lo | offset_hi | (match_len_extra)*
//! The last sequence is literals-only (no offset/match); `in_last` is asserted
//! on its final byte.
// domain SysDomain
//   freq_mhz: 100

/// 4 KB sliding-window history — simple-dual-port, 1-cycle read latency.
///
/// Every decompressed byte (literal or match copy) is recorded here so future
/// match-copy sequences can reference it. The write pointer (`out_pos`) wraps
/// modulo 4096; match reads use `out_pos - offset` (also mod 4096).
module HistBuf #(
  parameter int DEPTH = 4096,
  parameter int ADDR_W = 12,
  parameter int DATA_WIDTH = 8
) (
  input logic clk,
  input logic rd_port_en,
  input logic [ADDR_W-1:0] rd_port_addr,
  output logic [7:0] rd_port_data,
  input logic wr_port_en,
  input logic [ADDR_W-1:0] wr_port_addr,
  input logic [7:0] wr_port_data
);

  logic [DATA_WIDTH-1:0] mem [0:DEPTH-1];
  logic [DATA_WIDTH-1:0] rd_port_data_r;
  
  always_ff @(posedge clk) begin
    if (wr_port_en)
      mem[wr_port_addr] <= wr_port_data;
    if (rd_port_en)
      rd_port_data_r <= mem[rd_port_addr];
  end
  assign rd_port_data = rd_port_data_r;
  
  initial begin
    for (int i = 0; i < DEPTH; i++) mem[i] = '0;
  end

endmodule

/// LZ4 block decompressor with AXI-Stream handshake on both sides.
///
/// FSM states (encoded in `state` UInt<4>):
///   0 TOKEN       — consume token byte: lit_nibble[7:4], match_nibble[3:0]
///   1 LIT_EXTRA   — accumulate extended literal length (0xFF bytes → continue)
///   2 LIT_COPY    — pass literal bytes to output; record each in history
///   3 OFF_LO      — latch low byte of match offset
///   4 OFF_HI      — latch high byte; form 16-bit offset; validate non-zero
///   5 MATCH_EXTRA — accumulate extended match length (0xFF bytes → continue)
///   6 MATCH_RD    — assert hist_rd_en for one cycle (RAM latency 1 pipeline)
///   7 MATCH_WAIT  — capture RAM output, present to out_data, record in history
///   8 DONE        — terminal success (in_last consumed)
///   9 ERROR       — terminal error (zero offset)
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
  output logic done,
  output logic error
);

  logic s_token;
  logic s_lit_extra;
  logic s_lit_copy;
  logic s_off_lo;
  logic s_off_hi;
  logic s_match_extra;
  logic s_match_rd;
  logic s_match_wait;
  logic s_done;
  logic s_error;
  logic in_rdy;
  logic out_v;
  logic in_fire;
  logic out_fire;
  logic [11:0] match_rd_addr;
  logic [3:0] state;
  logic [15:0] lit_cnt;
  logic [15:0] match_cnt;
  logic [3:0] match_nibble_r;
  logic [7:0] off_lo;
  logic [15:0] offset;
  logic [11:0] out_pos;
  logic hist_rd_en;
  logic [11:0] hist_rd_addr;
  logic [7:0] hist_rd_data;
  logic hist_wr_en;
  logic [11:0] hist_wr_addr;
  logic [7:0] hist_wr_data;
  assign s_token = state == 4'd0;
  assign s_lit_extra = state == 4'd1;
  assign s_lit_copy = state == 4'd2;
  assign s_off_lo = state == 4'd3;
  assign s_off_hi = state == 4'd4;
  assign s_match_extra = state == 4'd5;
  assign s_match_rd = state == 4'd6;
  assign s_match_wait = state == 4'd7;
  assign s_done = state == 4'd8;
  assign s_error = state == 4'd9;
  // Input ready: all parsing states except LIT_COPY where we also need out_ready
  assign in_rdy = s_token || s_lit_extra || s_off_lo || s_off_hi || s_match_extra || s_lit_copy && out_ready;
  // Output valid: literal passthrough (combinational) or latency-1 match data
  assign out_v = s_lit_copy && in_valid || s_match_wait;
  assign in_fire = in_valid && in_rdy;
  assign out_fire = out_v && out_ready;
  // Match copy read address: current write pointer minus stored offset (mod 4096)
  assign match_rd_addr = 12'(out_pos - 12'(offset));
  HistBuf hist (
    .clk(clk),
    .rd_port_en(hist_rd_en),
    .rd_port_addr(hist_rd_addr),
    .rd_port_data(hist_rd_data),
    .wr_port_en(hist_wr_en),
    .wr_port_addr(hist_wr_addr),
    .wr_port_data(hist_wr_data)
  );
  assign in_ready = in_rdy;
  assign out_valid = out_v;
  assign out_data = s_match_wait ? hist_rd_data : in_data;
  assign done = s_done;
  assign error = s_error;
  assign hist_rd_en = s_match_rd;
  assign hist_rd_addr = match_rd_addr;
  assign hist_wr_en = s_lit_copy && in_fire || s_match_wait && out_fire;
  assign hist_wr_addr = out_pos;
  assign hist_wr_data = s_match_wait ? hist_rd_data : in_data;
  // Write every decompressed byte into history (literal passthrough or match)
  always_ff @(posedge clk) begin
    if (rst) begin
      lit_cnt <= 16'd0;
      match_cnt <= 16'd0;
      match_nibble_r <= 4'd0;
      off_lo <= 8'd0;
      offset <= 16'd0;
      out_pos <= 12'd0;
      state <= 4'd0;
    end else begin
      if (s_token) begin
        if (in_fire) begin
          match_nibble_r <= in_data[3:0];
          match_cnt <= 16'(16'($unsigned(in_data[3:0])) + 16'd4);
          lit_cnt <= 16'($unsigned(in_data[7:4]));
          if (in_last) begin
            // Zero-literal last sequence: stream ends here
            state <= 4'd8;
          end else if (in_data[7:4] == 4'd15) begin
            state <= 4'd1;
          end else if (in_data[7:4] == 4'd0) begin
            state <= 4'd3;
          end else begin
            state <= 4'd2;
          end
        end
      end else if (s_lit_extra) begin
        if (in_fire) begin
          lit_cnt <= 16'(lit_cnt + 16'($unsigned(in_data)));
          if (!(in_data == 8'd255)) begin
            state <= 4'd2;
          end
        end
      end else if (s_lit_copy) begin
        if (in_fire) begin
          out_pos <= 12'(out_pos + 12'd1);
          lit_cnt <= 16'(lit_cnt - 16'd1);
          if (in_last) begin
            // Last byte of compressed stream consumed — block complete
            state <= 4'd8;
          end else if (lit_cnt == 16'd1) begin
            // Last literal of this sequence — read match offset next
            state <= 4'd3;
          end
        end
      end else if (s_off_lo) begin
        if (in_fire) begin
          off_lo <= in_data;
          state <= 4'd4;
        end
      end else if (s_off_hi) begin
        if (in_fire) begin
          offset <= {in_data, off_lo};
          if (in_data == 8'd0 && off_lo == 8'd0) begin
            state <= 4'd9;
          end else if (match_nibble_r == 4'd15) begin
            state <= 4'd5;
          end else begin
            state <= 4'd6;
          end
        end
      end else if (s_match_extra) begin
        if (in_fire) begin
          match_cnt <= 16'(match_cnt + 16'($unsigned(in_data)));
          if (!(in_data == 8'd255)) begin
            state <= 4'd6;
          end
        end
      end else if (s_match_rd) begin
        // One-cycle pulse to pipeline the latency-1 RAM read
        state <= 4'd7;
      end else if (s_match_wait) begin
        if (out_fire) begin
          out_pos <= 12'(out_pos + 12'd1);
          match_cnt <= 16'(match_cnt - 16'd1);
          if (match_cnt == 16'd1) begin
            // Consumed last match byte; return to token parsing
            state <= 4'd0;
          end else begin
            state <= 4'd6;
          end
        end
      end
    end
  end

endmodule

