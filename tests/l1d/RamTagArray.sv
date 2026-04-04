// Tag SRAM — one instance per cache way.
// Entry encoding (54 bits): tag[53:2] | dirty[1] | valid[0]
// TAG_WIDTH=52 for SETS=64, LINE_BYTES=64, ADDR_W=64
module RamTagArray #(
  parameter int DEPTH = 64,
  parameter int DATA_WIDTH = 54
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

