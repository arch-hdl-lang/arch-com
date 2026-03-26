Wrote tests/verilog_eval/Prob064_vector3.sv
  input logic [5-1:0] b,
  input logic [5-1:0] c,
  input logic [5-1:0] d,
  input logic [5-1:0] e,
  input logic [5-1:0] f,
  output logic [8-1:0] w,
  output logic [8-1:0] x,
  output logic [8-1:0] y,
  output logic [8-1:0] z
);

  logic [32-1:0] cat;
  assign cat = {a, b, c, d, e, f, 2'd3};
  assign w = cat[31:24];
  assign x = cat[23:16];
  assign y = cat[15:8];
  assign z = cat[7:0];

endmodule

