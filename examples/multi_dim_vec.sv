module MultiDimVec (
  input logic clk,
  input logic rst,
  input logic wr_en,
  input logic [2-1:0] row_sel,
  input logic [3-1:0] col_sel,
  input logic [8-1:0] din,
  output logic [8-1:0] dout
);

  // 2D array: 4 rows x 8 cols of UInt<8>
  logic [8-1:0] storage [0:4-1] [0:8-1];
  always_ff @(posedge clk) begin
    if (rst) begin
      storage <= '{default: '{default: 0}};
    end else begin
      if (wr_en == 1) begin
        storage[row_sel][col_sel] <= din;
      end
    end
  end
  assign dout = storage[row_sel][col_sel];

endmodule

