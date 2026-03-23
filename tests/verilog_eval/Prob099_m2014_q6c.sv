module TopModule (
  input logic [6-1:0] y,
  input logic w,
  output logic y1,
  output logic y3
);

  assign y1 = (y[0] & (~w));
  assign y3 = ((((y[1] | y[2]) | y[4]) | y[5]) & w);

endmodule

