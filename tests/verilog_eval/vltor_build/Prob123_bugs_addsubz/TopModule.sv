module TopModule (
  input logic do_sub,
  input logic [8-1:0] a,
  input logic [8-1:0] b,
  output logic [8-1:0] out,
  output logic result_is_zero
);

  assign out = do_sub ? 8'(a - b) : 8'(a + b);
  assign result_is_zero = out == 0;

endmodule

