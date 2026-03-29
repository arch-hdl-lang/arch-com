module sgd_linear_regression #(
  parameter int DATA_WIDTH = 16,
  parameter int LEARNING_RATE = 1,
  parameter int NBW_PRED = 2 * DATA_WIDTH + 1,
  parameter int NBW_ERROR = NBW_PRED + 1,
  parameter int NBW_DELTA = 3 + NBW_ERROR + DATA_WIDTH
) (
  input logic clk,
  input logic reset,
  input logic signed [DATA_WIDTH-1:0] x_in,
  input logic signed [DATA_WIDTH-1:0] y_true,
  output logic signed [DATA_WIDTH-1:0] w_out,
  output logic signed [DATA_WIDTH-1:0] b_out
);

  logic signed [NBW_DELTA-1:0] lr;
  assign lr = $signed(NBW_DELTA'($unsigned(LEARNING_RATE)));
  logic signed [NBW_PRED-1:0] y_pred;
  logic signed [NBW_ERROR-1:0] error;
  logic signed [NBW_DELTA-1:0] delta_w;
  logic signed [NBW_DELTA-1:0] delta_b;
  assign y_pred = NBW_PRED'(NBW_PRED'({{(NBW_PRED-$bits(w_out)){w_out[$bits(w_out)-1]}}, w_out} * {{(NBW_PRED-$bits(x_in)){x_in[$bits(x_in)-1]}}, x_in}) + {{(NBW_PRED-$bits(b_out)){b_out[$bits(b_out)-1]}}, b_out});
  assign error = NBW_ERROR'({{(NBW_ERROR-$bits(y_true)){y_true[$bits(y_true)-1]}}, y_true} - {{(NBW_ERROR-$bits(y_pred)){y_pred[$bits(y_pred)-1]}}, y_pred});
  assign delta_w = NBW_DELTA'(NBW_DELTA'(lr * {{(NBW_DELTA-$bits(error)){error[$bits(error)-1]}}, error}) * {{(NBW_DELTA-$bits(x_in)){x_in[$bits(x_in)-1]}}, x_in});
  assign delta_b = NBW_DELTA'(lr * {{(NBW_DELTA-$bits(error)){error[$bits(error)-1]}}, error});
  always_ff @(posedge clk or posedge reset) begin
    if (reset) begin
      b_out <= 0;
      w_out <= 0;
    end else begin
      if (reset) begin
        w_out <= 0;
        b_out <= 0;
      end else begin
        w_out <= DATA_WIDTH'({{(NBW_DELTA-$bits(w_out)){w_out[$bits(w_out)-1]}}, w_out} + delta_w);
        b_out <= DATA_WIDTH'({{(NBW_DELTA-$bits(b_out)){b_out[$bits(b_out)-1]}}, b_out} + delta_b);
      end
    end
  end

endmodule

