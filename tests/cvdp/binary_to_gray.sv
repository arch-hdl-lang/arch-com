module binary_to_gray #(
  parameter int WIDTH = 6
) (
  input logic [WIDTH-1:0] binary_in,
  output logic [WIDTH-1:0] gray_out
);

  logic [WIDTH-1:0] shifted;
  assign shifted = binary_in >> 1;
  assign gray_out = binary_in ^ shifted;

endmodule

