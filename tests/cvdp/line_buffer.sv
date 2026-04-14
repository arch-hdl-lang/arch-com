module line_buffer #(
  parameter int NBW_DATA = 8,
  parameter int NS_ROW = 10,
  parameter int NS_COLUMN = 8,
  parameter int NS_R_OUT = 4,
  parameter int NS_C_OUT = 3,
  parameter int CONSTANT = 255,
  parameter int NBW_ROW = 4,
  parameter int NBW_COL = 3,
  parameter int NBW_MODE = 3,
  localparam int WIN_PIXELS = NS_R_OUT * NS_C_OUT,
  localparam int OUT_BITS = WIN_PIXELS * NBW_DATA
) (
  input logic clk,
  input logic rst_async_n,
  input logic [NBW_MODE-1:0] i_mode,
  input logic i_valid,
  input logic i_update_window,
  input logic [NS_COLUMN * NBW_DATA-1:0] i_row_image,
  input logic [NBW_ROW-1:0] i_image_row_start,
  input logic [NBW_COL-1:0] i_image_col_start,
  output logic [OUT_BITS-1:0] o_image_window
);

  // Line buffer: pix_rows[0] = newest row, pix_rows[NS_ROW-1] = oldest
  logic [NS_ROW-1:0] [NS_COLUMN * NBW_DATA-1:0] pix_rows;
  // Last registered window pixels (updated when i_update_window=1)
  logic [WIN_PIXELS-1:0] [NBW_DATA-1:0] last_win;
  // Compute current window combinatorially from current inputs + pix_rows
  logic [OUT_BITS-1:0] comb_win;
  always_comb begin
    for (int wr = 0; wr <= NS_R_OUT - 1; wr++) begin
      for (int wc = 0; wc <= NS_C_OUT - 1; wc++) begin
        if (i_mode == 0) begin
          if (8'($unsigned(i_image_row_start)) + 8'($unsigned(wr)) < 8'($unsigned(NS_ROW)) & 8'($unsigned(i_image_col_start)) + 8'($unsigned(wc)) < 8'($unsigned(NS_COLUMN))) begin
            comb_win[(wr * NS_C_OUT + wc) * NBW_DATA +: NBW_DATA] = NBW_DATA'(pix_rows[4'(8'($unsigned(i_image_row_start)) + 8'($unsigned(wr)))] >> (8'($unsigned(NS_COLUMN)) - 1 - (8'($unsigned(i_image_col_start)) + 8'($unsigned(wc)))) * 8'($unsigned(NBW_DATA)));
          end else begin
            comb_win[(wr * NS_C_OUT + wc) * NBW_DATA +: NBW_DATA] = NBW_DATA'($unsigned(0));
          end
        end else if (i_mode == 1) begin
          if (8'($unsigned(i_image_row_start)) + 8'($unsigned(wr)) < 8'($unsigned(NS_ROW)) & 8'($unsigned(i_image_col_start)) + 8'($unsigned(wc)) < 8'($unsigned(NS_COLUMN))) begin
            comb_win[(wr * NS_C_OUT + wc) * NBW_DATA +: NBW_DATA] = NBW_DATA'(pix_rows[4'(8'($unsigned(i_image_row_start)) + 8'($unsigned(wr)))] >> (8'($unsigned(NS_COLUMN)) - 1 - (8'($unsigned(i_image_col_start)) + 8'($unsigned(wc)))) * 8'($unsigned(NBW_DATA)));
          end else begin
            comb_win[(wr * NS_C_OUT + wc) * NBW_DATA +: NBW_DATA] = NBW_DATA'((NBW_DATA + 1)'($unsigned(CONSTANT)));
          end
        end else if (i_mode == 2) begin
          comb_win[(wr * NS_C_OUT + wc) * NBW_DATA +: NBW_DATA] = NBW_DATA'(pix_rows[4'(8'($unsigned(i_image_row_start)) + 8'($unsigned(wr)) < 8'($unsigned(NS_ROW)) ? 8'($unsigned(i_image_row_start)) + 8'($unsigned(wr)) : 8'($unsigned(NS_ROW - 1)))] >> (8'($unsigned(NS_COLUMN)) - 1 - (8'($unsigned(i_image_col_start)) + 8'($unsigned(wc)) < 8'($unsigned(NS_COLUMN)) ? 8'($unsigned(i_image_col_start)) + 8'($unsigned(wc)) : 8'($unsigned(NS_COLUMN - 1)))) * 8'($unsigned(NBW_DATA)));
        end else if (i_mode == 3) begin
          comb_win[(wr * NS_C_OUT + wc) * NBW_DATA +: NBW_DATA] = NBW_DATA'(pix_rows[4'(8'($unsigned(i_image_row_start)) + 8'($unsigned(wr)) < 8'($unsigned(NS_ROW)) ? 8'($unsigned(i_image_row_start)) + 8'($unsigned(wr)) : 8'($unsigned(2 * NS_ROW - 1)) - (8'($unsigned(i_image_row_start)) + 8'($unsigned(wr))))] >> (8'($unsigned(NS_COLUMN)) - 1 - (8'($unsigned(i_image_col_start)) + 8'($unsigned(wc)) < 8'($unsigned(NS_COLUMN)) ? 8'($unsigned(i_image_col_start)) + 8'($unsigned(wc)) : 8'($unsigned(2 * NS_COLUMN - 1)) - (8'($unsigned(i_image_col_start)) + 8'($unsigned(wc))))) * 8'($unsigned(NBW_DATA)));
        end else if (i_mode == 4) begin
          comb_win[(wr * NS_C_OUT + wc) * NBW_DATA +: NBW_DATA] = NBW_DATA'(pix_rows[4'((8'($unsigned(i_image_row_start)) + 8'($unsigned(wr))) % 8'($unsigned(NS_ROW)))] >> (8'($unsigned(NS_COLUMN)) - 1 - (8'($unsigned(i_image_col_start)) + 8'($unsigned(wc))) % 8'($unsigned(NS_COLUMN))) * 8'($unsigned(NBW_DATA)));
        end else begin
          comb_win[(wr * NS_C_OUT + wc) * NBW_DATA +: NBW_DATA] = NBW_DATA'($unsigned(0));
        end
      end
    end
  end
  // Output: if update_window, show comb result; else show last registered window
  always_comb begin
    for (int wr = 0; wr <= NS_R_OUT - 1; wr++) begin
      for (int wc = 0; wc <= NS_C_OUT - 1; wc++) begin
        o_image_window[(wr * NS_C_OUT + wc) * NBW_DATA +: NBW_DATA] = i_update_window ? comb_win[(wr * NS_C_OUT + wc) * NBW_DATA +: NBW_DATA] : last_win[wr * NS_C_OUT + wc];
      end
    end
  end
  always_ff @(posedge clk or negedge rst_async_n) begin
    if ((!rst_async_n)) begin
      for (int __ri0 = 0; __ri0 < WIN_PIXELS; __ri0++) begin
        last_win[__ri0] <= 0;
      end
      for (int __ri0 = 0; __ri0 < NS_ROW; __ri0++) begin
        pix_rows[__ri0] <= 0;
      end
    end else begin
      // Shift buffer: insert new row at index 0 on valid
      if (i_valid) begin
        for (int sr = 1; sr <= NS_ROW - 1; sr++) begin
          pix_rows[sr] <= pix_rows[sr - 1];
        end
        pix_rows[0] <= i_row_image;
      end
      // Register window when update_window=1
      if (i_update_window) begin
        for (int wr = 0; wr <= NS_R_OUT - 1; wr++) begin
          for (int wc = 0; wc <= NS_C_OUT - 1; wc++) begin
            last_win[wr * NS_C_OUT + wc] <= comb_win[(wr * NS_C_OUT + wc) * NBW_DATA +: NBW_DATA];
          end
        end
      end
    end
  end

endmodule

