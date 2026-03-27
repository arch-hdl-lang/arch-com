module reverse_bits (
  input logic [32-1:0] num_in,
  output logic [32-1:0] num_out
);

  logic [32-1:0] reversed;
  assign reversed = {<<1{num_in}};
  assign num_out = reversed;

endmodule

