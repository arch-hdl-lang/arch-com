module Attenuator (
  input logic clk,
  input logic reset,
  input logic [5-1:0] data,
  output logic [1-1:0] ATTN_CLK,
  output logic [1-1:0] ATTN_DATA,
  output logic [1-1:0] ATTN_LE
);

  logic [1-1:0] clk_div2;
  logic [5-1:0] shift_reg;
  logic [3-1:0] bit_cnt;
  logic [5-1:0] old_data;
  logic [1-1:0] attn_clk_r;
  logic [1-1:0] attn_data_r;
  logic [1-1:0] attn_le_r;
  logic [2-1:0] cur_state;
  logic [2-1:0] IDLE;
  assign IDLE = 0;
  logic [2-1:0] LOAD;
  assign LOAD = 1;
  logic [2-1:0] SHIFT;
  assign SHIFT = 2;
  logic [2-1:0] LATCH;
  assign LATCH = 3;
  logic [1-1:0] zero1;
  assign zero1 = 0;
  assign ATTN_CLK = attn_clk_r;
  assign ATTN_DATA = attn_data_r;
  assign ATTN_LE = attn_le_r;
  always_ff @(posedge clk or posedge reset) begin
    if (reset) begin
      attn_clk_r <= 0;
      attn_data_r <= 0;
      attn_le_r <= 0;
      bit_cnt <= 0;
      clk_div2 <= 0;
      cur_state <= 0;
      old_data <= 0;
      shift_reg <= 0;
    end else begin
      clk_div2 <= ~clk_div2;
      if (cur_state == IDLE) begin
        attn_clk_r <= 0;
        attn_data_r <= 0;
        attn_le_r <= 0;
        if (data != old_data) begin
          cur_state <= LOAD;
          old_data <= data;
        end
      end else if (cur_state == LOAD) begin
        shift_reg <= data;
        bit_cnt <= 0;
        attn_clk_r <= 0;
        attn_data_r <= 0;
        attn_le_r <= 0;
        cur_state <= SHIFT;
      end else if (cur_state == SHIFT) begin
        if (clk_div2 == 1) begin
          attn_data_r <= shift_reg[4:4];
          attn_clk_r <= 1;
          shift_reg <= {shift_reg[3:0], zero1};
          if (bit_cnt == 4) begin
            cur_state <= LATCH;
            bit_cnt <= 0;
          end else begin
            bit_cnt <= 3'(bit_cnt + 1);
          end
        end else begin
          attn_clk_r <= 0;
        end
      end else if (cur_state == LATCH) begin
        attn_clk_r <= 0;
        attn_data_r <= 0;
        attn_le_r <= 1;
        cur_state <= IDLE;
      end
    end
  end

endmodule

