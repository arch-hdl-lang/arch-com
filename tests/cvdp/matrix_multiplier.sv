module matrix_multiplier #(
  parameter int ROW_A = 4,
  parameter int COL_A = 4,
  parameter int ROW_B = 4,
  parameter int COL_B = 4,
  parameter int INPUT_DATA_WIDTH = 8,
  parameter int OUTPUT_DATA_WIDTH = 48
) (
  input logic [ROW_A * COL_A * INPUT_DATA_WIDTH-1:0] matrix_a,
  input logic [ROW_B * COL_B * INPUT_DATA_WIDTH-1:0] matrix_b,
  output logic [ROW_A * COL_B * OUTPUT_DATA_WIDTH-1:0] matrix_c
);

  always_comb begin
    matrix_c = 0;
    for (int i = 0; i <= ROW_A - 1; i++) begin
      for (int j = 0; j <= COL_B - 1; j++) begin
        for (int k = 0; k <= COL_A - 1; k++) begin
          matrix_c[(i * COL_B + j) * OUTPUT_DATA_WIDTH +: OUTPUT_DATA_WIDTH] = OUTPUT_DATA_WIDTH'(matrix_c[(i * COL_B + j) * OUTPUT_DATA_WIDTH +: OUTPUT_DATA_WIDTH] + OUTPUT_DATA_WIDTH'($unsigned(matrix_a[(i * COL_A + k) * INPUT_DATA_WIDTH +: INPUT_DATA_WIDTH])) * OUTPUT_DATA_WIDTH'($unsigned(matrix_b[(k * COL_B + j) * INPUT_DATA_WIDTH +: INPUT_DATA_WIDTH])));
        end
      end
    end
  end

endmodule

