// Data SRAM — stores cache line words across all ways.
// Address = {set_index[5:0], way_index[2:0], word_offset[2:0]} = 12 bits
// Depth = SETS(64) x WAYS(8) x WORDS_PER_LINE(8) = 4096 entries
// Byte-enable masking is done externally (RMW in the controller).
module RamDataArray #(
  parameter int DEPTH = 4096,
  parameter int DATA_WIDTH = 64
) (
  input logic clk,
  input logic rd_port_en,
  input logic [12-1:0] rd_port_addr,
  output logic [DATA_WIDTH-1:0] rd_port_rdata,
  input logic wr_port_en,
  input logic [12-1:0] wr_port_addr,
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

