module MultiDimVec (
  input logic clk,
  input logic rst,
  input logic wr_en,
  input logic [2-1:0] d0,
  input logic [2-1:0] d1,
  input logic [3-1:0] d2,
  input logic [8-1:0] din,
  output logic [8-1:0] dout
);

  // 3D array: 4 x 4 x 8 of UInt<8>
  logic [8-1:0] cube [0:4-1] [0:4-1] [0:8-1];
  always_ff @(posedge clk) begin
    if (rst) begin
      cube <= '{default: '{default: '{default: 0}}};
    end else begin
      if (wr_en == 1) begin
        cube[d0][d1][d2] <= din;
      end
    end
  end
  assign dout = cube[d0][d1][d2];

endmodule

