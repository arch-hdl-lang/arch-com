// VerilogEval Prob139: Motor controller FSM, active-low sync reset (resetn)
// States: A(reset), SetF, X1, X0, X01, SetG1, SetG2, GoodG, BadG
// domain SysDomain

module TopModule (
  input logic clk,
  input logic resetn,
  input logic x,
  input logic y,
  output logic f,
  output logic g
);

  logic [4-1:0] state_r;
  logic [4-1:0] next_state;
  // 0=A, 1=SetF, 2=X1, 3=X0, 4=X01, 5=SetG1, 6=SetG2, 7=GoodG, 8=BadG
  always_comb begin
    next_state = state_r;
    if ((state_r == 0)) begin
      next_state = 1;
    end else if ((state_r == 1)) begin
      next_state = 2;
    end else if ((state_r == 2)) begin
      if (x) begin
        next_state = 3;
      end
    end else if ((state_r == 3)) begin
      if (x) begin
        next_state = 3;
      end else begin
        next_state = 4;
      end
    end else if ((state_r == 4)) begin
      if (x) begin
        next_state = 5;
      end else begin
        next_state = 2;
      end
    end else if ((state_r == 5)) begin
      if (y) begin
        next_state = 7;
      end else begin
        next_state = 6;
      end
    end else if ((state_r == 6)) begin
      if (y) begin
        next_state = 7;
      end else begin
        next_state = 8;
      end
    end else if ((state_r == 7)) begin
      next_state = 7;
    end else if ((state_r == 8)) begin
      next_state = 8;
    end else begin
      next_state = 0;
    end
  end
  // A: after reset deasserted, go to SetF
  // SetF: f=1 for one cycle, then monitor x
  // Wait for x=1
  // Got 1, need 0
  // Got 1,0, need 1
  // SetG1: g=1, first cycle to check y
  // SetG2: g=1, second cycle to check y
  // GoodG: g=1 permanently
  // BadG: g=0 permanently
  always_ff @(posedge clk) begin
    if ((!resetn)) begin
      state_r <= 0;
    end else begin
      state_r <= next_state;
    end
  end
  always_comb begin
    if ((state_r == 1)) begin
      f = 1'b1;
    end else begin
      f = 1'b0;
    end
    if ((((state_r == 5) | (state_r == 6)) | (state_r == 7))) begin
      g = 1'b1;
    end else begin
      g = 1'b0;
    end
  end

endmodule

