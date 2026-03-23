// VerilogEval Prob129: Mealy FSM detecting overlapping 101, async active-low reset
// domain SysDomain

module TopModule (
  input logic clk,
  input logic aresetn,
  input logic x,
  output logic z
);

  logic [2-1:0] state_r = 0;
  logic [2-1:0] next_state;
  always_comb begin
    if ((state_r == 0)) begin
      if (x) begin
        next_state = 1;
      end else begin
        next_state = 0;
      end
    end else if ((state_r == 1)) begin
      if (x) begin
        next_state = 1;
      end else begin
        next_state = 2;
      end
    end else if ((state_r == 2)) begin
      if (x) begin
        next_state = 1;
      end else begin
        next_state = 0;
      end
    end else begin
      next_state = 0;
    end
  end
  always_ff @(posedge clk or negedge aresetn) begin
    if ((!aresetn)) begin
      state_r <= 0;
    end else begin
      state_r <= next_state;
    end
  end
  always_comb begin
    if (((state_r == 2) & x)) begin
      z = 1'b1;
    end else begin
      z = 1'b0;
    end
  end

endmodule

