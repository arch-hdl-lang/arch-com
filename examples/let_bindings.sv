// Test: let bindings — typed and untyped
// domain SysDomain
//   freq_mhz: 100

module LetDemo (
  input logic [7:0] a,
  input logic [7:0] b,
  output logic [7:0] masked,
  output logic equal
);

  // Typed let binding: explicit type annotation
  logic [7:0] mask;
  assign mask = a & b;
  // Untyped let binding: type inferred from expression
  logic same;
  assign same = a == b;
  assign masked = mask;
  assign equal = same;

endmodule

