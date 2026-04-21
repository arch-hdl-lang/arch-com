module hamming_tx #(
  parameter int DATA_WIDTH = 8,
  parameter int PART_WIDTH = 4,
  parameter int PARITY_BIT = 4,
  parameter int NUM_MODULES = DATA_WIDTH / PART_WIDTH,
  parameter int ENCODED_DATA = PART_WIDTH + PARITY_BIT + 1,
  parameter int TOTAL_ENCODED = NUM_MODULES * ENCODED_DATA
) (
  input logic [DATA_WIDTH-1:0] data_in,
  output logic [TOTAL_ENCODED-1:0] data_out
);

  logic [NUM_MODULES-1:0] [ENCODED_DATA-1:0] encoded_parts;
  genvar i;
  for (i = 0; i <= NUM_MODULES - 1; i = i + 1) begin : gen_i
    t_hamming_tx #(.DATA_WIDTH(PART_WIDTH), .PARITY_BIT(PARITY_BIT)) enc (
      .data_in(data_in[(i + 1) * PART_WIDTH - 1:i * PART_WIDTH]),
      .data_out(encoded_parts[i])
    );
  end
  always_comb begin
    data_out = 0;
    for (int i = 0; i <= NUM_MODULES - 1; i++) begin
      data_out = data_out | TOTAL_ENCODED'($unsigned(encoded_parts[i])) << i * ENCODED_DATA;
    end
  end

endmodule

