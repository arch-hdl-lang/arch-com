// VerilogEval Prob148: 4-state priority arbiter, active-low sync reset (resetn)
// domain SysDomain

module TopModule (
  input logic clk,
  input logic resetn,
  input logic [3-1:0] r,
  output logic [3-1:0] g
);

  // States: 0=A(idle), 1=B(grant0), 2=C(grant1), 3=D(grant2)
  logic [2-1:0] state_r;
  logic [2-1:0] next_state;
  always_comb begin
    next_state = state_r;
    if ((state_r == 0)) begin
      if (r[0]) begin
        next_state = 1;
      end else if (r[1]) begin
        next_state = 2;
      end else if (r[2]) begin
        next_state = 3;
      end
    end else if ((state_r == 1)) begin
      if ((~r[0])) begin
        next_state = 0;
      end
    end else if ((state_r == 2)) begin
      if ((~r[1])) begin
        next_state = 0;
      end
    end else if ((state_r == 3)) begin
      if ((~r[2])) begin
        next_state = 0;
      end
    end else begin
      next_state = 0;
    end
  end
  // Idle: priority r[0] > r[1] > r[2]
  // Grant 0: stay while r[0]=1
  // Grant 1: stay while r[1]=1
  // Grant 2: stay while r[2]=1
  always_ff @(posedge clk) begin
    if ((!resetn)) begin
      state_r <= 0;
    end else begin
      state_r <= next_state;
    end
  end
  always_comb begin
    if ((state_r == 1)) begin
      g = 1;
    end else if ((state_r == 2)) begin
      g = 2;
    end else if ((state_r == 3)) begin
      g = 4;
    end else begin
      g = 0;
    end
  end

endmodule

