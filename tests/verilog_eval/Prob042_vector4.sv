module TopModule (
  input logic [8-1:0] in,
  output logic [32-1:0] out
);

  assign out = {{24{in[7]}}, in};

endmodule

