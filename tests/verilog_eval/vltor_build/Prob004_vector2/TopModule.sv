// VerilogEval Prob004: Reverse byte order of 32-bit vector
module TopModule (
  input logic [32-1:0] in,
  output logic [32-1:0] out
);

  assign out = {<<8{in}};

endmodule

