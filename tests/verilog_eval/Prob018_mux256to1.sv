module TopModule (
  input logic [256-1:0] in,
  input logic [8-1:0] sel,
  output logic out
);

  assign out = in[sel];

endmodule

