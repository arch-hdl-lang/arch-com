module image_rotate #(
  parameter int IN_ROW = 4,
  parameter int IN_COL = 4,
  parameter int OUT_ROW = IN_ROW > IN_COL ? IN_ROW : IN_COL,
  parameter int OUT_COL = IN_ROW > IN_COL ? IN_ROW : IN_COL,
  parameter int DATA_WIDTH = 8
) (
  input logic [1:0] rotation_angle,
  input logic [IN_ROW * IN_COL * DATA_WIDTH-1:0] image_in,
  output logic [OUT_ROW * OUT_COL * DATA_WIDTH-1:0] image_out
);

  logic [OUT_ROW * OUT_COL * DATA_WIDTH-1:0] padded_image;
  logic [OUT_ROW * OUT_COL * DATA_WIDTH-1:0] transposed_image;
  // Padding: copy image_in into top-left of square matrix, zeros elsewhere
  always_comb begin
    for (int pad_row = 0; pad_row <= OUT_ROW - 1; pad_row++) begin
      for (int pad_col = 0; pad_col <= OUT_COL - 1; pad_col++) begin
        if (pad_row < IN_ROW) begin
          if (pad_col < IN_COL) begin
            padded_image[(pad_row * OUT_COL + pad_col) * DATA_WIDTH +: DATA_WIDTH] = image_in[(pad_row * IN_COL + pad_col) * DATA_WIDTH +: DATA_WIDTH];
          end else begin
            padded_image[(pad_row * OUT_COL + pad_col) * DATA_WIDTH +: DATA_WIDTH] = 0;
          end
        end else begin
          padded_image[(pad_row * OUT_COL + pad_col) * DATA_WIDTH +: DATA_WIDTH] = 0;
        end
      end
    end
  end
  // Transpose
  always_comb begin
    for (int trans_row = 0; trans_row <= OUT_ROW - 1; trans_row++) begin
      for (int trans_col = 0; trans_col <= OUT_COL - 1; trans_col++) begin
        transposed_image[(trans_row * OUT_COL + trans_col) * DATA_WIDTH +: DATA_WIDTH] = padded_image[(trans_col * OUT_ROW + trans_row) * DATA_WIDTH +: DATA_WIDTH];
      end
    end
  end
  // Rotation output: 90 CW (transpose then reverse each row)
  logic [OUT_ROW * OUT_COL * DATA_WIDTH-1:0] rot90;
  always_comb begin
    for (int r = 0; r <= OUT_ROW - 1; r++) begin
      for (int c = 0; c <= OUT_COL - 1; c++) begin
        rot90[(r * OUT_COL + c) * DATA_WIDTH +: DATA_WIDTH] = transposed_image[(r * OUT_COL + ((OUT_COL - 1) - c)) * DATA_WIDTH +: DATA_WIDTH];
      end
    end
  end
  // Rotation output: 180 (reverse rows and columns)
  logic [OUT_ROW * OUT_COL * DATA_WIDTH-1:0] rot180;
  always_comb begin
    for (int r = 0; r <= OUT_ROW - 1; r++) begin
      for (int c = 0; c <= OUT_COL - 1; c++) begin
        rot180[(r * OUT_COL + c) * DATA_WIDTH +: DATA_WIDTH] = padded_image[(((OUT_ROW - 1) - r) * OUT_COL + ((OUT_COL - 1) - c)) * DATA_WIDTH +: DATA_WIDTH];
      end
    end
  end
  // Rotation output: 270 CW (transpose then reverse columns)
  logic [OUT_ROW * OUT_COL * DATA_WIDTH-1:0] rot270;
  always_comb begin
    for (int r = 0; r <= OUT_ROW - 1; r++) begin
      for (int c = 0; c <= OUT_COL - 1; c++) begin
        rot270[(r * OUT_COL + c) * DATA_WIDTH +: DATA_WIDTH] = transposed_image[(((OUT_ROW - 1) - r) * OUT_COL + c) * DATA_WIDTH +: DATA_WIDTH];
      end
    end
  end
  // Output mux
  always_comb begin
    if (rotation_angle == 'b0) begin
      image_out = rot90;
    end else if (rotation_angle == 'b1) begin
      image_out = rot180;
    end else if (rotation_angle == 'b10) begin
      image_out = rot270;
    end else begin
      image_out = padded_image;
    end
  end

endmodule

