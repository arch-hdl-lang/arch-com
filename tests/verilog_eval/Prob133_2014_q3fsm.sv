// VerilogEval Prob133: FSM waits for s=1, then counts w over 3-cycle windows
// domain SysDomain

module TopModule (
  input logic clk,
  input logic rst,
  input logic s_sig,
  input logic w,
  output logic z
);

  // States: 0=A (wait for s), 1=B1 (cycle 1 of 3), 2=B2 (cycle 2), 3=B3 (cycle 3), 4=output
  logic [3-1:0] state_r;
  logic [2-1:0] count_r;
  logic [3-1:0] next_state;
  logic [2-1:0] next_count;
  always_comb begin
    next_count = count_r;
    next_state = state_r;
    if ((state_r == 0)) begin
      next_count = 0;
      if (s_sig) begin
        next_state = 1;
      end
    end else if ((state_r == 1)) begin
      if (w) begin
        next_count = 1;
      end else begin
        next_count = 0;
      end
      next_state = 2;
    end else if ((state_r == 2)) begin
      if (w) begin
        next_count = 2'((count_r + 1));
      end
      next_state = 3;
    end else if ((state_r == 3)) begin
      if (w) begin
        next_count = 2'((count_r + 1));
      end
      next_state = 4;
    end else if ((state_r == 4)) begin
      if (w) begin
        next_count = 1;
      end else begin
        next_count = 0;
      end
      next_state = 2;
    end else begin
      next_state = 0;
      next_count = 0;
    end
  end
  // State A: wait for s
  // B1: first cycle
  // B2: second cycle
  // B3: third cycle
  // Output cycle, then start next window
  always_ff @(posedge clk) begin
    if (rst) begin
      count_r <= 0;
      state_r <= 0;
    end else begin
      state_r <= next_state;
      count_r <= next_count;
    end
  end
  always_comb begin
    if (((state_r == 4) & (count_r == 2))) begin
      z = 1'b1;
    end else begin
      z = 1'b0;
    end
  end

endmodule

