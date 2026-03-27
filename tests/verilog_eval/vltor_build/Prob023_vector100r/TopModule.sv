module TopModule (
  input logic [100-1:0] in,
  output logic [100-1:0] out
);

  assign out = {<<1{in}};

endmodule

