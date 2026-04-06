module ReductionOps (
  input logic [8-1:0] data,
  output logic all_ones,
  output logic any_set,
  output logic parity
);

  assign all_ones = &data;
  assign any_set = |data;
  assign parity = ^data;

endmodule

