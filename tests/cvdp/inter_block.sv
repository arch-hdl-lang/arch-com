module inter_block #(
  parameter int ROW_COL_WIDTH = 16,
  parameter int SUB_BLOCKS = 4,
  parameter int DATA_WIDTH = ROW_COL_WIDTH * ROW_COL_WIDTH
) (
  input logic clk,
  input logic rst_n,
  input logic i_valid,
  input logic [DATA_WIDTH-1:0] in_data,
  output logic [DATA_WIDTH-1:0] out_data
);

  logic [DATA_WIDTH-1:0] data_reg;
  logic [8-1:0] blk_cnt;
  logic [DATA_WIDTH-1:0] xor_pattern;
  always_comb begin
    for (int i = 0; i <= DATA_WIDTH / 8 - 1; i++) begin
      xor_pattern[8 * i +: 8] = blk_cnt;
    end
  end
  assign out_data = data_reg;
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      blk_cnt <= 0;
      data_reg <= 0;
    end else begin
      if (i_valid) begin
        data_reg <= in_data ^ xor_pattern;
        if (blk_cnt == 8'($unsigned(SUB_BLOCKS - 1))) begin
          blk_cnt <= 0;
        end else begin
          blk_cnt <= 8'(blk_cnt + 1);
        end
      end
    end
  end

endmodule

