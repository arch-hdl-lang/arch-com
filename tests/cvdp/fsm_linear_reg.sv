module fsm_linear_reg #(
  parameter int DATA_WIDTH = 16
) (
  input logic clk,
  input logic reset,
  input logic start,
  input logic signed [DATA_WIDTH-1:0] x_in,
  input logic signed [DATA_WIDTH-1:0] w_in,
  input logic signed [DATA_WIDTH-1:0] b_in,
  output logic signed [DATA_WIDTH * 2-1:0] result1,
  output logic signed [DATA_WIDTH + 1-1:0] result2,
  output logic done
);

  logic [2-1:0] state;
  logic signed [DATA_WIDTH * 2-1:0] buf_result1;
  logic signed [DATA_WIDTH + 1-1:0] buf_result2;
  logic buf_done;
  logic signed [DATA_WIDTH-1:0] x_shifted;
  assign x_shifted = x_in >>> 2;
  logic signed [DATA_WIDTH * 2-1:0] product;
  assign product = (DATA_WIDTH * 2)'({{(DATA_WIDTH * 2-$bits(w_in)){w_in[$bits(w_in)-1]}}, w_in} * {{(DATA_WIDTH * 2-$bits(x_in)){x_in[$bits(x_in)-1]}}, x_in});
  always_ff @(posedge clk or posedge reset) begin
    if (reset) begin
      buf_done <= 0;
      buf_result1 <= 0;
      buf_result2 <= 0;
      done <= 0;
      result1 <= 0;
      result2 <= 0;
      state <= 0;
    end else begin
      result1 <= buf_result1;
      result2 <= buf_result2;
      done <= buf_done;
      if (state == 0) begin
        if (start) begin
          state <= 1;
        end
        buf_result1 <= 0;
        buf_result2 <= 0;
        buf_done <= 1'b0;
      end else if (state == 1) begin
        buf_result1 <= product >>> 1;
        buf_result2 <= (DATA_WIDTH + 1)'({{(DATA_WIDTH + 1-$bits(b_in)){b_in[$bits(b_in)-1]}}, b_in} + {{(DATA_WIDTH + 1-$bits(x_shifted)){x_shifted[$bits(x_shifted)-1]}}, x_shifted});
        buf_done <= 1'b0;
        state <= 2;
      end else if (state == 2) begin
        buf_done <= 1'b1;
        state <= 0;
      end else begin
        state <= 0;
        buf_result1 <= 0;
        buf_result2 <= 0;
        buf_done <= 1'b0;
      end
    end
  end

endmodule

