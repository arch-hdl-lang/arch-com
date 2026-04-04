module perceptron_gates (
  input logic clk,
  input logic rst_n,
  input logic signed [4-1:0] x1,
  input logic signed [4-1:0] x2,
  input logic [1-1:0] learning_rate,
  input logic signed [4-1:0] threshold,
  input logic [2-1:0] gate_select,
  output logic signed [4-1:0] percep_w1,
  output logic signed [4-1:0] percep_w2,
  output logic signed [4-1:0] percep_bias,
  output logic [4-1:0] present_addr,
  output logic stop,
  output logic [3-1:0] input_index,
  output logic signed [4-1:0] y_in,
  output logic signed [4-1:0] y,
  output logic signed [4-1:0] prev_percep_wt_1,
  output logic signed [4-1:0] prev_percep_wt_2,
  output logic signed [4-1:0] prev_percep_bias
);

  // Gate target outputs (combinational)
  logic signed [4-1:0] t1;
  logic signed [4-1:0] t2;
  logic signed [4-1:0] t3;
  logic signed [4-1:0] t4;
  always_comb begin
    if (gate_select == 0) begin
      t1 = 1;
      t2 = -1;
      t3 = -1;
      t4 = -1;
    end else if (gate_select == 1) begin
      t1 = 1;
      t2 = 1;
      t3 = 1;
      t4 = -1;
    end else if (gate_select == 2) begin
      t1 = 1;
      t2 = 1;
      t3 = 1;
      t4 = -1;
    end else begin
      t1 = 1;
      t2 = -1;
      t3 = -1;
      t4 = -1;
    end
  end
  // Target selection based on input_index
  logic signed [4-1:0] target_val;
  always_comb begin
    if (input_index == 0) begin
      target_val = t1;
    end else if (input_index == 1) begin
      target_val = t2;
    end else if (input_index == 2) begin
      target_val = t3;
    end else begin
      target_val = t4;
    end
  end
  // Compute weight/bias updates
  logic signed [4-1:0] wt1_update;
  logic signed [4-1:0] wt2_update;
  logic signed [4-1:0] bias_update;
  logic signed [4-1:0] lr_s;
  always_comb begin
    lr_s = $signed(4'($unsigned(learning_rate)));
    if (y != target_val) begin
      wt1_update = 4'(lr_s * x1 * target_val);
      wt2_update = 4'(lr_s * x2 * target_val);
      bias_update = 4'(lr_s * target_val);
    end else begin
      wt1_update = 0;
      wt2_update = 0;
      bias_update = 0;
    end
  end
  // Convergence check (avoid && codegen bug by using intermediate wire)
  logic converged;
  always_comb begin
    if (wt1_update == prev_percep_wt_1) begin
      if (wt2_update == prev_percep_wt_2) begin
        if (bias_update == prev_percep_bias) begin
          converged = 1'b1;
        end else begin
          converged = 1'b0;
        end
      end else begin
        converged = 1'b0;
      end
    end else begin
      converged = 1'b0;
    end
  end
  // Microcode ROM sequencer
  logic [4-1:0] mc;
  // Compute y_in combinationally for use in seq
  logic signed [4-1:0] yin_calc;
  logic signed [4-1:0] neg_thresh;
  assign yin_calc = 4'(percep_bias + 4'(x1 * percep_w1) + 4'(x2 * percep_w2));
  assign neg_thresh = 4'(0 - threshold);
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      input_index <= 0;
      mc <= 0;
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
      if (mc == 0) begin
        // Action 0: Initialize
        percep_w1 <= 0;
        percep_w2 <= 0;
        percep_bias <= 0;
        input_index <= 0;
        stop <= 1'b0;
        prev_percep_wt_1 <= 0;
        prev_percep_wt_2 <= 0;
        prev_percep_bias <= 0;
        y_in <= 0;
        y <= 0;
        mc <= 1;
        present_addr <= 1;
      end else if (mc == 1) begin
        // Action 1: Compute y_in and y
        y_in <= yin_calc;
        if (yin_calc > threshold) begin
          y <= 1;
        end else if (yin_calc < neg_thresh) begin
          y <= -1;
        end else begin
          y <= 0;
        end
        mc <= 2;
        present_addr <= 2;
      end else if (mc == 2) begin
        // Action 2: target selected combinationally, advance
        mc <= 3;
        present_addr <= 3;
      end else if (mc == 3) begin
        // Action 3: Update weights and bias
        percep_w1 <= 4'(percep_w1 + wt1_update);
        percep_w2 <= 4'(percep_w2 + wt2_update);
        percep_bias <= 4'(percep_bias + bias_update);
        mc <= 4;
        present_addr <= 4;
      end else if (mc == 4) begin
        // Action 4: Check convergence
        if (converged) begin
          stop <= 1'b1;
        end else begin
          stop <= 1'b0;
        end
        prev_percep_wt_1 <= wt1_update;
        prev_percep_wt_2 <= wt2_update;
        prev_percep_bias <= bias_update;
        mc <= 5;
        present_addr <= 5;
      end else if (mc == 5) begin
        // Action 5: Next input or loop
        if (input_index == 3) begin
          input_index <= 0;
        end else begin
          input_index <= 3'(input_index + 1);
        end
        mc <= 1;
        present_addr <= 5;
      end else begin
        mc <= 0;
        present_addr <= 0;
      end
    end
  end

endmodule

