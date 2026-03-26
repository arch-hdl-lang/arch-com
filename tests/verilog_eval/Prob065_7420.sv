Wrote tests/verilog_eval/Prob065_7420.sv
nput logic p1b,
  input logic p1c,
  input logic p1d,
  input logic p2a,
  input logic p2b,
  input logic p2c,
  input logic p2d,
  output logic p1y,
  output logic p2y
);

  assign p1y = ~(p1a & p1b & p1c & p1d);
  assign p2y = ~(p2a & p2b & p2c & p2d);

endmodule

