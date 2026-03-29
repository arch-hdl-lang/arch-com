// Bitwise reduction: reduces BIT_COUNT input bits to one output bit
// using a specified Boolean operation (AND/OR/XOR/NAND/NOR/XNOR)
module Bitwise_Reduction #(
  parameter int REDUCTION_OP = 0,
  parameter int BIT_COUNT = 4
) (
  input logic [BIT_COUNT-1:0] input_bits,
  output logic [1-1:0] reduced_bit
);

  logic [1-1:0] and_r;
  logic [1-1:0] or_r;
  logic [1-1:0] xor_r;
  always_comb begin
    and_r = input_bits[0:0];
    or_r = input_bits[0:0];
    xor_r = input_bits[0:0];
    for (int i = 1; i <= BIT_COUNT - 1; i++) begin
      and_r = and_r & input_bits[i +: 1];
      or_r = or_r | input_bits[i +: 1];
      xor_r = xor_r ^ input_bits[i +: 1];
    end
  end
  // Select operation: 0=AND, 1=OR, 2=XOR, 3=NAND, 4=NOR, 5=XNOR, default=AND
  always_comb begin
    if (REDUCTION_OP == 0) begin
      reduced_bit = and_r;
    end else if (REDUCTION_OP == 1) begin
      reduced_bit = or_r;
    end else if (REDUCTION_OP == 2) begin
      reduced_bit = xor_r;
    end else if (REDUCTION_OP == 3) begin
      reduced_bit = ~and_r;
    end else if (REDUCTION_OP == 4) begin
      reduced_bit = ~or_r;
    end else if (REDUCTION_OP == 5) begin
      reduced_bit = ~xor_r;
    end else begin
      reduced_bit = and_r;
    end
  end

endmodule

