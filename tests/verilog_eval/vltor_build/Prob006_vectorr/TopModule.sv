module TopModule (
  input logic [8-1:0] in,
  output logic [8-1:0] out
);

  assign out = {<<1{in}};

endmodule

