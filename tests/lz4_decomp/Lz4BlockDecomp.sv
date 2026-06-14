// LZ4 raw-block decompressor (CAST LZ4SNP-D reference implementation).
//
// Streaming byte interface with AXI4-Stream handshake (valid/ready).
// Assert in_last on the last compressed input byte.
// out_last pulses on the last decompressed output byte.
//
// Supports up to 255-byte backward match offsets (8-bit history ring).
// The LZ4 spec uses a 16-bit offset field; the high byte is consumed
// and discarded — the low byte drives the 256-entry history buffer.
//
// Reference: CAST LZ4SNP-D lossless data decompressor IP.
module Lz4BlockDecomp (
  input logic clk,
  input logic rst,
  input logic [7:0] in_data,
  input logic in_valid,
  input logic in_last,
  output logic in_ready,
  output logic [7:0] out_data,
  output logic out_valid,
  output logic out_last,
  input logic out_ready
);

  logic [3:0] ST_TOKEN;
  logic [3:0] ST_EXTRA_LIT;
  logic [3:0] ST_EMIT_LIT;
  logic [3:0] ST_OFF_LO;
  logic [3:0] ST_OFF_HI;
  logic [3:0] ST_EXTRA_MATCH;
  logic [3:0] ST_COPY_MATCH;
  logic [7:0] hist_rd_val;
  // State encoding constants.
  assign ST_TOKEN = 4'd0;
  assign ST_EXTRA_LIT = 4'd1;
  assign ST_EMIT_LIT = 4'd2;
  assign ST_OFF_LO = 4'd3;
  assign ST_OFF_HI = 4'd4;
  assign ST_EXTRA_MATCH = 4'd5;
  assign ST_COPY_MATCH = 4'd6;
  // 256-byte history ring — Vec of regs.
  // UInt<8> index into Vec<_,256> is always in-bounds (2^8 = 256).
  logic [255:0] [7:0] history;
  // FSM state
  logic [3:0] state;
  logic [15:0] lit_remain;
  logic [15:0] match_len;
  logic [3:0] match_nibble;
  logic [15:0] match_remain;
  logic [7:0] offset;
  logic [7:0] copy_rd_ptr;
  logic [7:0] wptr;
  logic saw_last;
  // Combinational read of history at the current copy read pointer.
  assign hist_rd_val = history[copy_rd_ptr];
  // Combinational output logic.
  // Default everything to 0/false; override per state.
  always_comb begin
    in_ready = 1'b0;
    out_data = 0;
    out_valid = 1'b0;
    out_last = 1'b0;
    if (state == ST_EMIT_LIT) begin
      in_ready = out_ready;
      out_valid = in_valid;
      out_data = in_data;
      out_last = in_valid && in_last && lit_remain == 1;
    end else if (state == ST_COPY_MATCH) begin
      out_valid = 1'b1;
      out_data = hist_rd_val;
      out_last = saw_last && match_remain == 1;
    end else begin
      // ST_TOKEN / ST_EXTRA_LIT / ST_OFF_LO / ST_OFF_HI / ST_EXTRA_MATCH
      in_ready = 1'b1;
    end
  end
  // Sequential FSM.
  always_ff @(posedge clk) begin
    if (rst) begin
      copy_rd_ptr <= 0;
      for (int __ri0 = 0; __ri0 < 256; __ri0++) begin
        history[__ri0] <= 0;
      end
      lit_remain <= 0;
      match_len <= 0;
      match_nibble <= 0;
      match_remain <= 0;
      offset <= 0;
      saw_last <= 1'b0;
      state <= 0;
      wptr <= 0;
    end else begin
      if (state == ST_TOKEN) begin
        if (in_valid) begin
          lit_remain <= 16'($unsigned(in_data[7:4]));
          match_nibble <= in_data[3:0];
          match_len <= 16'(16'($unsigned(in_data[3:0])) + 4);
          saw_last <= in_last;
          if (in_data[7:4] == 15) begin
            state <= ST_EXTRA_LIT;
          end else if (in_data[7:4] != 0) begin
            state <= ST_EMIT_LIT;
          end else begin
            state <= ST_OFF_LO;
          end
        end
      end else if (state == ST_EXTRA_LIT) begin
        if (in_valid) begin
          lit_remain <= 16'(lit_remain + 16'($unsigned(in_data)));
          saw_last <= in_last;
          if (in_data != 255) begin
            state <= ST_EMIT_LIT;
          end
        end
      end else if (state == ST_EMIT_LIT) begin
        if (in_valid && out_ready) begin
          history[wptr] <= in_data;
          wptr <= (8 > 1 ? 8 : 1)'(wptr + 1);
          lit_remain <= 16'(lit_remain - 1);
          saw_last <= in_last;
          if (lit_remain == 1) begin
            if (in_last) begin
              state <= ST_TOKEN;
            end else begin
              state <= ST_OFF_LO;
            end
          end
        end
      end else if (state == ST_OFF_LO) begin
        if (in_valid) begin
          offset <= in_data;
          saw_last <= in_last;
          state <= ST_OFF_HI;
        end
      end else if (state == ST_OFF_HI) begin
        if (in_valid) begin
          saw_last <= in_last;
          if (match_nibble == 15) begin
            state <= ST_EXTRA_MATCH;
          end else begin
            match_remain <= match_len;
            copy_rd_ptr <= 8'(wptr - offset);
            state <= ST_COPY_MATCH;
          end
        end
      end else if (state == ST_EXTRA_MATCH) begin
        if (in_valid) begin
          saw_last <= in_last;
          match_len <= 16'(match_len + 16'($unsigned(in_data)));
          if (in_data != 255) begin
            match_remain <= 16'(match_len + 16'($unsigned(in_data)));
            copy_rd_ptr <= 8'(wptr - offset);
            state <= ST_COPY_MATCH;
          end
        end
      end else if (state == ST_COPY_MATCH) begin
        if (out_ready) begin
          history[wptr] <= hist_rd_val;
          wptr <= (8 > 1 ? 8 : 1)'(wptr + 1);
          copy_rd_ptr <= (8 > 1 ? 8 : 1)'(copy_rd_ptr + 1);
          match_remain <= 16'(match_remain - 1);
          if (match_remain == 1) begin
            state <= ST_TOKEN;
          end
        end
      end
    end
  end
  // synopsys translate_off
  // Auto-generated safety assertions (bounds / divide-by-zero)
  _auto_bound_vec_0: assert property (@(posedge clk) disable iff (rst) (((((!(state == ST_TOKEN)) && (!(state == ST_EXTRA_LIT))) && (state == ST_EMIT_LIT)) && (in_valid && out_ready)) |-> (int'(wptr) < (256))))
    else $fatal(1, "BOUNDS VIOLATION: Lz4BlockDecomp._auto_bound_vec_0");
  _auto_bound_vec_1: assert property (@(posedge clk) disable iff (rst) (((((((((!(state == ST_TOKEN)) && (!(state == ST_EXTRA_LIT))) && (!(state == ST_EMIT_LIT))) && (!(state == ST_OFF_LO))) && (!(state == ST_OFF_HI))) && (!(state == ST_EXTRA_MATCH))) && (state == ST_COPY_MATCH)) && (out_ready)) |-> (int'(wptr) < (256))))
    else $fatal(1, "BOUNDS VIOLATION: Lz4BlockDecomp._auto_bound_vec_1");
  // synopsys translate_on

endmodule

