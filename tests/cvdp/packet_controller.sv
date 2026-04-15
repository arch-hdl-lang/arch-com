module packet_controller (
  input logic clk,
  input logic rst,
  input logic rx_valid_i,
  input logic [7:0] rx_data_8_i,
  input logic tx_done_tick_i,
  output logic tx_start_o,
  output logic [7:0] tx_data_8_o
);

  logic [7:0] buf0;
  logic [7:0] buf1;
  logic [7:0] buf2;
  logic [7:0] buf3;
  logic [7:0] buf4;
  logic [7:0] buf5;
  logic [7:0] buf6;
  logic [7:0] buf7;
  logic [3:0] byte_cnt;
  logic [7:0] resp0;
  logic [7:0] resp1;
  logic [7:0] resp2;
  logic [7:0] resp3;
  logic [7:0] resp4;
  logic [2:0] tx_idx;
  logic [2:0] st;
  logic [2:0] ST_IDLE;
  assign ST_IDLE = 3'd0;
  logic [2:0] ST_GOT_8;
  assign ST_GOT_8 = 3'd1;
  logic [2:0] ST_BUILD;
  assign ST_BUILD = 3'd2;
  logic [2:0] ST_SEND;
  assign ST_SEND = 3'd3;
  logic [7:0] cksum;
  assign cksum = 8'(buf0 + buf1 + buf2 + buf3 + buf4 + buf5 + buf6 + buf7);
  logic header_ok;
  assign header_ok = (buf0 == 8'd186) & (buf1 == 8'd205);
  logic cksum_ok;
  assign cksum_ok = cksum == 8'd0;
  logic [15:0] num1_val;
  assign num1_val = {buf2, buf3};
  logic [15:0] num2_val;
  assign num2_val = {buf4, buf5};
  logic [7:0] opcode_val;
  assign opcode_val = buf6;
  logic [15:0] result_add;
  assign result_add = 16'(num1_val + num2_val);
  logic [15:0] result_sub;
  assign result_sub = 16'(num1_val - num2_val);
  logic [15:0] result_val;
  always_comb begin
    if (opcode_val == 8'd0) begin
      result_val = result_add;
    end else if (opcode_val == 8'd1) begin
      result_val = result_sub;
    end else begin
      result_val = 16'd0;
    end
  end
  logic [7:0] partial_sum;
  assign partial_sum = 8'(8'd171 + 8'd205 + result_val[15:8] + result_val[7:0]);
  logic [7:0] resp_cksum;
  assign resp_cksum = 8'(8'd0 - partial_sum);
  always_ff @(posedge clk or posedge rst) begin
    if (rst) begin
      buf0 <= 0;
      buf1 <= 0;
      buf2 <= 0;
      buf3 <= 0;
      buf4 <= 0;
      buf5 <= 0;
      buf6 <= 0;
      buf7 <= 0;
      byte_cnt <= 0;
      resp0 <= 0;
      resp1 <= 0;
      resp2 <= 0;
      resp3 <= 0;
      resp4 <= 0;
      st <= 0;
      tx_data_8_o <= 0;
      tx_idx <= 0;
      tx_start_o <= 1'b0;
    end else begin
      if (st == ST_IDLE) begin
        tx_start_o <= 1'b0;
        tx_data_8_o <= 8'd0;
        if (rx_valid_i) begin
          if (byte_cnt == 4'd0) begin
            buf0 <= rx_data_8_i;
          end else if (byte_cnt == 4'd1) begin
            buf1 <= rx_data_8_i;
          end else if (byte_cnt == 4'd2) begin
            buf2 <= rx_data_8_i;
          end else if (byte_cnt == 4'd3) begin
            buf3 <= rx_data_8_i;
          end else if (byte_cnt == 4'd4) begin
            buf4 <= rx_data_8_i;
          end else if (byte_cnt == 4'd5) begin
            buf5 <= rx_data_8_i;
          end else if (byte_cnt == 4'd6) begin
            buf6 <= rx_data_8_i;
          end else if (byte_cnt == 4'd7) begin
            buf7 <= rx_data_8_i;
          end
          if (byte_cnt == 4'd7) begin
            st <= ST_GOT_8;
            byte_cnt <= 4'd0;
          end else begin
            byte_cnt <= 4'(byte_cnt + 4'd1);
          end
        end
      end else if (st == ST_GOT_8) begin
        if (header_ok & cksum_ok) begin
          resp0 <= 8'd171;
          resp1 <= 8'd205;
          resp2 <= result_val[15:8];
          resp3 <= result_val[7:0];
          resp4 <= resp_cksum;
          st <= ST_BUILD;
        end else begin
          st <= ST_IDLE;
        end
      end else if (st == ST_BUILD) begin
        tx_idx <= 3'd0;
        st <= ST_SEND;
      end else if (st == ST_SEND) begin
        tx_start_o <= 1'b1;
        if (tx_idx == 3'd0) begin
          tx_data_8_o <= resp0;
        end else if (tx_idx == 3'd1) begin
          tx_data_8_o <= resp1;
        end else if (tx_idx == 3'd2) begin
          tx_data_8_o <= resp2;
        end else if (tx_idx == 3'd3) begin
          tx_data_8_o <= resp3;
        end else begin
          tx_data_8_o <= resp4;
        end
        if (tx_done_tick_i) begin
          if (tx_idx == 3'd4) begin
            tx_start_o <= 1'b0;
            tx_data_8_o <= 8'd0;
            st <= ST_IDLE;
          end else begin
            tx_idx <= 3'(tx_idx + 3'd1);
          end
        end
      end
    end
  end

endmodule

