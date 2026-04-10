module intra_block #(
  parameter int ROW_COL_WIDTH = 16,
  parameter int DATA_WIDTH = ROW_COL_WIDTH * ROW_COL_WIDTH
) (
  input logic [DATA_WIDTH-1:0] in_data,
  output logic [DATA_WIDTH-1:0] out_data
);

  // Pure combinational bit permutation.
  // For each output bit j, compute the source input bit index:
  //   row = j / ROW_COL_WIDTH
  //   if j < DATA_WIDTH/2:
  //     r_prime = (j - 2*row) % ROW_COL_WIDTH
  //     c_prime = (j - row)   % ROW_COL_WIDTH
  //   else:
  //     r_prime = (j - 2*row - 1) % ROW_COL_WIDTH
  //     c_prime = (j - row - 1)   % ROW_COL_WIDTH
  //   src = r_prime * ROW_COL_WIDTH + c_prime
  always_comb begin
    for (int j = 0; j <= DATA_WIDTH - 1; j++) begin
      if (j < DATA_WIDTH / 2) begin
        out_data[j] = in_data[(j - 2 * (j / ROW_COL_WIDTH)) % ROW_COL_WIDTH * ROW_COL_WIDTH + (j - j / ROW_COL_WIDTH) % ROW_COL_WIDTH];
      end else begin
        out_data[j] = in_data[(j - 2 * (j / ROW_COL_WIDTH) - 1) % ROW_COL_WIDTH * ROW_COL_WIDTH + (j - j / ROW_COL_WIDTH - 1) % ROW_COL_WIDTH];
      end
    end
  end

endmodule

