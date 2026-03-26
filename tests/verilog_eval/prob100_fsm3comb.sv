Wrote tests/verilog_eval/prob100_fsm3comb.sv
ic only
// State encoding: A=0, B=1, C=2, D=3
module TopModule (
  input logic in,
  input logic [2-1:0] state,
  output logic [2-1:0] next_state,
  output logic out
);

  always_comb begin
    if (state == 0) begin
      next_state = in ? 1 : 0;
    end else if (state == 1) begin
      next_state = in ? 1 : 2;
    end else if (state == 2) begin
      next_state = in ? 3 : 0;
    end else begin
      next_state = in ? 1 : 2;
    end
    out = state == 3;
  end

endmodule

// State A
// State B
// State C
// State D
