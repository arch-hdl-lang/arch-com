module PageTableHandler #(
  parameter int ADDR_WIDTH = 8,
  parameter int PAGE_WIDTH = 8,
  parameter int PAGE_TABLE_SIZE = 16
) (
  input logic clk,
  input logic rst,
  input logic miss,
  input logic [ADDR_WIDTH-1:0] virtual_page,
  output logic [PAGE_WIDTH-1:0] page_table_entry,
  output logic ready
);

  logic rdy;
  assign page_table_entry = virtual_page[PAGE_WIDTH - 1:0];
  assign ready = rdy;
  always_ff @(posedge clk) begin
    if (rst) begin
      rdy <= 1'b0;
    end else begin
      if (miss) begin
        rdy <= 1'b1;
      end else begin
        rdy <= 1'b0;
      end
    end
  end

endmodule

