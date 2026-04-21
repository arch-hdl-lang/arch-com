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

  logic [TLB_SIZE-1:0] tlb_valid;
  logic in_page_table;
  assign in_page_table = virtual_address < 8;
  logic in_tlb_range;
  assign in_tlb_range = virtual_address < TLB_SIZE;
  always_comb begin
    if (in_page_table) begin
      physical_address = virtual_address;
    end else begin
      physical_address = 0;
    end
    if (in_tlb_range) begin
      hit = tlb_valid[virtual_address +: 1];
    end else begin
      hit = 1'b0;
    end
    miss = ~in_page_table;
  end
  always_ff @(posedge clk) begin
    if (reset) begin
      tlb_valid <= 0;
    end else begin
      if (in_tlb_range) begin
        tlb_valid[virtual_address +: 1] <= 1;
      end
    end
  end

endmodule

