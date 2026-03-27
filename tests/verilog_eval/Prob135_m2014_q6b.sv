module TopModule (
  input logic [3-1:0] y,
  input logic w,
  output logic Y1
);

  assign Y1 = y == 1 | y == 5 | w & (y == 2 | y == 4) ? 1 : 0;

endmodule

