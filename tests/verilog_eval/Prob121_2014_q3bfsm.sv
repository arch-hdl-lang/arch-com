// VerilogEval Prob121: 5-state FSM with state-assigned table
// domain SysDomain

module TopModule (
  input logic clk,
  input logic reset,
  input logic x,
  output logic z
);

  logic [3-1:0] state_r;
  logic [3-1:0] next_state;
  always_comb begin
    if ((state_r == 0)) begin
      if (x) begin
        next_state = 1;
      end else begin
        next_state = 0;
      end
    end else if ((state_r == 1)) begin
      if (x) begin
        next_state = 4;
      end else begin
        next_state = 1;
      end
    end else if ((state_r == 2)) begin
      if (x) begin
        next_state = 1;
      end else begin
        next_state = 2;
      end
    end else if ((state_r == 3)) begin
      if (x) begin
        next_state = 2;
      end else begin
        next_state = 1;
      end
    end else if ((state_r == 4)) begin
      if (x) begin
        next_state = 4;
      end else begin
        next_state = 3;
      end
    end else begin
      next_state = 0;
    end
  end
  always_ff @(posedge clk) begin
    if (reset) begin
      state_r <= 0;
    end else begin
      state_r <= next_state;
    end
  end
  always_comb begin
    if (((state_r == 3) | (state_r == 4))) begin
      z = 1'b1;
    end else begin
      z = 1'b0;
    end
  end

endmodule

