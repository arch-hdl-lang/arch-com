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
  localparam int NS_BUF = NS_ROW * NS_COLUMN,
  localparam int WIN_PIXELS = NS_R_OUT * NS_C_OUT,
  localparam int OUT_BITS = WIN_PIXELS * NBW_DATA,
  localparam int PAD_VAL = CONSTANT
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

  // Flat pixel buffer: pixel[r][c] at Vec index r*NS_COLUMN+c; row 0 is newest
  logic [NS_BUF-1:0] [NBW_DATA-1:0] pix_buf;
  // Window storage: o_win[r*NS_C_OUT+c] = output pixel at position (r,c)
  logic [WIN_PIXELS-1:0] [NBW_DATA-1:0] o_win;
  // Assemble flat output from o_win (for loop unrolled in comb → constant bit indices in SV)
  logic [OUT_BITS-1:0] win_flat;
  always_comb begin
    win_flat = 0;
    for (int r = 0; r <= NS_R_OUT - 1; r++) begin
      for (int c = 0; c <= NS_C_OUT - 1; c++) begin
        win_flat[(r * NS_C_OUT + c) * NBW_DATA +: NBW_DATA] = o_win[r * NS_C_OUT + c];
      end
    end
    o_image_window = win_flat;
  end
  always_ff @(posedge clk or negedge rst_async_n) begin
    if ((!rst_async_n)) begin
      for (int __ri0 = 0; __ri0 < WIN_PIXELS; __ri0++) begin
        o_win[__ri0] <= 0;
      end
      for (int __ri0 = 0; __ri0 < NS_BUF; __ri0++) begin
        pix_buf[__ri0] <= 0;
      end
    end else begin
      if (i_update_window) begin
        for (int r = 0; r <= NS_R_OUT - 1; r++) begin
          for (int c = 0; c <= NS_C_OUT - 1; c++) begin
            if (i_mode == 0) begin
              // NO_BOUND_PROCESS: out of bounds → 0
              if (32'($unsigned(i_image_row_start)) + 32'($unsigned(r)) < 32'($unsigned(NS_ROW)) & 32'($unsigned(i_image_col_start)) + 32'($unsigned(c)) < 32'($unsigned(NS_COLUMN))) begin
                o_win[r * NS_C_OUT + c] <= pix_buf[($clog2(NS_BUF) + 1)'((32'($unsigned(i_image_row_start)) + 32'($unsigned(r))) * 32'($unsigned(NS_COLUMN)) + 32'($unsigned(i_image_col_start)) + 32'($unsigned(c)))];
              end else begin
                o_win[r * NS_C_OUT + c] <= 0;
              end
            end else if (i_mode == 1) begin
              // PAD_CONSTANT: out of bounds → CONSTANT
              if (32'($unsigned(i_image_row_start)) + 32'($unsigned(r)) < 32'($unsigned(NS_ROW)) & 32'($unsigned(i_image_col_start)) + 32'($unsigned(c)) < 32'($unsigned(NS_COLUMN))) begin
                o_win[r * NS_C_OUT + c] <= pix_buf[($clog2(NS_BUF) + 1)'((32'($unsigned(i_image_row_start)) + 32'($unsigned(r))) * 32'($unsigned(NS_COLUMN)) + 32'($unsigned(i_image_col_start)) + 32'($unsigned(c)))];
              end else begin
                o_win[r * NS_C_OUT + c] <= NBW_DATA'((NBW_DATA + 1)'($unsigned(PAD_VAL)));
              end
            end else if (i_mode == 2) begin
              // EXTEND_NEAR: clamp row and col to [0, NS_ROW-1] / [0, NS_COLUMN-1]
              o_win[r * NS_C_OUT + c] <= pix_buf[($clog2(NS_BUF) + 1)'((32'($unsigned(i_image_row_start)) + 32'($unsigned(r)) < 32'($unsigned(NS_ROW)) ? 32'($unsigned(i_image_row_start)) + 32'($unsigned(r)) : 32'($unsigned(NS_ROW - 1))) * 32'($unsigned(NS_COLUMN)) + (32'($unsigned(i_image_col_start)) + 32'($unsigned(c)) < 32'($unsigned(NS_COLUMN)) ? 32'($unsigned(i_image_col_start)) + 32'($unsigned(c)) : 32'($unsigned(NS_COLUMN - 1))))];
            end else if (i_mode == 3) begin
              // MIRROR_BOUND: reflect at boundaries
              o_win[r * NS_C_OUT + c] <= pix_buf[($clog2(NS_BUF) + 1)'((32'($unsigned(i_image_row_start)) + 32'($unsigned(r)) < 32'($unsigned(NS_ROW)) ? 32'($unsigned(i_image_row_start)) + 32'($unsigned(r)) : 32'($unsigned(2 * NS_ROW - 1)) - (32'($unsigned(i_image_row_start)) + 32'($unsigned(r)))) * 32'($unsigned(NS_COLUMN)) + (32'($unsigned(i_image_col_start)) + 32'($unsigned(c)) < 32'($unsigned(NS_COLUMN)) ? 32'($unsigned(i_image_col_start)) + 32'($unsigned(c)) : 32'($unsigned(2 * NS_COLUMN - 1)) - (32'($unsigned(i_image_col_start)) + 32'($unsigned(c)))))];
            end else if (i_mode == 4) begin
              // WRAP_AROUND: modulo
              o_win[r * NS_C_OUT + c] <= pix_buf[($clog2(NS_BUF) + 1)'((32'($unsigned(i_image_row_start)) + 32'($unsigned(r))) % 32'($unsigned(NS_ROW)) * 32'($unsigned(NS_COLUMN)) + (32'($unsigned(i_image_col_start)) + 32'($unsigned(c))) % 32'($unsigned(NS_COLUMN)))];
            end else begin
              o_win[r * NS_C_OUT + c] <= 0;
            end
          end
        end
      end
      if (i_valid) begin
        // Shift buffer: row k <- row k-1 for k = NS_ROW-1 downto 1
        for (int r = 1; r <= NS_ROW - 1; r++) begin
          for (int c = 0; c <= NS_COLUMN - 1; c++) begin
            pix_buf[r * NS_COLUMN + c] <= pix_buf[(r - 1) * NS_COLUMN + c];
          end
        end
        // Insert new row at index 0: pixel[c] = i_row_image[(NS_COLUMN-1-c)*NBW_DATA +: NBW_DATA]
        for (int c = 0; c <= NS_COLUMN - 1; c++) begin
          pix_buf[c] <= i_row_image[(NS_COLUMN - 1 - c) * NBW_DATA +: NBW_DATA];
        end
      end
    end
  end

endmodule

