// LZ4 block decompressor — control FSM.
//
// Decodes a single LZ4 block (lz4.org/lz4_Block_format.html):
//   token byte  : high nibble = literal_len (LL), low nibble = match_base (MB)
//   ext bytes   : read while byte == 255, accumulating into LL or match_len
//   literal bytes: LL of them, copied straight to output and history
//   match offset: 2 bytes little-endian; absent for the last sequence
//   match copy  : match_len = MB + 4 bytes from (wr_ptr - offset) in history
//
// The last compressed byte has in_last=1.  For well-formed LZ4, in_last
// appears on the final literal byte of the final sequence (which has no
// match section).
//
// Assumes the history RAM is simple_dual latency-1: rd.rdata is valid one
// cycle after rd.en/rd.addr are presented.
//
// States
// ──────
//   Idle        waiting for a new block (busy=false)
//   TkWait      consume token byte from input
//   TkDec       route based on literal count (1-cycle decision state)
//   LitLenExt   consume extension bytes until byte < 255, accumulate into lit_cnt
//   LitCons     consume one literal byte; write to history; latch to byte_r
//   LitSend     present byte_r on output; wait for out_ready
//   OffLoWait   consume offset low byte
//   OffHiWait   consume offset high byte; compute offset_r and mat_cnt_r
//   MatLenExt   consume match-length extension bytes
//   MatRdIssue  issue history RAM read at (wr_ptr - offset)
//   MatSend     present RAM data on output; write to history; wait for out_ready
module Lz4Ctrl (
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
  output logic busy,
  output logic hist_rd_en,
  output logic [15:0] hist_rd_addr,
  input logic [7:0] hist_rd_rdata,
  output logic hist_wr_en,
  output logic [15:0] hist_wr_addr,
  output logic [7:0] hist_wr_wdata
);

  typedef enum logic [3:0] {
    IDLE = 4'd0,
    TKWAIT = 4'd1,
    TKDEC = 4'd2,
    LITLENEXT = 4'd3,
    LITCONS = 4'd4,
    LITSEND = 4'd5,
    OFFLOWAIT = 4'd6,
    OFFHIWAIT = 4'd7,
    MATLENEXT = 4'd8,
    MATRDISSUE = 4'd9,
    MATSEND = 4'd10
  } Lz4Ctrl_state_t;
  
  Lz4Ctrl_state_t state_r, state_next;
  
  logic [16:0] lit_cnt_r;
  logic [16:0] mat_cnt_r;
  logic [15:0] offset_r;
  logic [15:0] wr_ptr_r;
  logic [3:0] mat_base_r;
  logic [7:0] off_lo_r;
  logic last_r;
  logic [7:0] byte_r;
  
  always_ff @(posedge clk) begin
    if (rst) begin
      state_r <= IDLE;
      lit_cnt_r <= 0;
      mat_cnt_r <= 0;
      offset_r <= 0;
      wr_ptr_r <= 0;
      mat_base_r <= 0;
      off_lo_r <= 0;
      last_r <= 1'b0;
      byte_r <= 0;
    end else begin
      state_r <= state_next;
      unique case (state_r)
        IDLE: begin
          // Compressed byte-stream input (AXI-S compatible)
          // Decompressed byte-stream output (AXI-S compatible)
          // History RAM — read port (latency 1)
          // History RAM — write port
          // remaining literals to emit
          // remaining match bytes to copy
          // match backward offset
          // history write pointer (wraps mod 64K)
          // low nibble of token (match_len - 4)
          // offset low byte
          // sticky: in_last seen for this block
          // registered literal byte awaiting output
          // ── Idle ─────────────────────────────────────────────────────────────────
          // Signal to the host that the decompressor is free. Reset per-block state
          // (wr_ptr, last flag) then hand off to TkWait unconditionally.
          wr_ptr_r <= 0;
          last_r <= 1'b0;
        end
        TKWAIT: begin
          // ── TkWait ───────────────────────────────────────────────────────────────
          // Assert in_ready, wait for the token byte.
          if (in_valid) begin
            lit_cnt_r <= 17'($unsigned(in_data[7:4]));
            mat_base_r <= in_data[3:0];
            last_r <= last_r || in_last;
          end
        end
        LITLENEXT: begin
          // ── TkDec ────────────────────────────────────────────────────────────────
          // One-cycle routing state: choose next state based on literal count.
          // All transitions are mutually exclusive.
          // ── LitLenExt ────────────────────────────────────────────────────────────
          // Read extra literal-length bytes.  While in_data == 255, stay and
          // accumulate.  When in_data < 255, add the final byte and move to LitCons.
          // lit_cnt_r >= 15 when entering; always > 0 when leaving.
          if (in_valid) begin
            lit_cnt_r <= 17'(lit_cnt_r + 17'($unsigned(in_data)));
            last_r <= last_r || in_last;
          end
        end
        LITCONS: begin
          // in_data == 255: no transition fires, stays in LitLenExt
          // ── LitCons ──────────────────────────────────────────────────────────────
          // Consume one literal from the input stream.
          // Write it to history and latch it for output in LitSend.
          if (in_valid) begin
            byte_r <= in_data;
            wr_ptr_r <= 16'(wr_ptr_r + 1);
            lit_cnt_r <= 17'(lit_cnt_r - 1);
            last_r <= last_r || in_last;
          end
        end
        OFFLOWAIT: begin
          // ── LitSend ──────────────────────────────────────────────────────────────
          // Present the latched literal byte on the output.
          // lit_cnt_r was already decremented in LitCons, so lit_cnt_r == 0 means
          // this was the last literal in the sequence.
          // out_last is asserted on the final decompressed byte of the block.
          // ── OffLoWait ────────────────────────────────────────────────────────────
          // Consume match-offset low byte (little-endian, byte 0 of 2).
          if (in_valid) begin
            off_lo_r <= in_data;
            last_r <= last_r || in_last;
          end
        end
        OFFHIWAIT: begin
          // ── OffHiWait ────────────────────────────────────────────────────────────
          // Consume match-offset high byte; compute full 16-bit offset and initial
          // match length (4 + mat_base).  Route to MatLenExt if mat_base == 15.
          if (in_valid) begin
            offset_r <= 16'($unsigned(in_data)) << 8 | 16'($unsigned(off_lo_r));
            mat_cnt_r <= 17'(17'($unsigned(mat_base_r)) + 17'd4);
            last_r <= last_r || in_last;
          end
        end
        MATLENEXT: begin
          // ── MatLenExt ────────────────────────────────────────────────────────────
          // Read extra match-length bytes.  While in_data == 255, accumulate.
          // When in_data < 255, add it and proceed to MatRdIssue.
          // mat_cnt_r >= 19 when entering; always > 0 when leaving.
          if (in_valid) begin
            mat_cnt_r <= 17'(mat_cnt_r + 17'($unsigned(in_data)));
            last_r <= last_r || in_last;
          end
        end
        MATSEND: begin
          // in_data == 255: no transition fires, stays in MatLenExt
          // ── MatRdIssue ───────────────────────────────────────────────────────────
          // Issue history RAM read at address (wr_ptr - offset), wrapping mod 64K.
          // The RAM has latency 1: rdata is valid in the next cycle (MatSend).
          // ── MatSend ──────────────────────────────────────────────────────────────
          // Present hist_rd_rdata (latency-1 output from MatRdIssue's read) on the
          // output stream.  On handshake: write the byte into history, advance
          // wr_ptr, decrement mat_cnt.  Loop back to MatRdIssue for next byte;
          // after last byte, return to TkWait for the next sequence.
          //
          // hist_wr_en is gated on out_ready so the write only commits when the
          // consumer actually accepts the byte (idempotent if out_ready stays low).
          // out_last is asserted on the final match byte of the block (mat_cnt==1
          // and last_r set), covering blocks whose last compressed byte is the
          // match offset (no trailing literal sequence).
          if (out_ready) begin
            wr_ptr_r <= 16'(wr_ptr_r + 1);
            mat_cnt_r <= 17'(mat_cnt_r - 1);
          end
        end
        default: ;
      endcase
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    unique case (state_r)
      IDLE: begin
        state_next = TKWAIT;
      end
      TKWAIT: begin
        if (in_valid) state_next = TKDEC;
      end
      TKDEC: begin
        if (lit_cnt_r == 15) state_next = LITLENEXT;
        else if (lit_cnt_r != 15 && lit_cnt_r != 0) state_next = LITCONS;
        else if (lit_cnt_r == 0 && !last_r) state_next = OFFLOWAIT;
        else if (lit_cnt_r == 0 && last_r) state_next = IDLE;
      end
      LITLENEXT: begin
        if (in_valid && in_data != 255) state_next = LITCONS;
      end
      LITCONS: begin
        if (in_valid) state_next = LITSEND;
      end
      LITSEND: begin
        if (out_ready && lit_cnt_r != 0) state_next = LITCONS;
        else if (out_ready && lit_cnt_r == 0 && !last_r) state_next = OFFLOWAIT;
        else if (out_ready && lit_cnt_r == 0 && last_r) state_next = IDLE;
      end
      OFFLOWAIT: begin
        if (in_valid) state_next = OFFHIWAIT;
      end
      OFFHIWAIT: begin
        if (in_valid && mat_base_r == 15) state_next = MATLENEXT;
        else if (in_valid && mat_base_r != 15) state_next = MATRDISSUE;
      end
      MATLENEXT: begin
        if (in_valid && in_data != 255) state_next = MATRDISSUE;
      end
      MATRDISSUE: begin
        state_next = MATSEND;
      end
      MATSEND: begin
        if (out_ready && mat_cnt_r > 1) state_next = MATRDISSUE;
        else if (out_ready && mat_cnt_r == 1 && !last_r) state_next = TKWAIT;
        else if (out_ready && mat_cnt_r == 1 && last_r) state_next = IDLE;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    in_ready = 1'b0;
    out_valid = 1'b0;
    out_data = 0;
    out_last = 1'b0;
    busy = 1'b1;
    hist_rd_en = 1'b0;
    hist_rd_addr = 0;
    hist_wr_en = 1'b0;
    hist_wr_addr = 0;
    hist_wr_wdata = 0;
    unique case (state_r)
      IDLE: begin
        busy = 1'b0;
      end
      TKWAIT: begin
        in_ready = 1'b1;
      end
      TKDEC: begin
      end
      LITLENEXT: begin
        in_ready = 1'b1;
      end
      LITCONS: begin
        in_ready = 1'b1;
        hist_wr_en = in_valid;
        hist_wr_addr = wr_ptr_r;
        hist_wr_wdata = in_data;
      end
      LITSEND: begin
        out_valid = 1'b1;
        out_data = byte_r;
        out_last = lit_cnt_r == 0 && last_r;
      end
      OFFLOWAIT: begin
        in_ready = 1'b1;
      end
      OFFHIWAIT: begin
        in_ready = 1'b1;
      end
      MATLENEXT: begin
        in_ready = 1'b1;
      end
      MATRDISSUE: begin
        hist_rd_en = 1'b1;
        hist_rd_addr = 16'(wr_ptr_r - offset_r);
      end
      MATSEND: begin
        out_valid = 1'b1;
        out_data = hist_rd_rdata;
        out_last = mat_cnt_r == 1 && last_r;
        hist_wr_en = out_ready;
        hist_wr_addr = wr_ptr_r;
        hist_wr_wdata = hist_rd_rdata;
      end
      default: ;
    endcase
  end
  
  // synopsys translate_off
  _auto_legal_state: assert property (@(posedge clk) !rst |-> state_r < 11)
    else $fatal(1, "FSM ILLEGAL STATE: Lz4Ctrl.state_r = %0d", state_r);
  _auto_reach_Idle: cover property (@(posedge clk) state_r == IDLE);
  _auto_reach_TkWait: cover property (@(posedge clk) state_r == TKWAIT);
  _auto_reach_TkDec: cover property (@(posedge clk) state_r == TKDEC);
  _auto_reach_LitLenExt: cover property (@(posedge clk) state_r == LITLENEXT);
  _auto_reach_LitCons: cover property (@(posedge clk) state_r == LITCONS);
  _auto_reach_LitSend: cover property (@(posedge clk) state_r == LITSEND);
  _auto_reach_OffLoWait: cover property (@(posedge clk) state_r == OFFLOWAIT);
  _auto_reach_OffHiWait: cover property (@(posedge clk) state_r == OFFHIWAIT);
  _auto_reach_MatLenExt: cover property (@(posedge clk) state_r == MATLENEXT);
  _auto_reach_MatRdIssue: cover property (@(posedge clk) state_r == MATRDISSUE);
  _auto_reach_MatSend: cover property (@(posedge clk) state_r == MATSEND);
  _auto_tr_IDLE_to_TKWAIT: cover property (@(posedge clk) state_r == IDLE && state_next == TKWAIT);
  _auto_tr_TKWAIT_to_TKDEC: cover property (@(posedge clk) state_r == TKWAIT && state_next == TKDEC);
  _auto_tr_TKDEC_to_LITLENEXT: cover property (@(posedge clk) state_r == TKDEC && state_next == LITLENEXT);
  _auto_tr_TKDEC_to_LITCONS: cover property (@(posedge clk) state_r == TKDEC && state_next == LITCONS);
  _auto_tr_TKDEC_to_OFFLOWAIT: cover property (@(posedge clk) state_r == TKDEC && state_next == OFFLOWAIT);
  _auto_tr_TKDEC_to_IDLE: cover property (@(posedge clk) state_r == TKDEC && state_next == IDLE);
  _auto_tr_LITLENEXT_to_LITCONS: cover property (@(posedge clk) state_r == LITLENEXT && state_next == LITCONS);
  _auto_tr_LITCONS_to_LITSEND: cover property (@(posedge clk) state_r == LITCONS && state_next == LITSEND);
  _auto_tr_LITSEND_to_LITCONS: cover property (@(posedge clk) state_r == LITSEND && state_next == LITCONS);
  _auto_tr_LITSEND_to_OFFLOWAIT: cover property (@(posedge clk) state_r == LITSEND && state_next == OFFLOWAIT);
  _auto_tr_LITSEND_to_IDLE: cover property (@(posedge clk) state_r == LITSEND && state_next == IDLE);
  _auto_tr_OFFLOWAIT_to_OFFHIWAIT: cover property (@(posedge clk) state_r == OFFLOWAIT && state_next == OFFHIWAIT);
  _auto_tr_OFFHIWAIT_to_MATLENEXT: cover property (@(posedge clk) state_r == OFFHIWAIT && state_next == MATLENEXT);
  _auto_tr_OFFHIWAIT_to_MATRDISSUE: cover property (@(posedge clk) state_r == OFFHIWAIT && state_next == MATRDISSUE);
  _auto_tr_MATLENEXT_to_MATRDISSUE: cover property (@(posedge clk) state_r == MATLENEXT && state_next == MATRDISSUE);
  _auto_tr_MATRDISSUE_to_MATSEND: cover property (@(posedge clk) state_r == MATRDISSUE && state_next == MATSEND);
  _auto_tr_MATSEND_to_MATRDISSUE: cover property (@(posedge clk) state_r == MATSEND && state_next == MATRDISSUE);
  _auto_tr_MATSEND_to_TKWAIT: cover property (@(posedge clk) state_r == MATSEND && state_next == TKWAIT);
  _auto_tr_MATSEND_to_IDLE: cover property (@(posedge clk) state_r == MATSEND && state_next == IDLE);
  // synopsys translate_on

endmodule

