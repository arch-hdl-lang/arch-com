//! ---
//! tags: [compression, lz4, lossless, streaming, decompressor, cast-inc]
//! refs:
//!   - "LZ4 Block Format (https://github.com/lz4/lz4/blob/dev/doc/lz4_Block_format.md)"
//!   - "CAST LZ4SNP-D | LZ4/Snappy Data Decompressor IP Core (cast-inc.com)"
//! ---
//!
//! LZ4 Block Decompressor — hardware IP analog to CAST LZ4SNP-D
//!
//! Decodes a single LZ4 block (no frame header) from a byte-streaming input
//! to a byte-streaming output. Matches the I/O surface of the CAST LZ4SNP-D
//! IP: AXI-stream-style byte-wide ports, single-clock synchronous design,
//! parameterizable history-buffer depth.
//!
//! Interface:
//!   in_valid/in_data/in_last/in_ready  — compressed byte input stream
//!   out_valid/out_data/out_last/out_ready — decompressed byte output stream
//!   busy   — high while decoding (combinational)
//!   done   — pulses high for one cycle when the last output byte is accepted
//!   error  — latches high on malformed input; hold until reset
//!
//! Algorithm (LZ4 Block Format v1.9):
//!   Each "sequence":
//!     1. Token byte: upper nibble = literal_length, lower nibble = match_len_raw
//!     2. Extended literal length bytes (while byte == 255, if raw == 15)
//!     3. Literal bytes: literal_length bytes forwarded verbatim to output
//!     4. Match offset: 2 bytes little-endian (absent in the LAST sequence)
//!     5. Extended match length bytes (while byte == 255, if raw == 15)
//!     6. Match copy: (match_len_raw + 4) bytes from history at offset back
//!   The last sequence ends after step 3 (no offset or match). in_last is
//!   set on the final literal byte of the block. Minimum match = 4 bytes.
//!
//! Throughput: 1 byte/cycle. Register-array history (suitable for
//! HIST_DEPTH ≤ 512 in sim; replace with BRAM inst for synthesis).
//!
//! Error conditions latched in ERROR state (until reset):
//!   - offset == 0 (invalid per LZ4 spec)
//!   - offset high byte != 0 (offset > 255 > HIST_DEPTH for default params)
// domain SysDomain
//   freq_mhz: 200

module Lz4BlockDecoder #(
  parameter int HIST_DEPTH = 256,
  parameter int HIST_PTR = 8
) (
  input logic clk,
  input logic rst,
  input logic in_valid,
  input logic [7:0] in_data,
  input logic in_last,
  output logic in_ready,
  output logic out_valid,
  output logic [7:0] out_data,
  output logic out_last,
  input logic out_ready,
  output logic busy,
  output logic done,
  output logic error
);

  logic [7:0] rd_data;
  logic last_lit;
  logic [3:0] tok_ll_w;
  logic [3:0] tok_ml_w;
  logic [15:0] base_mat_cnt;
  logic [HIST_PTR-1:0] mat_rd_init;
  logic s_rd;
  logic s_le;
  logic s_lc;
  logic s_ol;
  logic s_oh;
  logic s_me;
  logic s_mc;
  logic s_er;
  // History buffer depth in bytes (must be a power of 2).
  // HIST_PTR must equal log2(HIST_DEPTH).
  // Compressed input byte stream
  // high on last byte of the compressed block
  // Decompressed output byte stream
  // high on last decompressed byte
  // Status
  // high while decoding
  // pulses one cycle when last byte is accepted
  // latches on malformed block
  // ── FSM state encoding ──────────────────────────────────────────────────
  // 0=READ_TOKEN (reset state)  1=LIT_EXT   2=LIT_COPY
  // 3=OFF_LO  4=OFF_HI  5=MAT_EXT  6=MAT_COPY  7=ERROR
  logic [2:0] state;
  // ── Decoder data registers ───────────────────────────────────────────────
  logic [15:0] lit_cnt;
  // literals remaining to copy
  logic [15:0] mat_cnt;
  // match bytes remaining to copy
  logic [3:0] tok_ml;
  // raw match nibble from token
  logic [7:0] offset_lo;
  // low byte of 16-bit LZ4 offset
  logic eob_r;
  // end-of-block accumulated flag
  // ── Circular history buffer ──────────────────────────────────────────────
  // Register array: suitable for HIST_DEPTH ≤ 512. For larger depths convert
  // hist_buf to a simple_dual RAM instantiation with 1-cycle read latency
  // and add a MAT_RD_WAIT state between MAT_REQ and MAT_COPY.
  logic [HIST_DEPTH-1:0] [7:0] hist_buf;
  logic [HIST_PTR-1:0] wr_ptr;
  logic [HIST_PTR-1:0] mat_rd_ptr;
  // ── Combinational signals ────────────────────────────────────────────────
  // History read — zero-latency register array
  assign rd_data = hist_buf[mat_rd_ptr];
  // True on the cycle the last literal of the block is accepted
  assign last_lit = lit_cnt == 16'd1 && (eob_r || in_last);
  // Token nibbles from current input (meaningful in READ_TOKEN)
  assign tok_ll_w = in_data[7:4];
  assign tok_ml_w = in_data[3:0];
  // Match count base = tok_ml + 4 (LZ4 min match is 4)
  assign base_mat_cnt = 16'(16'($unsigned(tok_ml)) + 16'd4);
  // mat_rd_ptr init = wr_ptr − offset (wrapping)
  assign mat_rd_init = (HIST_PTR > 8 ? HIST_PTR : 8)'(wr_ptr - offset_lo);
  // State one-hot flags for output mux
  assign s_rd = state == 3'd0;
  // READ_TOKEN
  assign s_le = state == 3'd1;
  // LIT_EXT
  assign s_lc = state == 3'd2;
  // LIT_COPY
  assign s_ol = state == 3'd3;
  // OFF_LO
  assign s_oh = state == 3'd4;
  // OFF_HI
  assign s_me = state == 3'd5;
  // MAT_EXT
  assign s_mc = state == 3'd6;
  // MAT_COPY
  assign s_er = state == 3'd7;
  // ERROR
  // ── Output port assignments (all combinational) ──────────────────────────
  assign in_ready = s_rd || s_le || s_ol || s_oh || s_me || s_lc && out_ready;
  assign out_valid = s_lc && in_valid || s_mc;
  assign out_data = s_lc ? in_data : rd_data;
  assign out_last = s_lc && last_lit;
  assign busy = !s_er;
  assign done = s_lc && last_lit && in_valid && out_ready;
  assign error = s_er;
  // in_ready: accept whenever we need a byte. In LIT_COPY gate on out_ready
  // so input and output flow together (the literal byte passes straight through).
  // out_valid: emit in LIT_COPY (when we have input) or MAT_COPY.
  // out_data: pass through literal; or serve from history during match copy.
  // out_last: high on the final output byte of the block.
  // busy: active in all non-error, non-idle states (READ_TOKEN counts as busy).
  // done: pulse on the cycle the last byte is handed off downstream.
  // error: latch
  // ── Sequential state machine ─────────────────────────────────────────────
  always_ff @(posedge clk) begin
    if (rst) begin
      eob_r <= 0;
      for (int __ri0 = 0; __ri0 < HIST_DEPTH; __ri0++) begin
        hist_buf[__ri0] <= 0;
      end
      lit_cnt <= 0;
      mat_cnt <= 0;
      mat_rd_ptr <= 0;
      offset_lo <= 0;
      state <= 0;
      tok_ml <= 0;
      wr_ptr <= 0;
    end else begin
      case (state)
        3'd0: begin
          // READ_TOKEN: wait for token byte and decode literal/match lengths
          if (in_valid) begin
            tok_ml <= tok_ml_w;
            eob_r <= in_last;
            if (tok_ll_w == 4'd15) begin
              lit_cnt <= 16'd15;
              state <= 1;
            end else if (tok_ll_w == 4'd0) begin
              // LIT_EXT
              lit_cnt <= 0;
              if (in_last) begin
                state <= 0;
              end else begin
                // empty last sequence: back to READ_TOKEN (done pulsed via comb)
                state <= 3;
              end
            end else begin
              // OFF_LO: no literals in this sequence
              lit_cnt <= 16'($unsigned(tok_ll_w));
              state <= 2;
            end
          end
        end
        3'd1: begin
          // LIT_COPY
          // LIT_EXT: accumulate extra literal length bytes
          if (in_valid) begin
            lit_cnt <= 16'(lit_cnt + 16'($unsigned(in_data)));
            eob_r <= eob_r || in_last;
            if (in_data != 8'd255) begin
              state <= 2;
            end
          end
        end
        3'd2: begin
          // LIT_COPY
          // LIT_COPY: forward input → output + history
          if (in_valid && out_ready) begin
            hist_buf[wr_ptr] <= in_data;
            wr_ptr <= (HIST_PTR > 8 ? HIST_PTR : 8)'(wr_ptr + 8'd1);
            eob_r <= eob_r || in_last;
            if (last_lit) begin
              lit_cnt <= 0;
              state <= 0;
            end else if (lit_cnt == 16'd1) begin
              // end of block: done pulsed combinationally
              lit_cnt <= 0;
              state <= 3;
            end else begin
              // last literal of mid-block sequence: read offset
              lit_cnt <= 16'(lit_cnt - 16'd1);
            end
          end
        end
        3'd3: begin
          // OFF_LO: read low byte of 16-bit little-endian match offset
          if (in_valid) begin
            offset_lo <= in_data;
            state <= 4;
          end
        end
        3'd4: begin
          // OFF_HI: read high byte, validate, initialise mat_rd_ptr
          if (in_valid) begin
            if (in_data != 8'd0) begin
              // offset > 255: exceeds history depth for default HIST_DEPTH=256
              state <= 7;
            end else if (offset_lo == 8'd0) begin
              // ERROR
              // offset == 0 is invalid in the LZ4 format
              state <= 7;
            end else begin
              // ERROR
              mat_rd_ptr <= mat_rd_init;
              mat_cnt <= base_mat_cnt;
              if (tok_ml == 4'd15) begin
                state <= 5;
              end else begin
                // MAT_EXT
                state <= 6;
              end
            end
          end
        end
        3'd5: begin
          // MAT_COPY
          // MAT_EXT: accumulate extra match length bytes
          if (in_valid) begin
            mat_cnt <= 16'(mat_cnt + 16'($unsigned(in_data)));
            if (in_data != 8'd255) begin
              state <= 6;
            end
          end
        end
        3'd6: begin
          // MAT_COPY
          // MAT_COPY: stream history bytes to output + record in history
          if (out_ready) begin
            hist_buf[wr_ptr] <= rd_data;
            wr_ptr <= (HIST_PTR > 8 ? HIST_PTR : 8)'(wr_ptr + 8'd1);
            mat_rd_ptr <= (HIST_PTR > 8 ? HIST_PTR : 8)'(mat_rd_ptr + 8'd1);
            if (mat_cnt == 16'd1) begin
              mat_cnt <= 0;
              state <= 0;
            end else begin
              // READ_TOKEN: fetch next sequence
              mat_cnt <= 16'(mat_cnt - 16'd1);
            end
          end
        end
        3'd7: begin
          // ERROR: latch until reset
          if (1'b0) begin
            state <= 7;
          end
        end
      endcase
      // satisfy exhaustiveness checker; dead code
    end
  end
  // synopsys translate_off
  // Auto-generated safety assertions (bounds / divide-by-zero)
  _auto_bound_vec_0: assert property (@(posedge clk) disable iff (rst) ((in_valid && out_ready) |-> (int'(wr_ptr) < (HIST_DEPTH))))
    else $fatal(1, "BOUNDS VIOLATION: Lz4BlockDecoder._auto_bound_vec_0");
  _auto_bound_vec_1: assert property (@(posedge clk) disable iff (rst) ((out_ready) |-> (int'(wr_ptr) < (HIST_DEPTH))))
    else $fatal(1, "BOUNDS VIOLATION: Lz4BlockDecoder._auto_bound_vec_1");
  // synopsys translate_on

endmodule

