// Data reduction: reduces DATA_COUNT elements of DATA_WIDTH bits each
// into a single DATA_WIDTH-bit output using Bitwise_Reduction
module Data_Reduction #(
  parameter int REDUCTION_OP = 0,
  parameter int DATA_WIDTH = 4,
  parameter int DATA_COUNT = 4
) (
  input logic [DATA_WIDTH * DATA_COUNT-1:0] data_in,
  output logic [DATA_WIDTH-1:0] reduced_data_out
);

  // Perform reduction directly: for each output bit position,
  // gather that bit from all elements and reduce
  logic [DATA_WIDTH-1:0] and_result;
  logic [DATA_WIDTH-1:0] or_result;
  logic [DATA_WIDTH-1:0] xor_result;
  always_comb begin
    // Initialize from first element
    for (int b = 0; b <= DATA_WIDTH - 1; b++) begin
      and_result[b +: 1] = data_in[b +: 1];
      or_result[b +: 1] = data_in[b +: 1];
      xor_result[b +: 1] = data_in[b +: 1];
    end
    // Reduce across remaining elements
    for (int e = 1; e <= DATA_COUNT - 1; e++) begin
      for (int b = 0; b <= DATA_WIDTH - 1; b++) begin
        and_result[b +: 1] = and_result[b +: 1] & data_in[e * DATA_WIDTH + b +: 1];
        or_result[b +: 1] = or_result[b +: 1] | data_in[e * DATA_WIDTH + b +: 1];
        xor_result[b +: 1] = xor_result[b +: 1] ^ data_in[e * DATA_WIDTH + b +: 1];
      end
    end
  end
  // Select operation: 0=AND, 1=OR, 2=XOR, 3=NAND, 4=NOR, 5=XNOR, default=AND
  always_comb begin
    if (REDUCTION_OP == 0) begin
      reduced_data_out = and_result;
    end else if (REDUCTION_OP == 1) begin
      reduced_data_out = or_result;
    end else if (REDUCTION_OP == 2) begin
      reduced_data_out = xor_result;
    end else if (REDUCTION_OP == 3) begin
      reduced_data_out = ~and_result;
    end else if (REDUCTION_OP == 4) begin
      reduced_data_out = ~or_result;
    end else if (REDUCTION_OP == 5) begin
      reduced_data_out = ~xor_result;
    end else begin
      reduced_data_out = and_result;
    end
  end

endmodule

