module binary_to_one_hot_decoder #(
  parameter int BINARY_WIDTH = 5,
  parameter int OUTPUT_WIDTH = 32
) (
  input logic [BINARY_WIDTH-1:0] binary_in,
  output logic [OUTPUT_WIDTH-1:0] one_hot_out
);

  assign one_hot_out = 32'd1 << 32'($unsigned(binary_in));

endmodule

