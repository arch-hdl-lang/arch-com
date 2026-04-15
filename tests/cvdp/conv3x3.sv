module conv3x3 (
  input logic clk,
  input logic rst_n,
  input logic [7:0] image_data0,
  input logic [7:0] image_data1,
  input logic [7:0] image_data2,
  input logic [7:0] image_data3,
  input logic [7:0] image_data4,
  input logic [7:0] image_data5,
  input logic [7:0] image_data6,
  input logic [7:0] image_data7,
  input logic [7:0] image_data8,
  input logic [7:0] kernel0,
  input logic [7:0] kernel1,
  input logic [7:0] kernel2,
  input logic [7:0] kernel3,
  input logic [7:0] kernel4,
  input logic [7:0] kernel5,
  input logic [7:0] kernel6,
  input logic [7:0] kernel7,
  input logic [7:0] kernel8,
  output logic [15:0] convolved_data
);

  // Stage 1: element-wise unsigned multiply (8x8 -> 16 bits)
  logic [15:0] mult0;
  logic [15:0] mult1;
  logic [15:0] mult2;
  logic [15:0] mult3;
  logic [15:0] mult4;
  logic [15:0] mult5;
  logic [15:0] mult6;
  logic [15:0] mult7;
  logic [15:0] mult8;
  // Stage 2: row-wise sums
  logic [19:0] row0;
  logic [19:0] row1;
  logic [19:0] row2;
  // Stage 3: total sum
  logic [19:0] total;
  // Stage 4: normalized result (divide by 9)
  logic [15:0] result_reg;
  assign convolved_data = result_reg;
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      mult0 <= 0;
      mult1 <= 0;
      mult2 <= 0;
      mult3 <= 0;
      mult4 <= 0;
      mult5 <= 0;
      mult6 <= 0;
      mult7 <= 0;
      mult8 <= 0;
      result_reg <= 0;
      row0 <= 0;
      row1 <= 0;
      row2 <= 0;
      total <= 0;
    end else begin
      // Stage 1
      mult0 <= 16'(16'($unsigned(image_data0)) * 16'($unsigned(kernel0)));
      mult1 <= 16'(16'($unsigned(image_data1)) * 16'($unsigned(kernel1)));
      mult2 <= 16'(16'($unsigned(image_data2)) * 16'($unsigned(kernel2)));
      mult3 <= 16'(16'($unsigned(image_data3)) * 16'($unsigned(kernel3)));
      mult4 <= 16'(16'($unsigned(image_data4)) * 16'($unsigned(kernel4)));
      mult5 <= 16'(16'($unsigned(image_data5)) * 16'($unsigned(kernel5)));
      mult6 <= 16'(16'($unsigned(image_data6)) * 16'($unsigned(kernel6)));
      mult7 <= 16'(16'($unsigned(image_data7)) * 16'($unsigned(kernel7)));
      mult8 <= 16'(16'($unsigned(image_data8)) * 16'($unsigned(kernel8)));
      // Stage 2: row sums
      row0 <= 20'(20'($unsigned(mult0)) + 20'($unsigned(mult1)) + 20'($unsigned(mult2)));
      row1 <= 20'(20'($unsigned(mult3)) + 20'($unsigned(mult4)) + 20'($unsigned(mult5)));
      row2 <= 20'(20'($unsigned(mult6)) + 20'($unsigned(mult7)) + 20'($unsigned(mult8)));
      // Stage 3: total sum
      total <= 20'(row0 + row1 + row2);
      // Stage 4: normalize by 9
      result_reg <= 16'(total / 9);
    end
  end

endmodule

