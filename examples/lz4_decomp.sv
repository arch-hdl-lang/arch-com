//! ---
//! tags: [lz4, decompression, compression, streaming, ram, fsm, ip]
//! refs:
//!   - "LZ4 Block Format Description — https://github.com/lz4/lz4/blob/dev/doc/lz4_Block_format.md"
//!   - "CAST LZ4SNP-C / LZ4 Decompressor IP — https://www.cast-inc.com/compression/lossless-data-compression"
//! ---
//!
//! LZ4 block decompressor — byte-streaming hardware IP.
//!
//! Implements the LZ4 block format (no framing / frame header).
//! Input:  compressed byte stream, valid/ready handshake + `in_last`
//!         asserted on the final compressed byte.
//! Output: decompressed byte stream, valid/ready handshake.
//! `done` pulses for one cycle after the last decompressed byte is accepted.
//!
//! History buffer: 1024-byte ring buffer (HIST_BITS=10).  Matches are
//! limited to a 1024-byte window; LZ4 match offsets > 1023 are unsupported
//! in this implementation.
//!
//! Protocol:
//!   The last sequence of a valid LZ4 block ends after its literals with
//!   no match copy.  The compressor must assert `in_last` on the last
//!   literal byte of the last sequence (or on the token byte if the last
//!   sequence has zero literals).
//!
//! States:
//!   S_TOKEN     read 1-byte token (lit nibble | match nibble)
//!   S_XLIT      accumulate extra literal-length bytes (nibble==15)
//!   S_LIT       copy literal bytes to output + history
//!   S_OFFLO     consume match-offset low byte
//!   S_OFFHI     consume match-offset high byte, compute rd_ptr
//!   S_XMATCH    accumulate extra match-length bytes (nibble==15)
//!   S_MATCH     copy bytes from history ring buffer to output + history
//!   S_DONE      end-of-block; `done` asserted until reset
// domain SysDomain
//   freq_mhz: 100

typedef enum logic [2:0] {
  S_TOKEN = 3'd0,
  S_XLIT = 3'd1,
  S_LIT = 3'd2,
  S_OFFLO = 3'd3,
  S_OFFHI = 3'd4,
  S_XMATCH = 3'd5,
  S_MATCH = 3'd6,
  S_DONE = 3'd7
} DecState;

// 1 KB ring-buffer for match history.
// latency 0 = async (combinational) reads; writes commit on clk rising edge.
module HistBuf #(
  parameter int DEPTH = 1024,
  parameter int HIST_BITS = 10,
  parameter int DATA_WIDTH = 8
) (
  input logic clk,
  input logic rd_en,
  input logic [HIST_BITS-1:0] rd_addr,
  output logic [7:0] rd_data,
  input logic wr_en,
  input logic [HIST_BITS-1:0] wr_addr,
  input logic [7:0] wr_data
);

  logic [DATA_WIDTH-1:0] mem [0:DEPTH-1];
  
  assign rd_data = mem[rd_addr];
  
  always_ff @(posedge clk) begin
    if (wr_en)
      mem[wr_addr] <= wr_data;
  end

endmodule

/// LZ4 block decompressor.
///
/// Byte-at-a-time streaming with AXI-style valid/ready handshake on both
/// input (compressed) and output (decompressed) sides.
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
  output logic done
);

  logic in_txn;
  logic out_txn;
  logic hist_wr_en;
  logic [7:0] hist_wr_dat;
  // Compressed input stream.
  // Assert on the last byte of the compressed block.
  // Decompressed output stream.
  // Pulses for one cycle once the last output byte is accepted.
  // ── FSM state ─────────────────────────────────────────────────────────────
  DecState state;
  // ── Datapath registers ────────────────────────────────────────────────────
  logic [15:0] lit_len;
  logic [15:0] match_len;
  logic [3:0] match_nibble_r;
  logic [7:0] off_lo;
  logic [9:0] wr_ptr;
  logic [9:0] rd_ptr;
  // Set when the last compressed byte has been consumed; causes the FSM
  // to transition to S_DONE instead of S_TOKEN at end of current sequence.
  logic last_flag;
  // ── History buffer wiring ─────────────────────────────────────────────────
  // hist_rd_byte: combinational output of the async-read history RAM.
  // Available the same cycle the read address (rd_ptr) is presented.
  logic [7:0] hist_rd_byte;
  // hist_wr_en / hist_wr_data: driven by let bindings below.
  // Write fires when an output byte is accepted (out_valid && out_ready).
  assign in_txn = in_valid & in_ready;
  assign out_txn = out_valid & out_ready;
  // In S_LIT  the byte being saved is in_data.
  // In S_MATCH the byte being saved is hist_rd_byte (copy from history).
  assign hist_wr_en = out_txn;
  assign hist_wr_dat = state == S_MATCH ? hist_rd_byte : in_data;
  // ── History RAM instance ───────────────────────────────────────────────────
  HistBuf hist (
    .clk(clk),
    .rd_en(1'b1),
    .rd_addr(rd_ptr),
    .rd_data(hist_rd_byte),
    .wr_en(hist_wr_en),
    .wr_addr(wr_ptr),
    .wr_data(hist_wr_dat)
  );
  // ── Combinational output steering ─────────────────────────────────────────
  always_comb begin
    // in_ready: accept input only when we're in a state that consumes it
    // AND (for S_LIT) only when the output side can also accept (combined
    // transaction: literal read = literal write = history write in one cycle).
    case (state)
      DECSTATE__S_TOKEN: in_ready = 1'b1;
      DECSTATE__S_XLIT: in_ready = 1'b1;
      DECSTATE__S_LIT: in_ready = out_ready;
      DECSTATE__S_OFFLO: in_ready = 1'b1;
      DECSTATE__S_OFFHI: in_ready = 1'b1;
      DECSTATE__S_XMATCH: in_ready = 1'b1;
      default: in_ready = 1'b0;
    endcase
    // out_valid: we have valid output data in S_LIT and S_MATCH.
    out_valid = state == S_LIT && in_valid || state == S_MATCH;
    // out_data: literal bytes come from in_data; match bytes from history.
    out_data = state == S_MATCH ? hist_rd_byte : in_data;
    done = state == S_DONE;
  end
  // ── Sequential FSM + datapath ─────────────────────────────────────────────
  always_ff @(posedge clk) begin
    if (rst) begin
      last_flag <= 1'b0;
      lit_len <= 0;
      match_len <= 0;
      match_nibble_r <= 0;
      off_lo <= 0;
      rd_ptr <= 0;
      state <= S_TOKEN;
      wr_ptr <= 0;
    end else begin
      // ── S_TOKEN ───────────────────────────────────────────────────────────
      if (state == S_TOKEN) begin
        if (in_valid) begin
          if (in_last) begin
            last_flag <= 1'b1;
          end
          match_nibble_r <= in_data[3:0];
          lit_len <= 16'($unsigned(in_data[7:4]));
          if (in_data[7:4] == 4'd15) begin
            state <= S_XLIT;
          end else if (in_data[7:4] == 4'd0) begin
            if (in_last) begin
              state <= S_DONE;
            end else begin
              state <= S_OFFLO;
            end
          end else begin
            state <= S_LIT;
          end
        end
      end
      // ── S_XLIT: accumulate extra literal-length bytes ─────────────────────
      if (state == S_XLIT) begin
        if (in_valid) begin
          if (in_last) begin
            last_flag <= 1'b1;
          end
          lit_len <= 16'(lit_len + 16'($unsigned(in_data)));
          if (in_data != 8'd255) begin
            state <= S_LIT;
          end
        end
      end
      // ── S_LIT: copy literal bytes to output + history ─────────────────────
      if (state == S_LIT) begin
        if (in_valid & out_ready) begin
          // Transaction fires: literal consumed from input, output, and saved.
          if (in_last) begin
            last_flag <= 1'b1;
          end
          wr_ptr <= 10'(wr_ptr + 1);
          if (lit_len == 16'd1) begin
            // Last literal of this sequence.
            if (in_last || last_flag) begin
              state <= S_DONE;
            end else begin
              state <= S_OFFLO;
            end
            lit_len <= 0;
          end else begin
            lit_len <= 16'(lit_len - 1);
          end
        end
      end
      // ── S_OFFLO: consume match-offset low byte ────────────────────────────
      if (state == S_OFFLO) begin
        if (in_valid) begin
          off_lo <= in_data;
          if (in_last) begin
            last_flag <= 1'b1;
          end
          state <= S_OFFHI;
        end
      end
      // ── S_OFFHI: consume match-offset high byte, compute rd_ptr ──────────
      if (state == S_OFFHI) begin
        if (in_valid) begin
          if (in_last) begin
            last_flag <= 1'b1;
          end
          // rd_ptr = wr_ptr - offset (wrapping in 10-bit ring buffer).
          rd_ptr <= 10'(wr_ptr - 10'({in_data, off_lo}));
          if (match_nibble_r == 4'd15) begin
            match_len <= 16'd4;
            state <= S_XMATCH;
          end else begin
            match_len <= 16'(16'($unsigned(match_nibble_r)) + 16'd4);
            state <= S_MATCH;
          end
        end
      end
      // ── S_XMATCH: accumulate extra match-length bytes ─────────────────────
      if (state == S_XMATCH) begin
        if (in_valid) begin
          if (in_last) begin
            last_flag <= 1'b1;
          end
          match_len <= 16'(match_len + 16'($unsigned(in_data)));
          if (in_data != 8'd255) begin
            state <= S_MATCH;
          end
        end
      end
      // ── S_MATCH: copy bytes from history to output + history ──────────────
      if (state == S_MATCH) begin
        if (out_ready) begin
          wr_ptr <= 10'(wr_ptr + 1);
          rd_ptr <= 10'(rd_ptr + 1);
          if (match_len == 16'd1) begin
            // Last match byte: sequence complete.
            if (last_flag) begin
              state <= S_DONE;
            end else begin
              state <= S_TOKEN;
            end
            match_len <= 0;
          end else begin
            match_len <= 16'(match_len - 1);
          end
        end
      end
      // S_DONE is terminal until reset.
    end
  end

endmodule

