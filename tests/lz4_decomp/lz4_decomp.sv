// LZ4 block decompressor — CAST LZ4SNP-D style
//
// Implements LZ4 block-format decompression (no frame header).
// Streaming byte-in / byte-out with AXI-stream-style valid/ready.
// History window: 4096 bytes (12-bit circular).
//
// Input stream: in_valid / in_data / in_ready / in_last
//   in_last must be asserted on the last byte of the compressed block.
//
// Output stream: out_valid / out_data / out_ready
//   out_ready = 1 assumed (no backpressure in v1).
//
// LZ4 sequence format (per block):
//   Token byte → [lit-len extension bytes] → literal bytes
//   → match-offset low → match-offset high → [match-len extension bytes]
//   (last sequence terminates after literal bytes, no match)
// domain SysDomain
//   freq_mhz: 100

// ── 4096 × 8-bit async-read simple-dual-port history RAM ─────────────────────
// Write port stores literals and match-copy bytes.
// Read port supplies match-copy bytes combinationally (latency 0 = async).
module Lz4HistBuf #(
  parameter int DEPTH = 256,
  parameter int DATA_WIDTH = 8
) (
  input logic clk,
  input logic [11:0] rd_port_addr,
  output logic [7:0] rd_port_rdata,
  input logic wr_port_en,
  input logic [11:0] wr_port_addr,
  input logic [7:0] wr_port_wdata
);

  logic [DATA_WIDTH-1:0] mem [0:DEPTH-1];
  
  assign rd_port_rdata = mem[rd_port_addr];
  
  always_ff @(posedge clk) begin
    if (wr_port_en)
      mem[wr_port_addr] <= wr_port_wdata;
  end

endmodule

// ── LZ4 decompressor FSM ──────────────────────────────────────────────────────
// States:
//   Token      — wait for token byte; decode literal-len / match-len fields
//   LitLenExt  — read extra literal-length bytes (nibble was 15)
//   LitCopy    — pass literal bytes to output, write to history
//   MatchOffLo — read match-offset low byte
//   MatchOffHi — read match-offset high byte; compute match read pointer
//   MatchLenExt — read extra match-length bytes (nibble was 15)
//   MatchCopy  — replay bytes from history into output and history
//   Done       — one-cycle end-of-block marker, then back to Token
module Lz4DecompFsm (
  input logic clk,
  input logic rst,
  input logic in_valid,
  input logic [7:0] in_data,
  output logic in_ready,
  input logic in_last,
  output logic out_valid,
  output logic [7:0] out_data,
  input logic out_ready,
  output logic [11:0] hist_rd_addr,
  input logic [7:0] hist_rd_data,
  output logic hist_wr_en,
  output logic [11:0] hist_wr_addr,
  output logic [7:0] hist_wr_data
);

  typedef enum logic [2:0] {
    TOKEN = 3'd0,
    LITLENEXT = 3'd1,
    LITCOPY = 3'd2,
    MATCHOFFLO = 3'd3,
    MATCHOFFHI = 3'd4,
    MATCHLENEXT = 3'd5,
    MATCHCOPY = 3'd6,
    DONE = 3'd7
  } Lz4DecompFsm_state_t;
  
  Lz4DecompFsm_state_t state_r, state_next;
  
  logic [15:0] lit_len_r;
  logic [15:0] match_len_r;
  logic [7:0] match_off_lo_r;
  logic [11:0] out_ptr_r;
  logic [11:0] match_rd_ptr_r;
  logic ml_need_ext_r;
  
  logic [7:0] token_ll;
  assign token_ll = 8'($unsigned(in_data[7:4]));
  logic [7:0] token_ml;
  assign token_ml = 8'($unsigned(in_data[3:0]));
  logic [15:0] match_off_full;
  assign match_off_full = 16'($unsigned(in_data)) << 8 | 16'($unsigned(match_off_lo_r));
  logic [11:0] neg_off12;
  assign neg_off12 = 12'(12'd0 - 12'(match_off_full));
  logic [11:0] match_rd_ptr_new;
  assign match_rd_ptr_new = (12 > $bits(neg_off12) ? 12 : $bits(neg_off12))'(out_ptr_r + neg_off12);
  
  always_ff @(posedge clk) begin
    if (rst) begin
      state_r <= TOKEN;
      lit_len_r <= 0;
      match_len_r <= 0;
      match_off_lo_r <= 0;
      out_ptr_r <= 0;
      match_rd_ptr_r <= 0;
      ml_need_ext_r <= 1'b0;
    end else begin
      state_r <= state_next;
      unique case (state_r)
        TOKEN: begin
          // Compressed input stream
          // Decompressed output stream
          // History RAM interface
          // Datapath registers
          // Combinational helpers — always active, valid when their inputs are meaningful
          // 16-bit match offset from two bytes: high byte on in_data, low byte in register
          // Match read pointer: out_ptr_r − offset (modular 12-bit)
          // ── Token: consume token byte ─────────────────────────────────────────────
          if (in_valid) begin
            lit_len_r <= 16'($unsigned(token_ll));
            match_len_r <= 16'($unsigned(9'($unsigned(token_ml)) + 9'd4));
            ml_need_ext_r <= token_ml == 8'd15;
          end
        end
        LITLENEXT: begin
          // ── LitLenExt: accumulate literal-length extension bytes ─────────────────
          // Extension byte 255 → add 255, keep reading; < 255 → add and proceed.
          if (in_valid) begin
            lit_len_r <= 16'(lit_len_r + 16'($unsigned(in_data)));
          end
        end
        LITCOPY: begin
          // ── LitCopy: stream literals from input to output and into history ────────
          if (in_valid) begin
            out_ptr_r <= 12'(out_ptr_r + 12'd1);
            lit_len_r <= 16'(lit_len_r - 16'd1);
          end
        end
        MATCHOFFLO: begin
          // ── MatchOffLo: read low byte of match offset ─────────────────────────────
          if (in_valid) begin
            match_off_lo_r <= in_data;
          end
        end
        MATCHOFFHI: begin
          // ── MatchOffHi: read high byte; compute circular history read pointer ─────
          if (in_valid) begin
            match_rd_ptr_r <= match_rd_ptr_new;
          end
        end
        MATCHLENEXT: begin
          // ── MatchLenExt: accumulate match-length extension bytes ──────────────────
          if (in_valid) begin
            match_len_r <= 16'(match_len_r + 16'($unsigned(in_data)));
          end
        end
        MATCHCOPY: begin
          // ── MatchCopy: replay bytes from history into output and history ──────────
          // One byte per cycle; output is combinational from async RAM read.
          // No input consumed (in_ready = false from default).
          out_ptr_r <= 12'(out_ptr_r + 12'd1);
          match_rd_ptr_r <= 12'(match_rd_ptr_r + 12'd1);
          match_len_r <= 16'(match_len_r - 16'd1);
        end
        default: ;
      endcase
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    unique case (state_r)
      TOKEN: begin
        if (in_valid & (token_ll == 8'd15)) state_next = LITLENEXT;
        else if (in_valid & (token_ll != 8'd15) & (token_ll != 8'd0)) state_next = LITCOPY;
        else if (in_valid & (token_ll == 8'd0) & in_last) state_next = DONE;
        else if (in_valid & (token_ll == 8'd0) & !in_last) state_next = MATCHOFFLO;
      end
      LITLENEXT: begin
        if (in_valid & (in_data != 8'd255)) state_next = LITCOPY;
      end
      LITCOPY: begin
        if (in_valid & (lit_len_r == 16'd1) & in_last) state_next = DONE;
        else if (in_valid & (lit_len_r == 16'd1) & !in_last) state_next = MATCHOFFLO;
      end
      MATCHOFFLO: begin
        if (in_valid) state_next = MATCHOFFHI;
      end
      MATCHOFFHI: begin
        if (in_valid & ml_need_ext_r) state_next = MATCHLENEXT;
        else if (in_valid & !ml_need_ext_r) state_next = MATCHCOPY;
      end
      MATCHLENEXT: begin
        if (in_valid & (in_data != 8'd255)) state_next = MATCHCOPY;
      end
      MATCHCOPY: begin
        if (match_len_r == 16'd1) state_next = TOKEN;
      end
      DONE: begin
        state_next = TOKEN;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    in_ready = 1'b0;
    out_valid = 1'b0;
    out_data = 0;
    hist_rd_addr = 0;
    hist_wr_en = 1'b0;
    hist_wr_addr = 0;
    hist_wr_data = 0;
    unique case (state_r)
      TOKEN: begin
        in_ready = 1'b1;
      end
      LITLENEXT: begin
        in_ready = 1'b1;
      end
      LITCOPY: begin
        in_ready = 1'b1;
        out_valid = in_valid;
        out_data = in_data;
        hist_wr_en = in_valid;
        hist_wr_addr = out_ptr_r;
        hist_wr_data = in_data;
      end
      MATCHOFFLO: begin
        in_ready = 1'b1;
      end
      MATCHOFFHI: begin
        in_ready = 1'b1;
      end
      MATCHLENEXT: begin
        in_ready = 1'b1;
      end
      MATCHCOPY: begin
        out_valid = 1'b1;
        out_data = hist_rd_data;
        hist_rd_addr = match_rd_ptr_r;
        hist_wr_en = 1'b1;
        hist_wr_addr = out_ptr_r;
        hist_wr_data = hist_rd_data;
      end
      DONE: begin
      end
      default: ;
    endcase
  end
  
  // synopsys translate_off
  _auto_legal_state: assert property (@(posedge clk) !rst |-> state_r < 8)
    else $fatal(1, "FSM ILLEGAL STATE: Lz4DecompFsm.state_r = %0d", state_r);
  _auto_reach_Token: cover property (@(posedge clk) state_r == TOKEN);
  _auto_reach_LitLenExt: cover property (@(posedge clk) state_r == LITLENEXT);
  _auto_reach_LitCopy: cover property (@(posedge clk) state_r == LITCOPY);
  _auto_reach_MatchOffLo: cover property (@(posedge clk) state_r == MATCHOFFLO);
  _auto_reach_MatchOffHi: cover property (@(posedge clk) state_r == MATCHOFFHI);
  _auto_reach_MatchLenExt: cover property (@(posedge clk) state_r == MATCHLENEXT);
  _auto_reach_MatchCopy: cover property (@(posedge clk) state_r == MATCHCOPY);
  _auto_reach_Done: cover property (@(posedge clk) state_r == DONE);
  _auto_tr_TOKEN_to_LITLENEXT: cover property (@(posedge clk) state_r == TOKEN && state_next == LITLENEXT);
  _auto_tr_TOKEN_to_LITCOPY: cover property (@(posedge clk) state_r == TOKEN && state_next == LITCOPY);
  _auto_tr_TOKEN_to_DONE: cover property (@(posedge clk) state_r == TOKEN && state_next == DONE);
  _auto_tr_TOKEN_to_MATCHOFFLO: cover property (@(posedge clk) state_r == TOKEN && state_next == MATCHOFFLO);
  _auto_tr_LITLENEXT_to_LITCOPY: cover property (@(posedge clk) state_r == LITLENEXT && state_next == LITCOPY);
  _auto_tr_LITCOPY_to_DONE: cover property (@(posedge clk) state_r == LITCOPY && state_next == DONE);
  _auto_tr_LITCOPY_to_MATCHOFFLO: cover property (@(posedge clk) state_r == LITCOPY && state_next == MATCHOFFLO);
  _auto_tr_MATCHOFFLO_to_MATCHOFFHI: cover property (@(posedge clk) state_r == MATCHOFFLO && state_next == MATCHOFFHI);
  _auto_tr_MATCHOFFHI_to_MATCHLENEXT: cover property (@(posedge clk) state_r == MATCHOFFHI && state_next == MATCHLENEXT);
  _auto_tr_MATCHOFFHI_to_MATCHCOPY: cover property (@(posedge clk) state_r == MATCHOFFHI && state_next == MATCHCOPY);
  _auto_tr_MATCHLENEXT_to_MATCHCOPY: cover property (@(posedge clk) state_r == MATCHLENEXT && state_next == MATCHCOPY);
  _auto_tr_MATCHCOPY_to_TOKEN: cover property (@(posedge clk) state_r == MATCHCOPY && state_next == TOKEN);
  _auto_tr_DONE_to_TOKEN: cover property (@(posedge clk) state_r == DONE && state_next == TOKEN);
  // synopsys translate_on

endmodule

// ── Done: end-of-block; one idle cycle then back to Token ─────────────────
// ── Lz4Decomp: top-level wrapper ──────────────────────────────────────────────
// Instantiates Lz4DecompFsm + Lz4HistBuf and connects them via internal wires.
module Lz4Decomp (
  input logic clk,
  input logic rst,
  input logic in_valid,
  input logic [7:0] in_data,
  output logic in_ready,
  input logic in_last,
  output logic out_valid,
  output logic [7:0] out_data,
  input logic out_ready
);

  // Internal wires: FSM ↔ history RAM
  logic [11:0] hist_rd_addr_w;
  logic [7:0] hist_rd_data_w;
  logic hist_wr_en_w;
  logic [11:0] hist_wr_addr_w;
  logic [7:0] hist_wr_data_w;
  // Internal wires: FSM outputs to module ports
  logic in_ready_w;
  logic out_valid_w;
  logic [7:0] out_data_w;
  Lz4DecompFsm fsm_i (
    .clk(clk),
    .rst(rst),
    .in_valid(in_valid),
    .in_data(in_data),
    .in_ready(in_ready_w),
    .in_last(in_last),
    .out_valid(out_valid_w),
    .out_data(out_data_w),
    .out_ready(out_ready),
    .hist_rd_addr(hist_rd_addr_w),
    .hist_rd_data(hist_rd_data_w),
    .hist_wr_en(hist_wr_en_w),
    .hist_wr_addr(hist_wr_addr_w),
    .hist_wr_data(hist_wr_data_w)
  );
  Lz4HistBuf hist_i (
    .clk(clk),
    .rd_port_addr(hist_rd_addr_w),
    .rd_port_rdata(hist_rd_data_w),
    .wr_port_en(hist_wr_en_w),
    .wr_port_addr(hist_wr_addr_w),
    .wr_port_wdata(hist_wr_data_w)
  );
  assign in_ready = in_ready_w;
  assign out_valid = out_valid_w;
  assign out_data = out_data_w;

endmodule

