// domain SysDomain
//   freq_mhz: 100

module HistBuf #(
  parameter int DEPTH = 4096,
  parameter int DATA_WIDTH = 8
) (
  input logic clk,
  input logic rd_port_en,
  input logic [11:0] rd_port_addr,
  output logic [7:0] rd_port_data,
  input logic wr_port_en,
  input logic [11:0] wr_port_addr,
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

endmodule

// LZ4 block decompressor.
// Accepts a stream of raw LZ4-block bytes (token + literals + offset + match)
// one byte per cycle via in_valid/in_data/in_ready handshake.
// Emits decoded bytes via out_valid/out_data/out_ready.
// last_seq must be asserted with the token byte of the final sequence
// (the one with no match section).  done pulses for one cycle after the
// last output byte is consumed.
//
// States:
//   0 = WAIT_TOKEN  receive token byte, decode lit/mat nibbles
//   1 = LIT_WAIT    wait for next literal byte on input
//   2 = LIT_EMIT    present literal byte to output, write to history
//   3 = OFF_BYTE0   receive offset low byte
//   4 = OFF_BYTE1   receive offset high nibble, start match
//   5 = MAT_RD      issue history read, absorb 1-cycle RAM latency
//   6 = MAT_EMIT    present history byte to output, write to history
//   7 = DONE        pulse done for one cycle then return to WAIT_TOKEN
module Lz4BlockDec (
  input logic clk,
  input logic rst,
  input logic in_valid,
  input logic [7:0] in_data,
  output logic in_ready,
  input logic last_seq,
  output logic out_valid,
  output logic [7:0] out_data,
  input logic out_ready,
  output logic done
);

  logic [3:0] tok_nib_hi;
  logic [3:0] tok_nib_lo;
  logic [7:0] tok_lit_len;
  logic [7:0] tok_mat_len;
  logic [11:0] off12;
  logic [2:0] state;
  logic [7:0] lit_len;
  logic [7:0] mat_len;
  logic [7:0] byte_r;
  logic [7:0] off_lo;
  logic [11:0] wr_ptr;
  logic [11:0] rd_ptr;
  logic is_last;
  // Token nibble decode
  assign tok_nib_hi = 4'(in_data >> 4);
  assign tok_nib_lo = 4'(in_data);
  assign tok_lit_len = 8'($unsigned(tok_nib_hi));
  // match length = low nibble + 4 (minimum match is 4)
  assign tok_mat_len = 8'(8'($unsigned(tok_nib_lo)) + 8'd4);
  // 12-bit offset assembled from two bytes (LZ4 uses little-endian 16-bit
  // offset; we cap history at 4096 so only the low 12 bits are significant)
  assign off12 = {4'(in_data), off_lo};
  logic hist_rd_en;
  logic [11:0] hist_rd_addr;
  logic hist_wr_en;
  logic [7:0] hist_wr_data;
  // Latch for the 1-cycle-latency RAM read result
  logic [7:0] hist_rd_data = 0;
  HistBuf hist (
    .clk(clk),
    .rd_port_en(hist_rd_en),
    .rd_port_addr(hist_rd_addr),
    .rd_port_data(hist_rd_data),
    .wr_port_en(hist_wr_en),
    .wr_port_addr(wr_ptr),
    .wr_port_data(hist_wr_data)
  );
  assign in_ready = state == 3'd0 || state == 3'd1 || state == 3'd3 || state == 3'd4;
  assign out_valid = state == 3'd2 || state == 3'd6;
  assign out_data = state == 3'd2 ? byte_r : hist_rd_data;
  assign done = state == 3'd7;
  assign hist_rd_en = state == 3'd5;
  assign hist_rd_addr = rd_ptr;
  assign hist_wr_en = (state == 3'd2 || state == 3'd6) && out_ready;
  assign hist_wr_data = state == 3'd2 ? byte_r : hist_rd_data;
  always_ff @(posedge clk) begin
    if (rst) begin
      byte_r <= 0;
      is_last <= 1'b0;
      lit_len <= 0;
      mat_len <= 0;
      off_lo <= 0;
      rd_ptr <= 0;
      state <= 0;
      wr_ptr <= 0;
    end else begin
      if (state == 3'd0) begin
        if (in_valid) begin
          lit_len <= tok_lit_len;
          mat_len <= tok_mat_len;
          is_last <= last_seq;
          if (tok_nib_hi == 4'd0) begin
            if (last_seq) begin
              state <= 3'd7;
            end else begin
              state <= 3'd3;
            end
          end else begin
            state <= 3'd1;
          end
        end
      end else if (state == 3'd1) begin
        if (in_valid) begin
          byte_r <= in_data;
          state <= 3'd2;
        end
      end else if (state == 3'd2) begin
        if (out_ready) begin
          wr_ptr <= 12'(wr_ptr + 12'd1);
          if (lit_len <= 8'd1) begin
            lit_len <= 8'd0;
            if (is_last) begin
              state <= 3'd7;
            end else begin
              state <= 3'd3;
            end
          end else begin
            lit_len <= 8'(lit_len - 8'd1);
            state <= 3'd1;
          end
        end
      end else if (state == 3'd3) begin
        if (in_valid) begin
          off_lo <= in_data;
          state <= 3'd4;
        end
      end else if (state == 3'd4) begin
        if (in_valid) begin
          rd_ptr <= 12'(wr_ptr - off12);
          state <= 3'd5;
        end
      end else if (state == 3'd5) begin
        state <= 3'd6;
      end else if (state == 3'd6) begin
        if (out_ready) begin
          wr_ptr <= 12'(wr_ptr + 12'd1);
          rd_ptr <= 12'(rd_ptr + 12'd1);
          if (mat_len <= 8'd1) begin
            mat_len <= 8'd0;
            state <= 3'd0;
          end else begin
            mat_len <= 8'(mat_len - 8'd1);
            state <= 3'd5;
          end
        end
      end else if (state == 3'd7) begin
        state <= 3'd0;
      end
    end
  end

endmodule

