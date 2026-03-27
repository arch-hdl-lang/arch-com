// domain SysDomain

module TopModule (
  input logic clk,
  input logic a,
  input logic b,
  output logic out_assign,
  output logic out_always_comb,
  output logic out_always_ff
);

  logic ff_r;
  always_ff @(posedge clk) begin
    ff_r <= a ^ b;
  end
  assign out_assign = a ^ b;
  assign out_always_comb = a ^ b;
  assign out_always_ff = ff_r;

endmodule

