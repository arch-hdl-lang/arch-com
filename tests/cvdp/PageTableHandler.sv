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

  logic [PAGE_WIDTH-1:0] pt_mem_0;
  logic [PAGE_WIDTH-1:0] pt_mem_1;
  logic [PAGE_WIDTH-1:0] pt_mem_2;
  logic [PAGE_WIDTH-1:0] pt_mem_3;
  logic [PAGE_WIDTH-1:0] pt_mem_4;
  logic [PAGE_WIDTH-1:0] pt_mem_5;
  logic [PAGE_WIDTH-1:0] pt_mem_6;
  logic [PAGE_WIDTH-1:0] pt_mem_7;
  logic [PAGE_WIDTH-1:0] pt_mem_8;
  logic [PAGE_WIDTH-1:0] pt_mem_9;
  logic [PAGE_WIDTH-1:0] pt_mem_10;
  logic [PAGE_WIDTH-1:0] pt_mem_11;
  logic [PAGE_WIDTH-1:0] pt_mem_12;
  logic [PAGE_WIDTH-1:0] pt_mem_13;
  logic [PAGE_WIDTH-1:0] pt_mem_14;
  logic [PAGE_WIDTH-1:0] pt_mem_15;
  logic rdy;
  always_comb begin
    if (virtual_page == 0) begin
      page_table_entry = pt_mem_0;
    end else if (virtual_page == 1) begin
      page_table_entry = pt_mem_1;
    end else if (virtual_page == 2) begin
      page_table_entry = pt_mem_2;
    end else if (virtual_page == 3) begin
      page_table_entry = pt_mem_3;
    end else if (virtual_page == 4) begin
      page_table_entry = pt_mem_4;
    end else if (virtual_page == 5) begin
      page_table_entry = pt_mem_5;
    end else if (virtual_page == 6) begin
      page_table_entry = pt_mem_6;
    end else if (virtual_page == 7) begin
      page_table_entry = pt_mem_7;
    end else if (virtual_page == 8) begin
      page_table_entry = pt_mem_8;
    end else if (virtual_page == 9) begin
      page_table_entry = pt_mem_9;
    end else if (virtual_page == 10) begin
      page_table_entry = pt_mem_10;
    end else if (virtual_page == 11) begin
      page_table_entry = pt_mem_11;
    end else if (virtual_page == 12) begin
      page_table_entry = pt_mem_12;
    end else if (virtual_page == 13) begin
      page_table_entry = pt_mem_13;
    end else if (virtual_page == 14) begin
      page_table_entry = pt_mem_14;
    end else begin
      page_table_entry = pt_mem_15;
    end
  end
  assign ready = rdy;
  always_ff @(posedge clk) begin
    if (rst) begin
      rdy <= 1'b0;
    end else begin
      if (rst) begin
        rdy <= 1'b0;
      end else if (miss) begin
        rdy <= 1'b1;
      end else begin
        rdy <= 1'b0;
      end
    end
  end

endmodule

