// VerilogEval Prob100: FSM combinational logic only
// State encoding: A=0, B=1, C=2, D=3
module TopModule (
  input logic in,
  input logic [2-1:0] state,
  output logic [2-1:0] next_state,
  output logic out
);

  always_comb begin
    if (state == 0) begin
      if (in) begin
        next_state = 1;
      end else begin
        next_state = 0;
      end
    end else if (state == 1) begin
      if (in) begin
        next_state = 1;
      end else begin
        next_state = 2;
      end
    end else if (state == 2) begin
      if (in) begin
        next_state = 3;
      end else begin
        next_state = 0;
      end
    end else if (in) begin
      next_state = 1;
    end else begin
      next_state = 2;
    end
    out = state == 3;
  end

endmodule

// State A
// State B
// State C
// State D
