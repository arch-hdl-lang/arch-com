module dot_product (
  input logic clk_in,
  input logic reset_in,
  input logic start_in,
  input logic [6:0] dot_length_in,
  input logic [7:0] vector_a_in,
  input logic vector_a_valid_in,
  input logic [15:0] vector_b_in,
  input logic vector_b_valid_in,
  output logic [31:0] dot_product_out,
  output logic dot_product_valid_out
);

  logic [1:0] state;
  logic [31:0] accumulator;
  logic [6:0] cnt;
  logic [6:0] length_reg;
  logic [31:0] result_reg;
  logic valid_reg;
  logic [23:0] product;
  assign product = vector_a_in * vector_b_in;
  always_ff @(posedge clk_in or posedge reset_in) begin
    if (reset_in) begin
      accumulator <= 0;
      cnt <= 0;
      length_reg <= 0;
      result_reg <= 0;
      state <= 0;
      valid_reg <= 1'b0;
    end else begin
      if (state == 0) begin
        valid_reg <= 1'b0;
        result_reg <= 0;
        if (start_in) begin
          state <= 1;
          accumulator <= 0;
          cnt <= 0;
          length_reg <= dot_length_in;
        end
      end else if (state == 1) begin
        if (vector_a_valid_in & vector_b_valid_in) begin
          accumulator <= 32'(accumulator + 32'($unsigned(product)));
          cnt <= 7'(cnt + 1);
          if (7'(cnt + 1) == length_reg) begin
            state <= 2;
          end
        end
      end else if (state == 2) begin
        result_reg <= accumulator;
        valid_reg <= 1'b1;
        state <= 0;
      end
    end
  end
  assign dot_product_out = result_reg;
  assign dot_product_valid_out = valid_reg;

endmodule

