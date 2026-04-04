// LRU pseudo-tree state SRAM — one 7-bit tree per set.
// 7 bits = WAYS-1 internal nodes for 8-way pseudo-LRU tree.
// Bit layout: tree[6:0] where bit i steers left(0)/right(1) at node i.
module RamLruState #(
  parameter int DEPTH = 64,
  parameter int DATA_WIDTH = 7
) (
  input logic clk,
  input logic rd_port_en,
  input logic [6-1:0] rd_port_addr,
  output logic [DATA_WIDTH-1:0] rd_port_rdata,
  input logic wr_port_en,
  input logic [6-1:0] wr_port_addr,
  input logic [DATA_WIDTH-1:0] wr_port_wdata
);

  logic [DATA_WIDTH-1:0] mem [0:DEPTH-1];
  logic [DATA_WIDTH-1:0] rd_port_rdata_r;
  
  always_ff @(posedge clk) begin
    if (wr_port_en)
      mem[wr_port_addr] <= wr_port_wdata;
    if (rd_port_en)
      rd_port_rdata_r <= mem[rd_port_addr];
  end
  assign rd_port_rdata = rd_port_rdata_r;

endmodule

