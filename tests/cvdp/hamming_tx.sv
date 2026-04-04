module hamming_tx #(
  parameter int DATA_WIDTH = 64,
  parameter int PART_WIDTH = 4,
  parameter int PARITY_BIT = 3,
  parameter int ENCODED_DATA = PARITY_BIT + PART_WIDTH + 1,
  parameter int NUM_MODULES = DATA_WIDTH / PART_WIDTH,
  parameter int TOTAL_ENCODED = ENCODED_DATA * NUM_MODULES
) (
  input logic [DATA_WIDTH-1:0] data_in,
  output logic [TOTAL_ENCODED-1:0] data_out
);

  genvar i;
  for (i = 0; i <= NUM_MODULES - 1; i = i + 1) begin : gen_i
    t_hamming_tx #(.DATA_WIDTH(PART_WIDTH), .PARITY_BIT(PARITY_BIT)) tx_i (
      .data_in(data_in[i * PART_WIDTH +: PART_WIDTH]),
      .data_out(data_out[i * ENCODED_DATA +: ENCODED_DATA])
    );
  end

endmodule

