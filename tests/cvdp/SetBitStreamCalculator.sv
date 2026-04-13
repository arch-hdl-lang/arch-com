module SetBitStreamCalculator #(
  parameter int p_max_set_bit_count_width = 8
) (
  input logic i_clk,
  input logic i_rst_n,
  input logic i_ready,
  input logic i_bit_in,
  output logic [p_max_set_bit_count_width-1:0] o_set_bit_count
);

  logic [p_max_set_bit_count_width-1:0] count;
  logic prev_ready;
  logic [8:0] sum_wide;
  logic [p_max_set_bit_count_width-1:0] next_count;
  always_comb begin
    sum_wide = 9'(9'($unsigned(count)) + 9'($unsigned(i_bit_in)));
    if (sum_wide[8:8]) begin
      next_count = 8'd255;
    end else begin
      next_count = p_max_set_bit_count_width'(sum_wide);
    end
  end
  always_ff @(posedge i_clk or negedge i_rst_n) begin
    if ((!i_rst_n)) begin
      count <= 0;
      prev_ready <= 0;
    end else begin
      prev_ready <= i_ready;
      if (i_ready & ~prev_ready) begin
        count <= 0;
      end else if (i_ready) begin
        count <= next_count;
      end
    end
  end
  assign o_set_bit_count = count;

endmodule

