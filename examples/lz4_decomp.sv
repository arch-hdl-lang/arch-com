//! ---
//! tags: [lossless, compression, lz4, decompressor, streaming, fsm, ram, tutorial]
//! refs:
//!   - "LZ4 Block Format Description (https://github.com/lz4/lz4/blob/dev/doc/lz4_Block_format.md)"
//!   - "CAST Inc. LZ4SNP-D LZ4/Snappy Data Decompressor IP Core"
//! ---
//!
//! LZ4 block decompressor — streaming byte-by-byte hardware implementation.
//!
//! Implements the LZ4 block format: each "sequence" contains a token byte,
//! optional extra literal-length bytes, literal bytes, a 16-bit match
//! offset (little-endian), and optional extra match-length bytes. The last
//! sequence in a block has no match section; the end of the block is
//! signalled by asserting `in_last` on the last compressed byte.
//!
//! This implementation uses an 8-bit (256-byte) circular history buffer,
//! supporting match offsets in 1..255 and naturally handling LZ4's
//! overlapping-match "fill" pattern (offset < match_length).
//!
//! Interface:
//!   in_valid / in_data / in_last / in_ready  — compressed byte stream
//!   out_valid / out_data / out_ready          — decompressed byte stream
//!   done                                      — 1-cycle pulse on completion
//!
//! State encoding (reg state: UInt<4>):
//!   0 Idle      1 RdToken   2 RdXLit    3 LitIn     4 LitOut
//!   5 RdOffLo   6 RdOffHi   7 RdXMatch  8 HistRdA   9 HistRdD
//!  10 MatchOut
// domain SysDomain
//   freq_mhz: 100

// ── History buffer ────────────────────────────────────────────────────────────
/// 256-byte simple-dual-port BRAM used as a circular match-copy window.
/// Write port (wr) receives newly decompressed bytes; read port (rd) serves
/// match-copy look-ups with 1-cycle latency.
module HistBuf #(
  parameter int DEPTH = 256,
  parameter int DATA_WIDTH = 8
) (
  input logic clk,
  input logic wr_en,
  input logic [7:0] wr_addr,
  input logic [7:0] wr_data,
  input logic rd_en,
  input logic [7:0] rd_addr,
  output logic [7:0] rd_data
);

  logic [DATA_WIDTH-1:0] mem [0:DEPTH-1];
  logic [DATA_WIDTH-1:0] rd_data_r;
  
  always_ff @(posedge clk) begin
    if (wr_en)
      mem[wr_addr] <= wr_data;
    if (rd_en)
      rd_data_r <= mem[rd_addr];
  end
  assign rd_data = rd_data_r;

endmodule

// ── LZ4 decompressor ─────────────────────────────────────────────────────────
/// LZ4 block decompressor.
///
/// After reset the module enters Idle (state 0) for one cycle, then
/// transitions to RdToken and is ready to receive compressed bytes.
/// It continuously decompresses blocks; after `done` pulses it is ready
/// for the next block immediately.
module Lz4Decomp (
  input logic clk,
  input logic rst,
  input logic in_valid,
  input logic [7:0] in_data,
  input logic in_last,
  output logic in_ready,
  output logic out_valid,
  output logic [7:0] out_data,
  input logic out_ready,
  output logic done
);

  logic hw_rd_en_sig;
  logic [7:0] hw_rd_addr_sig;
  logic hw_wr_en_sig;
  logic [7:0] hw_wr_addr_sig;
  logic [7:0] hw_wr_data_sig;
  logic done_next;
  // ── State and data-path registers ────────────────────────────────────────
  logic [3:0] state;
  logic [7:0] write_ptr;
  logic [11:0] lit_cnt;
  logic [3:0] ml_nib_r;
  logic [11:0] ml_cnt;
  logic [7:0] match_off;
  logic [7:0] off_lo_r;
  logic [7:0] byte_buf;
  logic end_r;
  logic done_r;
  // ── History RAM connections ───────────────────────────────────────────────
  logic hw_wr_en;
  logic [7:0] hw_wr_addr;
  logic [7:0] hw_wr_data;
  logic hw_rd_en;
  logic [7:0] hw_rd_addr;
  logic [7:0] hw_rd_data = 0;
  // ── Combinational: RAM drives, port outputs ───────────────────────────────
  // History read: presented from HistRdA state; data arrives in HistRdD.
  // History write: on every literal consumed (LitIn+in_valid) and every
  // match byte latched (HistRdD).
  assign hw_rd_en_sig = state == 4'd8;
  assign hw_rd_addr_sig = 8'(write_ptr - match_off);
  assign hw_wr_en_sig = state == 4'd3 && in_valid || state == 4'd9;
  assign hw_wr_addr_sig = write_ptr;
  assign hw_wr_data_sig = state == 4'd9 ? hw_rd_data : in_data;
  // Done fires the cycle after end-of-block is detected:
  //   LitOut path: last literal consumed (end_r=1) and out_ready=1
  //   RdToken path: 0-literal last-sequence token (lnib=0, in_last=1)
  assign done_next = state == 4'd4 && out_ready && end_r || state == 4'd1 && in_valid && in_data[7:4] == 4'd0 && in_last;
  HistBuf hist (
    .clk(clk),
    .wr_en(hw_wr_en),
    .wr_addr(hw_wr_addr),
    .wr_data(hw_wr_data),
    .rd_en(hw_rd_en),
    .rd_addr(hw_rd_addr),
    .rd_data(hw_rd_data)
  );
  assign hw_rd_en = hw_rd_en_sig;
  assign hw_rd_addr = hw_rd_addr_sig;
  assign hw_wr_en = hw_wr_en_sig;
  assign hw_wr_addr = hw_wr_addr_sig;
  assign hw_wr_data = hw_wr_data_sig;
  assign in_ready = state == 4'd1 || state == 4'd2 || state == 4'd3 || state == 4'd5 || state == 4'd6 || state == 4'd7;
  assign out_valid = state == 4'd4 || state == 4'd10;
  assign out_data = byte_buf;
  assign done = done_r;
  // in_ready is asserted in all states that consume an input byte.
  // ── Sequential: state machine ─────────────────────────────────────────────
  always_ff @(posedge clk) begin
    if (rst) begin
      byte_buf <= 8'd0;
      done_r <= 0;
      end_r <= 0;
      lit_cnt <= 12'd0;
      match_off <= 8'd0;
      ml_cnt <= 12'd0;
      ml_nib_r <= 4'd0;
      off_lo_r <= 8'd0;
      state <= 4'd0;
      write_ptr <= 8'd0;
    end else begin
      done_r <= done_next;
      if (state == 4'd0) begin
        // Idle — immediately enter RdToken
        state <= 4'd1;
        write_ptr <= 8'd0;
        end_r <= 0;
      end else if (state == 4'd1) begin
        // RdToken: wait for token byte
        if (in_valid) begin
          ml_nib_r <= in_data[3:0];
          if (in_data[7:4] == 4'd15) begin
            lit_cnt <= 12'd15;
            state <= 4'd2;
          end else if (in_data[7:4] == 4'd0 && in_last) begin
            // RdXLit: accumulate extra lit-len
            state <= 4'd0;
          end else if (in_data[7:4] == 4'd0) begin
            // 0-lit last sequence → done (via done_next)
            state <= 4'd5;
          end else begin
            // 0 literals → straight to match offset
            lit_cnt <= 12'($unsigned(in_data[7:4]));
            state <= 4'd3;
          end
        end
      end else if (state == 4'd2) begin
        // LitIn: copy literals
        // RdXLit: accumulate extra literal-length bytes
        if (in_valid) begin
          lit_cnt <= 12'(lit_cnt + 12'($unsigned(in_data)));
          if (in_data != 8'd255) begin
            state <= 4'd3;
          end
        end
      end else if (state == 4'd3) begin
        // last extra byte → start copying literals
        // LitIn: consume one literal byte
        if (in_valid) begin
          byte_buf <= in_data;
          end_r <= in_last;
          write_ptr <= 8'(write_ptr + 8'd1);
          lit_cnt <= 12'(lit_cnt - 12'd1);
          state <= 4'd4;
        end
      end else if (state == 4'd4) begin
        // LitOut: wait for downstream to accept
        // LitOut: present byte to output stream
        if (out_ready) begin
          if (end_r) begin
            state <= 4'd0;
          end else if (lit_cnt == 12'd0) begin
            // last literal of block → Idle (done fires)
            state <= 4'd5;
          end else begin
            // last literal of sequence → read offset
            state <= 4'd3;
          end
        end
      end else if (state == 4'd5) begin
        // more literals
        // RdOffLo: read low byte of match offset
        if (in_valid) begin
          off_lo_r <= in_data;
          state <= 4'd6;
        end
      end else if (state == 4'd6) begin
        // RdOffHi: read high byte of match offset
        if (in_valid) begin
          match_off <= off_lo_r;
          // 8-bit window: use low byte; high byte consumed
          ml_cnt <= 12'($unsigned(5'($unsigned(ml_nib_r)) + 5'd4));
          if (ml_nib_r == 4'd15) begin
            state <= 4'd7;
          end else begin
            // RdXMatch: accumulate extra match-len
            state <= 4'd8;
          end
        end
      end else if (state == 4'd7) begin
        // HistRdA: begin match copy
        // RdXMatch: accumulate extra match-length bytes
        if (in_valid) begin
          ml_cnt <= 12'(ml_cnt + 12'($unsigned(in_data)));
          if (in_data != 8'd255) begin
            state <= 4'd8;
          end
        end
      end else if (state == 4'd8) begin
        // HistRdA: begin match copy
        // HistRdA: present history read address (1 cycle)
        state <= 4'd9;
      end else if (state == 4'd9) begin
        // data arrives next cycle (RAM latency = 1)
        // HistRdD: latch history byte, write back to buf
        byte_buf <= hw_rd_data;
        write_ptr <= 8'(write_ptr + 8'd1);
        ml_cnt <= 12'(ml_cnt - 12'd1);
        state <= 4'd10;
      end else if (state == 4'd10) begin
        // MatchOut: present to output stream
        // MatchOut: present match byte to output stream
        if (out_ready) begin
          if (ml_cnt == 12'd0) begin
            state <= 4'd1;
          end else begin
            // sequence done → next token
            state <= 4'd8;
          end
        end
      end
      // more match bytes
    end
  end

endmodule

