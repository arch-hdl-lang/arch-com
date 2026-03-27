module SetBitStreamCalculator #(
  parameter int p_max_set_bit_count_width = 8
) (
  input logic i_clk,
  input logic i_rst_n,
  input logic i_bit_in,
  input logic i_ready,
  output logic [p_max_set_bit_count_width-1:0] o_set_bit_count
);

  logic prev_ready;
  logic [p_max_set_bit_count_width-1:0] max_val;
  assign max_val = 8'd255;
  always_ff @(posedge i_clk or negedge i_rst_n) begin
    if ((!i_rst_n)) begin
      o_set_bit_count <= 0;
      prev_ready <= 0;
    end else begin
      prev_ready <= i_ready;
      if (i_ready) begin
        if (~prev_ready) begin
          o_set_bit_count <= 0;
        end else if (i_bit_in) begin
          if (o_set_bit_count < max_val) begin
            o_set_bit_count <= p_max_set_bit_count_width'(o_set_bit_count + 1'd1);
          end
        end
      end
    end
  end

endmodule

