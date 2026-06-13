//! ---
//! tags: [compression, lz4, decompressor, thread, vec, tutorial]
//! refs:
//!   - "LZ4 Block Format Description — lz4.org/lz4_Block_format.html"
//!   - "CAST LZ4SNP-D LZ4/Snappy Data Decompressor IP Core"
//! ---
//!
//! LZ4 block decompressor (simplified).
//!
//! Implements the LZ4 block format byte-serial decompressor with a 16-byte
//! circular history window.  Two constraints relative to full LZ4:
//!   (1) Literal-length and match-length nibbles must be < 15 (no extension
//!       bytes).  Streams that exceed this will silently mis-decode.
//!   (2) Match offsets are taken mod 16 (only the low 4 bits of the 16-bit
//!       little-endian offset field are used).  Offsets up to 15 work
//!       correctly for the 16-byte history window.
//!
//! Interface
//!   in_data / in_valid / in_last / in_ready  — compressed input stream.
//!     Assert in_last on the final compressed byte of the block.
//!   out_data / out_valid / out_ready          — decompressed output stream.
//!   done                                      — pulses high for one cycle
//!     after the last output byte is accepted (same cycle out_ready fires
//!     the last byte).  Stays high until reset.
module _Lz4Decomp_threads #(
  localparam [3:0] _t0_S0_wait_until = 0,
  localparam [3:0] _t0_S1_wait_until = 1,
  localparam [3:0] _t0_S2_action = 2,
  localparam [3:0] _t0_S3_dispatch = 3,
  localparam [3:0] _t0_S4_action = 4,
  localparam [3:0] _t0_S5_wait_until = 5,
  localparam [3:0] _t0_S6_action = 6,
  localparam [3:0] _t0_S7_dispatch = 7,
  localparam [3:0] _t0_S8_dispatch = 8,
  localparam [3:0] _t0_S9_action = 9,
  localparam [3:0] _t0_S10_wait_until = 10,
  localparam [3:0] _t0_S11_action = 11,
  localparam [3:0] _t0_S12_wait_until = 12,
  localparam [3:0] _t0_S13_action = 13,
  localparam [3:0] _t0_S14_dispatch = 14
) (
  input logic clk,
  input logic rst,
  input logic [7:0] in_data,
  input logic in_last,
  input logic in_valid,
  input logic out_ready,
  output logic in_ready,
  output logic [7:0] out_data,
  output logic out_valid,
  output logic [7:0] byte_r,
  output logic [3:0] copy_src_r,
  output logic done_r,
  output logic [15:0] [7:0] history,
  output logic [7:0] mat_off_lo_r,
  output logic seen_last_r,
  output logic [7:0] token_r,
  output logic [3:0] wr_ptr_r
);

  logic [3:0] _t0_state = 0;
  logic [15:0] _t0_loop_cnt_0 = 0;
  logic [15:0] _t0_loop_cnt_1 = 0;
  always_comb begin
    in_ready = 0;
    out_data = 0;
    out_valid = 0;
    // Compressed input stream (byte-serial, valid/ready)
    // high on the last compressed byte of the block
    // Decompressed output stream (byte-serial, valid/ready)
    // Status: asserted once all output bytes of the block have been accepted
    // ── Internal state ──────────────────────────────────────────────────────
    // 16-byte circular history buffer (Vec register, no RAM latency)
    // next write slot
    // Per-sequence working registers
    // captured token byte
    // captured literal byte
    // match offset low byte
    // current match-copy read ptr
    // ── Decompressor thread ─────────────────────────────────────────────────
    // Each pass through the thread body handles ONE LZ4 sequence
    // (token → literals → [match_offset → match_copy]).
    // The thread loops indefinitely; once done_r is true it stalls at the
    // opening guard and waits for reset.
    in_ready = 1'b0;
    out_valid = 1'b0;
    out_data = 0;
    if (_t0_state == _t0_S1_wait_until) begin
      // Stall here after the block is done (until reset).
      // ── Step 1: Read token byte ─────────────────────────────────────────
      in_ready = 1;
    end
    if (_t0_state == _t0_S5_wait_until) begin
      // reset per sequence; true if token itself is last
      // ── Step 2: Read literal bytes (token[7:4] of them) ────────────────
      // token_r is valid in the next state (registered in previous cycle).
      // Consume one literal from the input stream.
      in_ready = 1;
    end
    if (_t0_state == _t0_S7_dispatch) begin
      // Forward literal to output and write into history.
      out_valid = 1;
      out_data = byte_r;
    end
    if (_t0_state == _t0_S10_wait_until) begin
      // ── Step 3: Check for end-of-block ─────────────────────────────────
      // Last sequence has no match — decompression complete.
      // ── Step 4: Read match offset (16-bit little-endian) ────────────
      in_ready = 1;
    end
    if (_t0_state == _t0_S12_wait_until) begin
      // low byte; high byte consumed next
      in_ready = 1;
    end
    if (_t0_state == _t0_S14_dispatch) begin
      // High byte consumed but not stored (offset mod 16 for 16-entry history).
      // Compute match-copy source pointer: wraps mod 16.
      // mat_off_lo_r is valid (captured two states ago).
      // ── Step 5: Copy match bytes (4 + token[3:0] of them) ──────────
      // copy_src_r and wr_ptr_r are updated after the do-until exits;
      // the for-loop counter evaluates token_r[3:0] which hasn't changed.
      // Output one match byte (comb read of Vec history, no latency).
      out_valid = 1;
      out_data = history[copy_src_r];
    end
    // Byte-by-byte copy into history (handles overlapping matches).
    // Thread loops back to 'wait until not done_r' — processes next sequence.
  end
  always_ff @(posedge clk) begin
    if (rst) begin
      _t0_state <= 0;
      byte_r <= 0;
      copy_src_r <= 0;
      done_r <= 1'b0;
      for (int __ri0 = 0; __ri0 < 16; __ri0++) begin
        history[__ri0] <= 0;
      end
      mat_off_lo_r <= 0;
      seen_last_r <= 1'b0;
      token_r <= 0;
      wr_ptr_r <= 0;
    end else begin
      if (_t0_state == _t0_S0_wait_until) begin
        if (!done_r) begin
          _t0_state <= _t0_S1_wait_until;
        end
      end
      if (_t0_state == _t0_S1_wait_until) begin
        if (in_valid) begin
          token_r <= in_data;
          seen_last_r <= in_last;
          _t0_state <= _t0_S3_dispatch;
        end
      end
      if (_t0_state == _t0_S3_dispatch) begin
        if (token_r[7:4] != 4'd0) begin
          _t0_state <= _t0_S4_action;
        end
        if (!(token_r[7:4] != 4'd0)) begin
          _t0_state <= _t0_S8_dispatch;
        end
      end
      if (_t0_state == _t0_S4_action) begin
        _t0_state <= _t0_S5_wait_until;
      end
      if (_t0_state == _t0_S5_wait_until) begin
        if (in_valid) begin
          byte_r <= in_data;
          seen_last_r <= seen_last_r || in_last;
          _t0_state <= _t0_S7_dispatch;
        end
      end
      if (_t0_state == _t0_S7_dispatch) begin
        if (out_ready) begin
          history[wr_ptr_r] <= byte_r;
        end
        if (out_ready) begin
          wr_ptr_r <= 4'(wr_ptr_r + 1);
        end
        if (out_ready && _t0_loop_cnt_0 < 16'(($bits(token_r[7:4]) > 4 ? $bits(token_r[7:4]) : 4)'(token_r[7:4] - 4'd1))) begin
          _t0_state <= _t0_S5_wait_until;
        end
        if (out_ready && _t0_loop_cnt_0 >= 16'(($bits(token_r[7:4]) > 4 ? $bits(token_r[7:4]) : 4)'(token_r[7:4] - 4'd1))) begin
          _t0_state <= _t0_S8_dispatch;
        end
      end
      if (_t0_state == _t0_S8_dispatch) begin
        if (seen_last_r) begin
          _t0_state <= _t0_S9_action;
        end
        if (!seen_last_r) begin
          _t0_state <= _t0_S10_wait_until;
        end
      end
      if (_t0_state == _t0_S9_action) begin
        done_r <= 1'b1;
        if (1'b1) begin
          _t0_state <= _t0_S0_wait_until;
        end
      end
      if (_t0_state == _t0_S10_wait_until) begin
        if (in_valid) begin
          mat_off_lo_r <= in_data;
          _t0_state <= _t0_S12_wait_until;
        end
      end
      if (_t0_state == _t0_S12_wait_until) begin
        if (in_valid) begin
          copy_src_r <= (4 > $bits(mat_off_lo_r[3:0]) ? 4 : $bits(mat_off_lo_r[3:0]))'(wr_ptr_r - mat_off_lo_r[3:0]);
          _t0_state <= _t0_S14_dispatch;
        end
      end
      if (_t0_state == _t0_S14_dispatch) begin
        if (out_ready) begin
          history[wr_ptr_r] <= history[copy_src_r];
        end
        if (out_ready) begin
          wr_ptr_r <= 4'(wr_ptr_r + 1);
        end
        if (out_ready) begin
          copy_src_r <= 4'(copy_src_r + 1);
        end
        if (out_ready && _t0_loop_cnt_1 < 16'(8'(8'($unsigned(token_r[3:0])) + 8'd3))) begin
          _t0_state <= _t0_S14_dispatch;
        end
        if (out_ready && _t0_loop_cnt_1 >= 16'(8'(8'($unsigned(token_r[3:0])) + 8'd3))) begin
          _t0_state <= _t0_S0_wait_until;
        end
      end
    end
  end
  always_ff @(posedge clk) begin
    if (_t0_state == _t0_S4_action) begin
      _t0_loop_cnt_0 <= 0;
    end
    if (_t0_state == _t0_S7_dispatch) begin
      if (out_ready) begin
        _t0_loop_cnt_0 <= 16'(_t0_loop_cnt_0 + 16'd1);
      end
    end
    if (_t0_state == _t0_S12_wait_until) begin
      if (in_valid) begin
        _t0_loop_cnt_1 <= 0;
      end
    end
    if (_t0_state == _t0_S14_dispatch) begin
      if (out_ready) begin
        _t0_loop_cnt_1 <= 16'(_t0_loop_cnt_1 + 16'd1);
      end
    end
  end
  // synopsys translate_off
  // Auto-generated safety assertions (bounds / divide-by-zero)
  _auto_bound_vec_0: assert property (@(posedge clk) disable iff (rst) (((_t0_state == _t0_S7_dispatch) && (out_ready)) |-> (int'(wr_ptr_r) < (16))))
    else $fatal(1, "BOUNDS VIOLATION: _Lz4Decomp_threads._auto_bound_vec_0");
  _auto_bound_vec_1: assert property (@(posedge clk) disable iff (rst) (((_t0_state == _t0_S14_dispatch) && (out_ready)) |-> (int'(wr_ptr_r) < (16))))
    else $fatal(1, "BOUNDS VIOLATION: _Lz4Decomp_threads._auto_bound_vec_1");
  _auto_bound_vec_2: assert property (@(posedge clk) disable iff (rst) (((_t0_state == _t0_S14_dispatch) && (out_ready)) |-> (int'(copy_src_r) < (16))))
    else $fatal(1, "BOUNDS VIOLATION: _Lz4Decomp_threads._auto_bound_vec_2");
  // synopsys translate_on

endmodule

// domain SysDomain
//   freq_mhz: 100

module Lz4Decomp (
  input logic clk,
  input logic rst,
  input logic [7:0] in_data,
  input logic in_valid,
  input logic in_last,
  output logic in_ready,
  output logic [7:0] out_data,
  output logic out_valid,
  input logic out_ready,
  output logic done
);

  logic [15:0] [7:0] history;
  logic [3:0] wr_ptr_r;
  logic done_r;
  logic seen_last_r;
  logic [7:0] token_r;
  logic [7:0] byte_r;
  logic [7:0] mat_off_lo_r;
  logic [3:0] copy_src_r;
  assign done = done_r;
  _Lz4Decomp_threads _threads (
    .clk(clk),
    .rst(rst),
    .in_data(in_data),
    .in_last(in_last),
    .in_valid(in_valid),
    .out_ready(out_ready),
    .in_ready(in_ready),
    .out_data(out_data),
    .out_valid(out_valid),
    .byte_r(byte_r),
    .copy_src_r(copy_src_r),
    .done_r(done_r),
    .history(history),
    .mat_off_lo_r(mat_off_lo_r),
    .seen_last_r(seen_last_r),
    .token_r(token_r),
    .wr_ptr_r(wr_ptr_r)
  );

endmodule

