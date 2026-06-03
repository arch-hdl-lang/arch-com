//! ---
//! tags: [compression, lz4, thread, streaming]
//! refs:
//!   - "LZ4 Block Format Description — lz4.org"
//!   - "CAST LZ4SNP-D LZ4/Snappy Data Decompressor IP Core"
//! ---
//!
//! LZ4 raw-block decompressor — byte-stream I/O, valid/ready handshake.
//!
//! Implements the LZ4 "block" format (no frame wrapper).
//! Sequence structure: token | [extra lit len] | literals |
//!                     offset-lo | offset-hi | [extra match len]
//! The last sequence in a block has only literals (no match bytes read).
//! s_last must be asserted on the last compressed byte (= last literal
//! of the last sequence). m_last is asserted on the last decompressed byte.
//!
//! History depth: 256 bytes (8-bit write pointer). Match offsets > 255
//! are clipped to the low byte — sufficient for the demo test vectors.
//!
//! Extended lengths: supports up to two extra-length bytes per field
//! (lit_len or match_len up to 15+255+254 = 524). Cascade-of-if avoids
//! the nested-do-until restriction.
//!
//! Architecture mirrors the CAST LZ4SNP-D pipelined token-parse datapath:
//!   1. Token parser (thread FSM)
//!   2. Extended-length accumulator (cascade-of-if)
//!   3. Literal copy engine  (input → output + history write)
//!   4. Match copy engine    (history read → output + history write)
//!   5. Circular history buffer (Vec<UInt<8>,256> register array)
module _Lz4Decomp_threads #(
  localparam [4:0] _t0_S0_wait_until = 0,
  localparam [4:0] _t0_S1_action = 1,
  localparam [4:0] _t0_S2_action = 2,
  localparam [4:0] _t0_S3_dispatch = 3,
  localparam [4:0] _t0_S4_wait_until = 4,
  localparam [4:0] _t0_S5_action = 5,
  localparam [4:0] _t0_S6_dispatch = 6,
  localparam [4:0] _t0_S7_wait_until = 7,
  localparam [4:0] _t0_S8_action = 8,
  localparam [4:0] _t0_S9_dispatch = 9,
  localparam [4:0] _t0_S10_action = 10,
  localparam [4:0] _t0_S11_wait_until = 11,
  localparam [4:0] _t0_S12_action = 12,
  localparam [4:0] _t0_S13_wait_until = 13,
  localparam [4:0] _t0_S14_dispatch = 14,
  localparam [4:0] _t0_S15_dispatch = 15,
  localparam [4:0] _t0_S16_wait_until = 16,
  localparam [4:0] _t0_S17_action = 17,
  localparam [4:0] _t0_S18_wait_until = 18,
  localparam [4:0] _t0_S19_action = 19,
  localparam [4:0] _t0_S20_dispatch = 20,
  localparam [4:0] _t0_S21_action = 21,
  localparam [4:0] _t0_S22_wait_until = 22,
  localparam [4:0] _t0_S23_action = 23,
  localparam [4:0] _t0_S24_dispatch = 24,
  localparam [4:0] _t0_S25_wait_until = 25,
  localparam [4:0] _t0_S26_action = 26,
  localparam [4:0] _t0_S27_action = 27,
  localparam [4:0] _t0_S28_action = 28,
  localparam [4:0] _t0_S29_action = 29,
  localparam [4:0] _t0_S30_wait_until = 30,
  localparam [4:0] _t0_S31_dispatch = 31
) (
  input logic clk,
  input logic rst,
  input logic m_ready,
  input logic [7:0] s_data,
  input logic s_last,
  input logic s_valid,
  output logic [7:0] m_data,
  output logic m_last,
  output logic m_valid,
  output logic s_ready,
  output logic blk_end_r,
  output logic [7:0] byte_r,
  output logic [255:0] [7:0] hist,
  output logic [15:0] lit_len_r,
  output logic [15:0] match_len_r,
  output logic [7:0] match_off_lo_r,
  output logic [7:0] match_off_r,
  output logic [3:0] match_pref_r,
  output logic [7:0] tok_r,
  output logic [7:0] wr_ptr
);

  logic [4:0] _t0_state = 0;
  logic [31:0] _t0_cnt = 0;
  logic [15:0] _t0_loop_cnt_0 = 0;
  logic [15:0] _t0_loop_cnt_1 = 0;
  always_comb begin
    m_data = 0;
    m_last = 0;
    m_valid = 0;
    s_ready = 0;
    // Compressed input (byte-wide valid/ready)
    // Decompressed output (byte-wide valid/ready)
    // 256-byte circular history buffer for match copy
    // Parse state registers (owned by thread Decomp)
    s_ready = 1'b0;
    m_valid = 1'b0;
    m_data = 8'd0;
    m_last = 1'b0;
    if (_t0_state == _t0_S0_wait_until) begin
      // ── TOKEN READ ──────────────────────────────────────────────────
      s_ready = 1;
    end
    if (_t0_state == _t0_S4_wait_until) begin
      // ── LITERAL LENGTH DECODE ───────────────────────────────────────
      // ── EXTENDED LITERAL LENGTH (cascade-of-if, up to 2 extra bytes)
      // Each extra byte adds its value to lit_len_r; exit as soon as
      // a byte < 255 is seen (no more data needed).
      s_ready = 1;
    end
    if (_t0_state == _t0_S7_wait_until) begin
      s_ready = 1;
    end
    if (_t0_state == _t0_S11_wait_until) begin
      // ── LITERAL COPY ────────────────────────────────────────────────
      // lit_len_r bytes: consume from input, emit to output, write hist.
      // blk_end_r is updated each literal so m_last fires on the last byte.
      s_ready = 1;
    end
    if (_t0_state == _t0_S13_wait_until) begin
      m_valid = 1;
      m_data = byte_r;
      m_last = blk_end_r;
    end
    if (_t0_state == _t0_S16_wait_until) begin
      // ── MATCH PHASE (skip for last sequence — blk_end_r is true) ────
      // Match offset low byte
      s_ready = 1;
    end
    if (_t0_state == _t0_S18_wait_until) begin
      // Match offset high byte (consumed; discarded for 256-byte buffer)
      s_ready = 1;
    end
    if (_t0_state == _t0_S22_wait_until) begin
      // Effective match length = token_nibble + MINMATCH(4) + extras.
      // Extended match length — same cascade pattern, up to 2 bytes
      s_ready = 1;
    end
    if (_t0_state == _t0_S25_wait_until) begin
      s_ready = 1;
    end
    if (_t0_state == _t0_S30_wait_until) begin
      // Copy match_len bytes from the circular history buffer.
      // Overlapping copies (offset < match_len) work correctly because
      // hist[wr_ptr] is written each iteration before the next read.
      m_valid = 1;
      m_data = byte_r;
      m_last = 1'b0;
    end
    // Thread loops back to next token of next sequence / next block.
  end
  always_ff @(posedge clk) begin
    if (rst) begin
      _t0_state <= 0;
      blk_end_r <= 1'b0;
      byte_r <= 0;
      for (int __ri0 = 0; __ri0 < 256; __ri0++) begin
        hist[__ri0] <= 0;
      end
      lit_len_r <= 0;
      match_len_r <= 4;
      match_off_lo_r <= 0;
      match_off_r <= 1;
      match_pref_r <= 0;
      tok_r <= 0;
      wr_ptr <= 0;
    end else begin
      if (_t0_state == _t0_S0_wait_until) begin
        if (s_valid) begin
          _t0_state <= _t0_S1_action;
        end
      end
      if (_t0_state == _t0_S1_action) begin
        tok_r <= s_data;
        blk_end_r <= s_last;
        _t0_state <= _t0_S2_action;
      end
      if (_t0_state == _t0_S2_action) begin
        if (tok_r[7:4] == 4'd15) begin
          lit_len_r <= 16'd15;
        end else begin
          lit_len_r <= 16'($unsigned(tok_r[7:4]));
        end
        match_pref_r <= tok_r[3:0];
        _t0_state <= _t0_S3_dispatch;
      end
      if (_t0_state == _t0_S3_dispatch) begin
        if (lit_len_r == 16'd15) begin
          _t0_state <= _t0_S4_wait_until;
        end
        if (!(lit_len_r == 16'd15)) begin
          _t0_state <= _t0_S9_dispatch;
        end
      end
      if (_t0_state == _t0_S4_wait_until) begin
        if (s_valid) begin
          _t0_state <= _t0_S5_action;
        end
      end
      if (_t0_state == _t0_S5_action) begin
        byte_r <= s_data;
        lit_len_r <= 16'(lit_len_r + 16'($unsigned(s_data)));
        _t0_state <= _t0_S6_dispatch;
      end
      if (_t0_state == _t0_S6_dispatch) begin
        if (byte_r == 8'd255) begin
          _t0_state <= _t0_S7_wait_until;
        end
        if (!(byte_r == 8'd255)) begin
          _t0_state <= _t0_S9_dispatch;
        end
      end
      if (_t0_state == _t0_S7_wait_until) begin
        if (s_valid) begin
          _t0_state <= _t0_S8_action;
        end
      end
      if (_t0_state == _t0_S8_action) begin
        byte_r <= s_data;
        lit_len_r <= 16'(lit_len_r + 16'($unsigned(s_data)));
        if (1'b1) begin
          _t0_state <= _t0_S9_dispatch;
        end
      end
      if (_t0_state == _t0_S9_dispatch) begin
        if (lit_len_r != 16'd0) begin
          _t0_state <= _t0_S10_action;
        end
        if (!(lit_len_r != 16'd0)) begin
          _t0_state <= _t0_S15_dispatch;
        end
      end
      if (_t0_state == _t0_S10_action) begin
        _t0_state <= _t0_S11_wait_until;
      end
      if (_t0_state == _t0_S11_wait_until) begin
        if (s_valid) begin
          _t0_state <= _t0_S12_action;
        end
      end
      if (_t0_state == _t0_S12_action) begin
        byte_r <= s_data;
        blk_end_r <= s_last;
        _t0_state <= _t0_S13_wait_until;
      end
      if (_t0_state == _t0_S13_wait_until) begin
        if (m_ready) begin
          _t0_state <= _t0_S14_dispatch;
        end
      end
      if (_t0_state == _t0_S14_dispatch) begin
        hist[wr_ptr] <= byte_r;
        wr_ptr <= 8'(wr_ptr + 1);
        if (_t0_loop_cnt_0 < 16'(lit_len_r - 1)) begin
          _t0_state <= _t0_S11_wait_until;
        end
        if (_t0_loop_cnt_0 >= 16'(lit_len_r - 1)) begin
          _t0_state <= _t0_S15_dispatch;
        end
      end
      if (_t0_state == _t0_S15_dispatch) begin
        if (!blk_end_r) begin
          _t0_state <= _t0_S16_wait_until;
        end
        if (!!blk_end_r) begin
          _t0_state <= _t0_S0_wait_until;
        end
      end
      if (_t0_state == _t0_S16_wait_until) begin
        if (s_valid) begin
          _t0_state <= _t0_S17_action;
        end
      end
      if (_t0_state == _t0_S17_action) begin
        match_off_lo_r <= s_data;
        _t0_state <= _t0_S18_wait_until;
      end
      if (_t0_state == _t0_S18_wait_until) begin
        if (s_valid) begin
          _t0_state <= _t0_S19_action;
        end
      end
      if (_t0_state == _t0_S19_action) begin
        match_off_r <= match_off_lo_r;
        _t0_state <= _t0_S20_dispatch;
      end
      if (_t0_state == _t0_S20_dispatch) begin
        if (match_pref_r == 4'd15) begin
          _t0_state <= _t0_S21_action;
        end
        if (!(match_pref_r == 4'd15)) begin
          _t0_state <= _t0_S27_action;
        end
      end
      if (_t0_state == _t0_S21_action) begin
        match_len_r <= 16'(16'd15 + 16'd4);
        _t0_state <= _t0_S22_wait_until;
      end
      if (_t0_state == _t0_S22_wait_until) begin
        if (s_valid) begin
          _t0_state <= _t0_S23_action;
        end
      end
      if (_t0_state == _t0_S23_action) begin
        byte_r <= s_data;
        match_len_r <= 16'(match_len_r + 16'($unsigned(s_data)));
        _t0_state <= _t0_S24_dispatch;
      end
      if (_t0_state == _t0_S24_dispatch) begin
        if (byte_r == 8'd255) begin
          _t0_state <= _t0_S25_wait_until;
        end
        if (!(byte_r == 8'd255)) begin
          _t0_state <= _t0_S28_action;
        end
      end
      if (_t0_state == _t0_S25_wait_until) begin
        if (s_valid) begin
          _t0_state <= _t0_S26_action;
        end
      end
      if (_t0_state == _t0_S26_action) begin
        byte_r <= s_data;
        match_len_r <= 16'(match_len_r + 16'($unsigned(s_data)));
        if (1'b1) begin
          _t0_state <= _t0_S28_action;
        end
      end
      if (_t0_state == _t0_S27_action) begin
        match_len_r <= 16'(16'($unsigned(match_pref_r)) + 16'd4);
        if (1'b1) begin
          _t0_state <= _t0_S28_action;
        end
      end
      if (_t0_state == _t0_S28_action) begin
        _t0_state <= _t0_S29_action;
      end
      if (_t0_state == _t0_S29_action) begin
        byte_r <= hist[8'(wr_ptr - match_off_r)];
        _t0_state <= _t0_S30_wait_until;
      end
      if (_t0_state == _t0_S30_wait_until) begin
        if (m_ready) begin
          _t0_state <= _t0_S31_dispatch;
        end
      end
      if (_t0_state == _t0_S31_dispatch) begin
        hist[wr_ptr] <= byte_r;
        wr_ptr <= 8'(wr_ptr + 1);
        if (_t0_loop_cnt_1 < 16'(match_len_r - 1)) begin
          _t0_state <= _t0_S29_action;
        end
        if (_t0_loop_cnt_1 >= 16'(match_len_r - 1)) begin
          _t0_state <= _t0_S0_wait_until;
        end
      end
    end
  end
  always_ff @(posedge clk) begin
    if (_t0_state == _t0_S10_action) begin
      _t0_loop_cnt_0 <= 0;
    end
    if (_t0_state == _t0_S14_dispatch) begin
      _t0_loop_cnt_0 <= 16'(_t0_loop_cnt_0 + 16'd1);
    end
    if (_t0_state == _t0_S28_action) begin
      _t0_loop_cnt_1 <= 0;
    end
    if (_t0_state == _t0_S31_dispatch) begin
      _t0_loop_cnt_1 <= 16'(_t0_loop_cnt_1 + 16'd1);
    end
  end
  // synopsys translate_off
  // Auto-generated safety assertions (bounds / divide-by-zero)
  _auto_bound_vec_0: assert property (@(posedge clk) disable iff (rst) int'(wr_ptr) < (256))
    else $fatal(1, "BOUNDS VIOLATION: _Lz4Decomp_threads._auto_bound_vec_0");
  _auto_bound_vec_1: assert property (@(posedge clk) disable iff (rst) int'(8'(wr_ptr - match_off_r)) < (256))
    else $fatal(1, "BOUNDS VIOLATION: _Lz4Decomp_threads._auto_bound_vec_1");
  // synopsys translate_on

endmodule

// domain SysDomain
//   freq_mhz: 100

module Lz4Decomp (
  input logic clk,
  input logic rst,
  input logic s_valid,
  output logic s_ready,
  input logic [7:0] s_data,
  input logic s_last,
  output logic m_valid,
  input logic m_ready,
  output logic [7:0] m_data,
  output logic m_last
);

  logic [255:0] [7:0] hist;
  logic [7:0] wr_ptr;
  logic [7:0] tok_r;
  logic blk_end_r;
  logic [15:0] lit_len_r;
  logic [3:0] match_pref_r;
  logic [7:0] match_off_lo_r;
  logic [7:0] match_off_r;
  logic [15:0] match_len_r;
  logic [7:0] byte_r;
  _Lz4Decomp_threads _threads (
    .clk(clk),
    .rst(rst),
    .m_ready(m_ready),
    .s_data(s_data),
    .s_last(s_last),
    .s_valid(s_valid),
    .m_data(m_data),
    .m_last(m_last),
    .m_valid(m_valid),
    .s_ready(s_ready),
    .blk_end_r(blk_end_r),
    .byte_r(byte_r),
    .hist(hist),
    .lit_len_r(lit_len_r),
    .match_len_r(match_len_r),
    .match_off_lo_r(match_off_lo_r),
    .match_off_r(match_off_r),
    .match_pref_r(match_pref_r),
    .tok_r(tok_r),
    .wr_ptr(wr_ptr)
  );

endmodule

