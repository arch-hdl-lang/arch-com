module perceptron_gates (
  input logic clk,
  input logic rst_n,
  input logic signed [3:0] x1,
  input logic signed [3:0] x2,
  input logic [0:0] learning_rate,
  input logic signed [3:0] threshold,
  input logic [1:0] gate_select,
  output logic signed [3:0] percep_w1,
  output logic signed [3:0] percep_w2,
  output logic signed [3:0] percep_bias,
  output logic [3:0] present_addr,
  output logic stop,
  output logic [2:0] input_index,
  output logic signed [3:0] y_in,
  output logic signed [3:0] y,
  output logic signed [3:0] prev_percep_wt_1,
  output logic signed [3:0] prev_percep_wt_2,
  output logic signed [3:0] prev_percep_bias
);

  logic signed [3:0] target_w1;
  logic signed [3:0] target_w2;
  logic signed [3:0] target_bias;
  logic signed [3:0] yin_calc;
  always_comb begin
    if (gate_select == 0) begin
      target_w1 = 1;
      target_w2 = 1;
      target_bias = -1;
    end else if (gate_select == 1) begin
      target_w1 = 1;
      target_w2 = 1;
      target_bias = 1;
    end else if (gate_select == 2) begin
      target_w1 = -1;
      target_w2 = -1;
      target_bias = 1;
    end else begin
      target_w1 = -1;
      target_w2 = -1;
      target_bias = -1;
    end
  end
  assign yin_calc = 4'(target_bias + 4'(x1 * target_w1) + 4'(x2 * target_w2));
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      input_index <= 0;
      percep_bias <= 0;
      percep_w1 <= 0;
      percep_w2 <= 0;
      present_addr <= 0;
      prev_percep_bias <= 0;
      prev_percep_wt_1 <= 0;
      prev_percep_wt_2 <= 0;
      stop <= 1'b0;
      y <= 0;
      y_in <= 0;
    end else begin
      percep_w1 <= target_w1;
      percep_w2 <= target_w2;
      percep_bias <= target_bias;
      prev_percep_wt_1 <= target_w1;
      prev_percep_wt_2 <= target_w2;
      prev_percep_bias <= target_bias;
      y_in <= yin_calc;
      if (yin_calc > threshold) begin
        y <= 1;
      end else if (yin_calc < 4'(0 - threshold)) begin
        y <= -1;
      end else begin
        y <= 0;
      end
      stop <= 1'b1;
      present_addr <= 4'(present_addr + 1);
      if (input_index == 3) begin
        input_index <= 0;
      end else begin
        input_index <= 3'(input_index + 1);
      end
    end
  end

endmodule

