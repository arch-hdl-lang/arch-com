// Test: bit concatenation {a, b} and bit replication {N{expr}}
// domain SysDomain
//   freq_mhz: 100

module ConcatRepeatTest (
  input logic clk,
  input logic rst,
  input logic [8-1:0] a,
  input logic [8-1:0] b,
  input logic sign_bit,
  output logic [16-1:0] cat_out,
  output logic [4-1:0] rep_out,
  output logic [9-1:0] mixed_out,
  output logic [16-1:0] sext_out
);

  // Concatenation: {a, b} → 16-bit result
  // Replication: {4{sign_bit}} → 4-bit sign extension mask
  // Mixed: {sign_bit, a} → 9-bit result
  // Replication in concat: {{8{sign_bit}}, a} → 16-bit sign-extended
  assign cat_out = {a, b};
  assign rep_out = {4{sign_bit}};
  assign mixed_out = {sign_bit, a};
  assign sext_out = {{8{sign_bit}}, a};

endmodule

