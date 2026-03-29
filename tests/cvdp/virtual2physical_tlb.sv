module ControlUnit (
  input logic clk,
  input logic rst,
  input logic hit,
  input logic miss,
  input logic ready,
  output logic tlb_write_enable,
  output logic flsh
);

  assign tlb_write_enable = miss;
  assign flsh = 1'b0;

endmodule

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

module TLB #(
  parameter int TLB_SIZE = 4,
  parameter int ADDR_WIDTH = 8,
  parameter int PAGE_WIDTH = 8
) (
  input logic clk,
  input logic rst,
  input logic [ADDR_WIDTH-1:0] virtual_address,
  input logic tlb_write_enable,
  input logic flsh,
  input logic [PAGE_WIDTH-1:0] page_table_entry,
  output logic [PAGE_WIDTH-1:0] physical_address,
  output logic hit,
  output logic miss
);

  logic [ADDR_WIDTH-1:0] virtual_tags_0;
  logic [ADDR_WIDTH-1:0] virtual_tags_1;
  logic [ADDR_WIDTH-1:0] virtual_tags_2;
  logic [ADDR_WIDTH-1:0] virtual_tags_3;
  logic [PAGE_WIDTH-1:0] physical_pages_0;
  logic [PAGE_WIDTH-1:0] physical_pages_1;
  logic [PAGE_WIDTH-1:0] physical_pages_2;
  logic [PAGE_WIDTH-1:0] physical_pages_3;
  logic [TLB_SIZE-1:0] valid_bits;
  logic [2-1:0] replacement_idx;
  logic v0;
  assign v0 = virtual_tags_0 == virtual_address;
  logic v1;
  assign v1 = virtual_tags_1 == virtual_address;
  logic v2;
  assign v2 = virtual_tags_2 == virtual_address;
  logic v3;
  assign v3 = virtual_tags_3 == virtual_address;
  logic match0;
  assign match0 = valid_bits[0:0] & v0;
  logic match1;
  assign match1 = valid_bits[1:1] & v1;
  logic match2;
  assign match2 = valid_bits[2:2] & v2;
  logic match3;
  assign match3 = valid_bits[3:3] & v3;
  always_comb begin
    if (match0) begin
      hit = 1'b1;
      miss = 1'b0;
      physical_address = physical_pages_0;
    end else if (match1) begin
      hit = 1'b1;
      miss = 1'b0;
      physical_address = physical_pages_1;
    end else if (match2) begin
      hit = 1'b1;
      miss = 1'b0;
      physical_address = physical_pages_2;
    end else if (match3) begin
      hit = 1'b1;
      miss = 1'b0;
      physical_address = physical_pages_3;
    end else begin
      hit = 1'b0;
      miss = 1'b1;
      physical_address = page_table_entry;
    end
  end
  always_ff @(posedge clk) begin
    if (rst) begin
      physical_pages_0 <= 0;
      physical_pages_1 <= 0;
      physical_pages_2 <= 0;
      physical_pages_3 <= 0;
      replacement_idx <= 0;
      valid_bits <= 0;
      virtual_tags_0 <= 0;
      virtual_tags_1 <= 0;
      virtual_tags_2 <= 0;
      virtual_tags_3 <= 0;
    end else begin
      if (rst) begin
        valid_bits <= 0;
        replacement_idx <= 0;
        virtual_tags_0 <= 0;
        virtual_tags_1 <= 0;
        virtual_tags_2 <= 0;
        virtual_tags_3 <= 0;
        physical_pages_0 <= 0;
        physical_pages_1 <= 0;
        physical_pages_2 <= 0;
        physical_pages_3 <= 0;
      end else if (flsh) begin
        valid_bits <= 0;
      end else if (tlb_write_enable) begin
        if (replacement_idx == 0) begin
          virtual_tags_0 <= virtual_address;
          physical_pages_0 <= page_table_entry;
          valid_bits <= valid_bits | 4'd1;
          replacement_idx <= 1;
        end else if (replacement_idx == 1) begin
          virtual_tags_1 <= virtual_address;
          physical_pages_1 <= page_table_entry;
          valid_bits <= valid_bits | 4'd2;
          replacement_idx <= 2;
        end else if (replacement_idx == 2) begin
          virtual_tags_2 <= virtual_address;
          physical_pages_2 <= page_table_entry;
          valid_bits <= valid_bits | 4'd4;
          replacement_idx <= 3;
        end else begin
          virtual_tags_3 <= virtual_address;
          physical_pages_3 <= page_table_entry;
          valid_bits <= valid_bits | 4'd8;
          replacement_idx <= 0;
        end
      end
    end
  end

endmodule

module virtual2physical_tlb #(
  parameter int ADDR_WIDTH = 8,
  parameter int PAGE_WIDTH = 8,
  parameter int TLB_SIZE = 4,
  parameter int PAGE_TABLE_SIZE = 16
) (
  input logic clk,
  input logic reset,
  input logic [ADDR_WIDTH-1:0] virtual_address,
  output logic [PAGE_WIDTH-1:0] physical_address,
  output logic hit,
  output logic miss
);

  logic tlb_write_enable;
  logic flsh;
  logic ready;
  logic [PAGE_WIDTH-1:0] page_table_entry;
  TLB #(.TLB_SIZE(TLB_SIZE), .ADDR_WIDTH(ADDR_WIDTH), .PAGE_WIDTH(PAGE_WIDTH)) tlb_inst (
    .clk(clk),
    .rst(reset),
    .virtual_address(virtual_address),
    .tlb_write_enable(tlb_write_enable),
    .flsh(flsh),
    .page_table_entry(page_table_entry),
    .physical_address(physical_address),
    .hit(hit),
    .miss(miss)
  );
  PageTableHandler #(.ADDR_WIDTH(ADDR_WIDTH), .PAGE_WIDTH(PAGE_WIDTH), .PAGE_TABLE_SIZE(PAGE_TABLE_SIZE)) pth_inst (
    .clk(clk),
    .rst(reset),
    .miss(miss),
    .virtual_page(virtual_address),
    .page_table_entry(page_table_entry),
    .ready(ready)
  );
  ControlUnit cu_inst (
    .clk(clk),
    .rst(reset),
    .hit(hit),
    .miss(miss),
    .ready(ready),
    .tlb_write_enable(tlb_write_enable),
    .flsh(flsh)
  );

endmodule

