// VerilogEval Prob061: Shift register stage with load
module TopModule (
  input logic clk,
  input logic w,
  input logic R,
  input logic E,
  input logic L,
  output logic Q
);

  always_ff @(posedge clk) begin
    if (L) begin
      Q <= R;
    end else if (E) begin
      Q <= w;
    end
  end

endmodule

