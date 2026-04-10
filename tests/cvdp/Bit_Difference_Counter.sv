module Bit_Difference_Counter #(
  parameter int BIT_WIDTH = 3,
  localparam int COUNT_WIDTH = $clog2(BIT_WIDTH) + 1
) (
  input logic [BIT_WIDTH-1:0] input_A,
  input logic [BIT_WIDTH-1:0] input_B,
  output logic [COUNT_WIDTH-1:0] bit_difference_count
);

  // XOR to find differing bits
  logic [BIT_WIDTH-1:0] xor_bits;
  assign xor_bits = input_A ^ input_B;
  // Popcount: sum individual bits
  logic [COUNT_WIDTH-1:0] sum;
  always_comb begin
    sum = COUNT_WIDTH'($unsigned(0));
    for (int i = 0; i <= BIT_WIDTH - 1; i++) begin
      sum = COUNT_WIDTH'(sum + COUNT_WIDTH'($unsigned(xor_bits[i +: 1])));
    end
    bit_difference_count = sum;
  end

endmodule

