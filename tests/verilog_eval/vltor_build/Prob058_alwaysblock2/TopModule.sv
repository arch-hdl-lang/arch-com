module TopModule (
  input logic clk,
  input logic a,
  input logic b,
  output logic out_assign,
  output logic out_always_comb,
  output logic out_always_ff
);

  always_ff @(posedge clk) begin
    out_always_ff <= a ^ b;
  end
  assign out_assign = a ^ b;
  assign out_always_comb = a ^ b;

endmodule

