/// LZ4 block decompressor — decode state machine.
///
/// Implements the LZ4 block format (RFC-lz4 block spec):
///
///   repeat until end-of-block:
///     token := read_byte()         // high nibble = lit_len, low = match_len
///     if lit_nibble == 15: accumulate extra lit-len bytes (stop at < 255)
///     copy lit_len literal bytes   → output + history
///     if not last-sequence:
///       offset := read_u16_le()    // back-reference distance
///       if match_nibble == 15: accumulate extra match-len bytes
///       copy (match_len + 4) bytes from history[write_ptr - offset]
///
/// Ports:
///   in_valid/in_data/in_ready/in_last  — compressed byte stream (AXI-S style)
///   out_valid/out_data/out_ready       — decompressed byte stream
///   hist_*                             — connect to Lz4HistBuf RAM
///
/// Throughput (with out_ready = 1):
///   Literal copy: 1 byte/cycle
///   Match copy:   1 byte / 2 cycles (IssueMRead → CopyMatch pipeline)
///
/// The FSM ports for the history RAM are exposed so the wrapper module
/// (Lz4BlockDec) can instantiate the RAM separately and wire it in.
module Lz4DecFsm (
  input logic clk,
  input logic rst,
  input logic in_valid,
  input logic [7:0] in_data,
  output logic in_ready,
  input logic in_last,
  output logic out_valid,
  output logic [7:0] out_data,
  input logic out_ready,
  output logic hist_wr_en,
  output logic [15:0] hist_wr_addr,
  output logic [7:0] hist_wr_data,
  output logic hist_rd_en,
  output logic [15:0] hist_rd_addr,
  input logic [7:0] hist_rd_data
);

  typedef enum logic [3:0] {
    READTOKEN = 4'd0,
    EXTLITLEN = 4'd1,
    COPYLIT = 4'd2,
    READOFFLO = 4'd3,
    READOFFHI = 4'd4,
    EXTMATCHLEN = 4'd5,
    ISSUEMREAD = 4'd6,
    COPYMATCH = 4'd7,
    DONE = 4'd8
  } Lz4DecFsm_state_t;
  
  Lz4DecFsm_state_t state_r, state_next;
  
  logic [15:0] lit_len_r;
  logic [15:0] match_len_r;
  logic [15:0] match_off_r;
  logic [15:0] copy_cnt_r;
  logic [15:0] write_ptr_r;
  logic [7:0] off_lo_r;
  
  always_ff @(posedge clk) begin
    if (rst) begin
      state_r <= READTOKEN;
      lit_len_r <= 0;
      match_len_r <= 0;
      match_off_r <= 0;
      copy_cnt_r <= 0;
      write_ptr_r <= 0;
      off_lo_r <= 0;
    end else begin
      state_r <= state_next;
      unique case (state_r)
        READTOKEN: begin
          // Compressed input stream
          // Assert in_last on the final byte of the compressed block.
          // For well-formed LZ4 input this is the last literal of the last sequence.
          // Decompressed output stream
          // History RAM — write port (combinational; driven from comb blocks)
          // History RAM — read port (1-cycle latency; addr issued in IssueMRead,
          // data arrives in CopyMatch)
          // Datapath registers
          // accumulated literal length
          // token match nibble (0-15)
          // 16-bit back-reference offset
          // bytes remaining in current copy
          // output byte index (mod 65536)
          // offset low byte (held for ReadOffHi)
          // Default combinational outputs (all quiesced; per-state blocks override).
          // ── ReadToken ────────────────────────────────────────────────────────────
          // Consume one token byte, split into lit_len and match_len nibbles.
          if (in_valid) begin
            lit_len_r <= 16'($unsigned(in_data[7:4]));
            match_len_r <= 16'($unsigned(in_data[3:0]));
            copy_cnt_r <= 16'($unsigned(in_data[7:4]));
          end
        end
        EXTLITLEN: begin
          // in_last on the token byte signals a zero-literal last sequence → done.
          // ── ExtLitLen ────────────────────────────────────────────────────────────
          // Accumulate extended literal-length bytes until one reads < 255.
          // On each 255 byte: lit_len_r += 255, stay.
          // On final byte X (<255): lit_len_r += X, copy_cnt_r = total, → CopyLit.
          if (in_valid) begin
            lit_len_r <= 16'(lit_len_r + 16'($unsigned(in_data)));
            if (in_data != 255) begin
              copy_cnt_r <= 16'(lit_len_r + 16'($unsigned(in_data)));
            end
          end
        end
        COPYLIT: begin
          // ── CopyLit ──────────────────────────────────────────────────────────────
          // Pass copy_cnt_r literal bytes from input to output and to history.
          // Handshake: accept input only when output can accept (in_ready = out_ready).
          // One byte per cycle when both sides are ready.
          if (in_valid && out_ready) begin
            copy_cnt_r <= 16'(copy_cnt_r - 1);
            write_ptr_r <= 16'(write_ptr_r + 1);
          end
        end
        READOFFLO: begin
          // last byte of last sequence: done
          // last byte of non-last sequence: read match offset
          // ── ReadOffLo ─────────────────────────────────────────────────────────────
          // Consume match offset low byte (little-endian, byte 0 of 2).
          if (in_valid) begin
            off_lo_r <= in_data;
          end
        end
        READOFFHI: begin
          // ── ReadOffHi ─────────────────────────────────────────────────────────────
          // Consume match offset high byte, form 16-bit offset, initialise copy_cnt.
          if (in_valid) begin
            match_off_r <= {in_data, off_lo_r};
            if (match_len_r == 15) begin
              copy_cnt_r <= 15;
            end else begin
              copy_cnt_r <= 16'(match_len_r + 4);
            end
          end
        end
        EXTMATCHLEN: begin
          // ── ExtMatchLen ───────────────────────────────────────────────────────────
          // Accumulate extended match-length bytes (same scheme as ExtLitLen).
          // copy_cnt_r enters as 15 (from nibble); on the final byte +4 is added
          // to form the actual match length.
          if (in_valid) begin
            if (in_data != 255) begin
              copy_cnt_r <= 16'(copy_cnt_r + 16'($unsigned(in_data)) + 4);
            end else begin
              copy_cnt_r <= 16'(copy_cnt_r + 16'($unsigned(in_data)));
            end
          end
        end
        COPYMATCH: begin
          // ── IssueMRead ────────────────────────────────────────────────────────────
          // Issue a RAM read for the next match byte.  Address = write_ptr - offset,
          // which naturally wraps (both UInt<16>) to give the ring-buffer index.
          // Data arrives one cycle later in CopyMatch.
          // ── CopyMatch ─────────────────────────────────────────────────────────────
          // Output the match byte received from the RAM and write it to history so
          // overlapping copies (match_off < copy_cnt) produce the correct RLE bytes.
          // Stays until out_ready: the registered RAM output is stable while rd_en = 0.
          if (out_ready) begin
            copy_cnt_r <= 16'(copy_cnt_r - 1);
            write_ptr_r <= 16'(write_ptr_r + 1);
          end
        end
        default: ;
      endcase
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    unique case (state_r)
      READTOKEN: begin
        if (in_valid && in_last) state_next = DONE;
        else if (in_valid && !in_last && in_data[7:4] == 15) state_next = EXTLITLEN;
        else if (in_valid && !in_last && in_data[7:4] > 0 && in_data[7:4] < 15) state_next = COPYLIT;
        else if (in_valid && !in_last && in_data[7:4] == 0) state_next = READOFFLO;
      end
      EXTLITLEN: begin
        if (in_valid && in_data != 255) state_next = COPYLIT;
      end
      COPYLIT: begin
        if (in_valid && out_ready && copy_cnt_r == 1 && in_last) state_next = DONE;
        else if (in_valid && out_ready && copy_cnt_r == 1 && !in_last) state_next = READOFFLO;
      end
      READOFFLO: begin
        if (in_valid) state_next = READOFFHI;
      end
      READOFFHI: begin
        if (in_valid && match_len_r == 15) state_next = EXTMATCHLEN;
        else if (in_valid && match_len_r < 15) state_next = ISSUEMREAD;
      end
      EXTMATCHLEN: begin
        if (in_valid && in_data != 255) state_next = ISSUEMREAD;
      end
      ISSUEMREAD: begin
        state_next = COPYMATCH;
      end
      COPYMATCH: begin
        if (out_ready && copy_cnt_r > 1) state_next = ISSUEMREAD;
        else if (out_ready && copy_cnt_r == 1) state_next = READTOKEN;
      end
      DONE: begin
        state_next = READTOKEN;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    in_ready = 1'b0;
    out_valid = 1'b0;
    out_data = 0;
    hist_wr_en = 1'b0;
    hist_wr_addr = 0;
    hist_wr_data = 0;
    hist_rd_en = 1'b0;
    hist_rd_addr = 0;
    unique case (state_r)
      READTOKEN: begin
        in_ready = 1'b1;
      end
      EXTLITLEN: begin
        in_ready = 1'b1;
      end
      COPYLIT: begin
        in_ready = out_ready;
        out_valid = in_valid;
        out_data = in_data;
        hist_wr_en = in_valid && out_ready;
        hist_wr_addr = write_ptr_r;
        hist_wr_data = in_data;
      end
      READOFFLO: begin
        in_ready = 1'b1;
      end
      READOFFHI: begin
        in_ready = 1'b1;
      end
      EXTMATCHLEN: begin
        in_ready = 1'b1;
      end
      ISSUEMREAD: begin
        hist_rd_en = 1'b1;
        hist_rd_addr = 16'(write_ptr_r - match_off_r);
      end
      COPYMATCH: begin
        out_valid = 1'b1;
        out_data = hist_rd_data;
        hist_wr_en = out_ready;
        hist_wr_addr = write_ptr_r;
        hist_wr_data = hist_rd_data;
      end
      DONE: begin
      end
      default: ;
    endcase
  end
  
  // synopsys translate_off
  _auto_legal_state: assert property (@(posedge clk) !rst |-> state_r < 9)
    else $fatal(1, "FSM ILLEGAL STATE: Lz4DecFsm.state_r = %0d", state_r);
  _auto_reach_ReadToken: cover property (@(posedge clk) state_r == READTOKEN);
  _auto_reach_ExtLitLen: cover property (@(posedge clk) state_r == EXTLITLEN);
  _auto_reach_CopyLit: cover property (@(posedge clk) state_r == COPYLIT);
  _auto_reach_ReadOffLo: cover property (@(posedge clk) state_r == READOFFLO);
  _auto_reach_ReadOffHi: cover property (@(posedge clk) state_r == READOFFHI);
  _auto_reach_ExtMatchLen: cover property (@(posedge clk) state_r == EXTMATCHLEN);
  _auto_reach_IssueMRead: cover property (@(posedge clk) state_r == ISSUEMREAD);
  _auto_reach_CopyMatch: cover property (@(posedge clk) state_r == COPYMATCH);
  _auto_reach_Done: cover property (@(posedge clk) state_r == DONE);
  _auto_tr_READTOKEN_to_DONE: cover property (@(posedge clk) state_r == READTOKEN && state_next == DONE);
  _auto_tr_READTOKEN_to_EXTLITLEN: cover property (@(posedge clk) state_r == READTOKEN && state_next == EXTLITLEN);
  _auto_tr_READTOKEN_to_COPYLIT: cover property (@(posedge clk) state_r == READTOKEN && state_next == COPYLIT);
  _auto_tr_READTOKEN_to_READOFFLO: cover property (@(posedge clk) state_r == READTOKEN && state_next == READOFFLO);
  _auto_tr_EXTLITLEN_to_COPYLIT: cover property (@(posedge clk) state_r == EXTLITLEN && state_next == COPYLIT);
  _auto_tr_COPYLIT_to_DONE: cover property (@(posedge clk) state_r == COPYLIT && state_next == DONE);
  _auto_tr_COPYLIT_to_READOFFLO: cover property (@(posedge clk) state_r == COPYLIT && state_next == READOFFLO);
  _auto_tr_READOFFLO_to_READOFFHI: cover property (@(posedge clk) state_r == READOFFLO && state_next == READOFFHI);
  _auto_tr_READOFFHI_to_EXTMATCHLEN: cover property (@(posedge clk) state_r == READOFFHI && state_next == EXTMATCHLEN);
  _auto_tr_READOFFHI_to_ISSUEMREAD: cover property (@(posedge clk) state_r == READOFFHI && state_next == ISSUEMREAD);
  _auto_tr_EXTMATCHLEN_to_ISSUEMREAD: cover property (@(posedge clk) state_r == EXTMATCHLEN && state_next == ISSUEMREAD);
  _auto_tr_ISSUEMREAD_to_COPYMATCH: cover property (@(posedge clk) state_r == ISSUEMREAD && state_next == COPYMATCH);
  _auto_tr_COPYMATCH_to_ISSUEMREAD: cover property (@(posedge clk) state_r == COPYMATCH && state_next == ISSUEMREAD);
  _auto_tr_COPYMATCH_to_READTOKEN: cover property (@(posedge clk) state_r == COPYMATCH && state_next == READTOKEN);
  _auto_tr_DONE_to_READTOKEN: cover property (@(posedge clk) state_r == DONE && state_next == READTOKEN);
  // synopsys translate_on

endmodule

// ── Done ──────────────────────────────────────────────────────────────────
// One-cycle interlude; returns to ReadToken for the next block.
