module TopModule (
  input logic [8-1:0] a,
  input logic [8-1:0] b,
  input logic [8-1:0] c,
  input logic [8-1:0] d,
  output logic [8-1:0] min
);

  logic [8-1:0] ab_min;
  logic [8-1:0] cd_min;
  assign ab_min = a < b ? a : b;
  assign cd_min = c < d ? c : d;
  assign min = ab_min < cd_min ? ab_min : cd_min;

endmodule

