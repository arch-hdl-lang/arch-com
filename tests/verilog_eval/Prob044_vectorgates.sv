Wrote tests/verilog_eval/Prob044_vectorgates.sv
NOT of 3-bit vectors
module TopModule (
  input logic [3-1:0] a,
  input logic [3-1:0] b,
  output logic [3-1:0] out_or_bitwise,
  output logic out_or_logical,
  output logic [6-1:0] out_not
);

  assign out_or_bitwise = a | b;
  assign out_or_logical = a[0] | a[1] | a[2] | b[0] | b[1] | b[2];
  assign out_not = {~b, ~a};

endmodule

