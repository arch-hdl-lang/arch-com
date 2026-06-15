/// LZ4 history ring buffer.
///
/// 65536-byte simple_dual-port SRAM backing the back-reference copy
/// window.  Write port and read port operate independently at 1-cycle
/// read latency.  The write-then-read overlap that LZ4's RLE-style
/// sequences require is handled at the FSM level: the read address for
/// copy byte N is issued one cycle before the data is needed, so a byte
/// written at cycle T is visible to a read address issued at cycle T+1.
module Lz4HistBuf #(
  parameter int DEPTH = 65536,
  parameter int DATA_WIDTH = 8
) (
  input logic clk,
  input logic wr_en,
  input logic [15:0] wr_addr,
  input logic [DATA_WIDTH-1:0] wr_data,
  input logic rd_en,
  input logic [15:0] rd_addr,
  output logic [DATA_WIDTH-1:0] rd_data
);

  logic [DATA_WIDTH-1:0] mem [0:DEPTH-1];
  logic [DATA_WIDTH-1:0] rd_data_r;
  
  always_ff @(posedge clk) begin
    if (wr_en)
      mem[wr_addr] <= wr_data;
    if (rd_en)
      rd_data_r <= mem[rd_addr];
  end
  assign rd_data = rd_data_r;

endmodule

