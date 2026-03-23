module TopModule (
  input logic p1a,
  input logic p1b,
  input logic p1c,
  input logic p1d,
  input logic p1e,
  input logic p1f,
  input logic p2a,
  input logic p2b,
  input logic p2c,
  input logic p2d,
  output logic p1y,
  output logic p2y
);

  assign p1y = (((p1a & p1b) & p1c) | ((p1d & p1e) & p1f));
  assign p2y = ((p2a & p2b) | (p2c & p2d));

endmodule

