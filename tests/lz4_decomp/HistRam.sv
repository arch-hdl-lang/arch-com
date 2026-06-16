// 64KB sliding-window history buffer for the LZ4 block decompressor.
//
// simple_dual latency-1: read data appears on the rising edge one cycle
// after rd.en / rd.addr are presented.  Writes committed in the same
// cycle are NOT visible on a read in the same cycle, but ARE visible
// on reads issued in the next cycle — which satisfies LZ4's RLE case
// (offset=1 copy of the byte just written).
// domain SysDomain
//   freq_mhz: 200

module HistRam #(
  parameter int DEPTH = 256,
  parameter int DATA_WIDTH = 8
) (
  input logic clk,
  input logic rd_en,
  input logic [15:0] rd_addr,
  output logic [7:0] rd_rdata,
  input logic wr_en,
  input logic [15:0] wr_addr,
  input logic [7:0] wr_wdata
);

  logic [DATA_WIDTH-1:0] mem [0:DEPTH-1];
  logic [DATA_WIDTH-1:0] rd_rdata_r;
  
  always_ff @(posedge clk) begin
    if (wr_en)
      mem[wr_addr] <= wr_wdata;
    if (rd_en)
      rd_rdata_r <= mem[rd_addr];
  end
  assign rd_rdata = rd_rdata_r;

endmodule

