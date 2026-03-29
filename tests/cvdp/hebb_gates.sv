module hebb_gates (
  input logic clk,
  input logic rst,
  input logic start,
  input logic signed [4-1:0] a,
  input logic signed [4-1:0] b,
  input logic [2-1:0] gate_select,
  output logic signed [4-1:0] w1,
  output logic signed [4-1:0] w2,
  output logic signed [4-1:0] bias,
  output logic [4-1:0] present_state,
  output logic [4-1:0] next_state,
  output logic signed [4-1:0] test_x1,
  output logic signed [4-1:0] test_x2,
  output logic signed [4-1:0] expected_output,
  output logic signed [4-1:0] test_output,
  output logic signed [4-1:0] test_result,
  output logic [1-1:0] test_done,
  output logic [4-1:0] test_present_state,
  output logic [4-1:0] test_index
);

  logic signed [4-1:0] x1;
  logic signed [4-1:0] x2;
  logic signed [4-1:0] target_r;
  logic signed [8-1:0] delta_w1;
  logic signed [8-1:0] delta_w2;
  logic signed [4-1:0] delta_b;
  logic [4-1:0] iter;
  logic [4-1:0] wait_cnt;
  logic [2-1:0] gs_r;
  logic [4-1:0] ns;
  logic signed [4-1:0] pos_one;
  assign pos_one = $signed({1'b0, 1'b0, 1'b0, 1'b1});
  logic signed [4-1:0] neg_one;
  assign neg_one = $signed({1'b1, 1'b1, 1'b1, 1'b1});
  logic signed [4-1:0] zero4;
  assign zero4 = $signed({1'b0, 1'b0, 1'b0, 1'b0});
  // Determine if a and b are positive (bipolar: +1 or -1)
  logic a_pos;
  logic b_pos;
  assign a_pos = ~a[3] & a[0];
  assign b_pos = ~b[3] & b[0];
  // Gate target logic based on external inputs
  logic gate_result;
  always_comb begin
    if (gate_select == 0) begin
      gate_result = a_pos & b_pos;
    end else if (gate_select == 1) begin
      gate_result = a_pos | b_pos;
    end else if (gate_select == 2) begin
      gate_result = ~(a_pos & b_pos);
    end else begin
      gate_result = ~(a_pos | b_pos);
    end
  end
  logic signed [4-1:0] tgt_val;
  always_comb begin
    if (gate_result) begin
      tgt_val = pos_one;
    end else begin
      tgt_val = neg_one;
    end
  end
  // Gate target logic based on test inputs (for self-test phase)
  logic tx1_pos;
  logic tx2_pos;
  assign tx1_pos = ~test_x1[3] & test_x1[0];
  assign tx2_pos = ~test_x2[3] & test_x2[0];
  logic test_gate_result;
  always_comb begin
    if (gs_r == 0) begin
      test_gate_result = tx1_pos & tx2_pos;
    end else if (gs_r == 1) begin
      test_gate_result = tx1_pos | tx2_pos;
    end else if (gs_r == 2) begin
      test_gate_result = ~(tx1_pos & tx2_pos);
    end else begin
      test_gate_result = ~(tx1_pos | tx2_pos);
    end
  end
  logic signed [4-1:0] test_tgt_val;
  always_comb begin
    if (test_gate_result) begin
      test_tgt_val = pos_one;
    end else begin
      test_tgt_val = neg_one;
    end
  end
  // Compute net for testing: w1*test_x1 + w2*test_x2 + bias
  logic signed [8-1:0] prod1;
  logic signed [8-1:0] prod2;
  logic signed [8-1:0] bias_ext;
  logic signed [8-1:0] net_sum;
  assign prod1 = $signed(8'(w1 * test_x1));
  assign prod2 = $signed(8'(w2 * test_x2));
  assign bias_ext = $signed({{(8-$bits(bias)){bias[$bits(bias)-1]}}, bias});
  assign net_sum = $signed(8'(prod1 + prod2 + bias_ext));
  // Test input vectors: depend on gate type (stored in gs_r)
  // AND(0): (1,1),(1,-1),(-1,1),(-1,-1)
  // OR(1):  (1,1),(-1,1),(1,-1),(-1,-1)
  // NAND(2): (-1,-1),(-1,1),(1,-1),(1,1)
  // NOR(3):  (-1,-1),(-1,1),(1,-1),(1,1)
  logic signed [4-1:0] tv_x1;
  logic signed [4-1:0] tv_x2;
  always_comb begin
    if (gs_r == 0) begin
      if (test_index == 0) begin
        tv_x1 = pos_one;
        tv_x2 = pos_one;
      end else if (test_index == 1) begin
        tv_x1 = pos_one;
        tv_x2 = neg_one;
      end else if (test_index == 2) begin
        tv_x1 = neg_one;
        tv_x2 = pos_one;
      end else begin
        tv_x1 = neg_one;
        tv_x2 = neg_one;
      end
    end else if (gs_r == 1) begin
      if (test_index == 0) begin
        tv_x1 = pos_one;
        tv_x2 = pos_one;
      end else if (test_index == 1) begin
        tv_x1 = neg_one;
        tv_x2 = pos_one;
      end else if (test_index == 2) begin
        tv_x1 = pos_one;
        tv_x2 = neg_one;
      end else begin
        tv_x1 = neg_one;
        tv_x2 = neg_one;
      end
    end else if (gs_r == 2) begin
      if (test_index == 0) begin
        tv_x1 = neg_one;
        tv_x2 = neg_one;
      end else if (test_index == 1) begin
        tv_x1 = neg_one;
        tv_x2 = pos_one;
      end else if (test_index == 2) begin
        tv_x1 = pos_one;
        tv_x2 = neg_one;
      end else begin
        tv_x1 = pos_one;
        tv_x2 = pos_one;
      end
    end else if (test_index == 0) begin
      tv_x1 = neg_one;
      tv_x2 = neg_one;
    end else if (test_index == 1) begin
      tv_x1 = neg_one;
      tv_x2 = pos_one;
    end else if (test_index == 2) begin
      tv_x1 = pos_one;
      tv_x2 = neg_one;
    end else begin
      tv_x1 = pos_one;
      tv_x2 = pos_one;
    end
  end
  // FSM next state logic
  // Training: 0(init)->1(capture)->2-5(target)->6(pass)->7(delta)->8(update)->9(iter check)
  // Testing: 10(init)->11(load)->12(compute)->13(wait)->11 or 0(done)
  always_comb begin
    if (present_state == 0) begin
      if (start) begin
        ns = 1;
      end else begin
        ns = 0;
      end
    end else if (present_state == 1) begin
      if (a_pos & b_pos) begin
        ns = 2;
      end else if (a_pos & ~b_pos) begin
        ns = 3;
      end else if (~a_pos & b_pos) begin
        ns = 4;
      end else begin
        ns = 5;
      end
    end else if (present_state == 2) begin
      ns = 6;
    end else if (present_state == 3) begin
      ns = 6;
    end else if (present_state == 4) begin
      ns = 6;
    end else if (present_state == 5) begin
      ns = 6;
    end else if (present_state == 6) begin
      ns = 7;
    end else if (present_state == 7) begin
      ns = 8;
    end else if (present_state == 8) begin
      ns = 9;
    end else if (present_state == 9) begin
      if (iter >= 3) begin
        ns = 10;
      end else begin
        ns = 1;
      end
    end else if (present_state == 10) begin
      ns = 11;
    end else if (present_state == 11) begin
      ns = 12;
    end else if (present_state == 12) begin
      ns = 13;
    end else if (present_state == 13) begin
      if (wait_cnt >= 5) begin
        if (test_index >= 3) begin
          ns = 0;
        end else begin
          ns = 11;
        end
      end else begin
        ns = 13;
      end
    end else begin
      ns = 0;
    end
  end
  always_ff @(posedge clk or negedge rst) begin
    if ((!rst)) begin
      next_state <= 0;
    end else begin
      next_state <= ns;
    end
  end
  always_ff @(posedge clk or negedge rst) begin
    if ((!rst)) begin
      present_state <= 0;
    end else begin
      present_state <= ns;
    end
  end
  // State actions
  always_ff @(posedge clk or negedge rst) begin
    if ((!rst)) begin
      bias <= 0;
      delta_b <= 0;
      delta_w1 <= 0;
      delta_w2 <= 0;
      expected_output <= 0;
      gs_r <= 0;
      iter <= 0;
      target_r <= 0;
      test_done <= 0;
      test_index <= 0;
      test_output <= 0;
      test_present_state <= 0;
      test_result <= 0;
      test_x1 <= 0;
      test_x2 <= 0;
      w1 <= 0;
      w2 <= 0;
      wait_cnt <= 0;
      x1 <= 0;
      x2 <= 0;
    end else begin
      if (present_state == 0) begin
        w1 <= zero4;
        w2 <= zero4;
        bias <= zero4;
        iter <= 0;
        test_done <= 0;
        test_index <= 0;
        test_present_state <= 0;
        wait_cnt <= 0;
        gs_r <= gate_select;
      end else if (present_state == 1) begin
        x1 <= a;
        x2 <= b;
      end else if (present_state == 2) begin
        target_r <= tgt_val;
      end else if (present_state == 3) begin
        target_r <= tgt_val;
      end else if (present_state == 4) begin
        target_r <= tgt_val;
      end else if (present_state == 5) begin
        target_r <= tgt_val;
      end else if (present_state == 7) begin
        delta_w1 <= x1 * target_r;
        delta_w2 <= x2 * target_r;
        delta_b <= target_r;
      end else if (present_state == 8) begin
        w1 <= $signed(4'({{(8-$bits(w1)){w1[$bits(w1)-1]}}, w1} + delta_w1));
        w2 <= $signed(4'({{(8-$bits(w2)){w2[$bits(w2)-1]}}, w2} + delta_w2));
        bias <= $signed(4'(bias + delta_b));
      end else if (present_state == 9) begin
        iter <= 4'(iter + 1);
      end else if (present_state == 10) begin
        test_index <= 0;
        test_present_state <= 1;
        wait_cnt <= 0;
      end else if (present_state == 11) begin
        test_present_state <= 2;
        wait_cnt <= 0;
        test_x1 <= tv_x1;
        test_x2 <= tv_x2;
      end else if (present_state == 12) begin
        test_present_state <= 3;
        if (net_sum[7]) begin
          test_output <= neg_one;
        end else begin
          test_output <= pos_one;
        end
        expected_output <= test_tgt_val;
        test_result <= zero4;
      end else if (present_state == 13) begin
        test_present_state <= 4;
        if (wait_cnt >= 5) begin
          test_index <= 4'(test_index + 1);
          if (test_index >= 3) begin
            test_done <= 1;
          end
        end else begin
          wait_cnt <= 4'(wait_cnt + 1);
        end
      end
    end
  end

endmodule

