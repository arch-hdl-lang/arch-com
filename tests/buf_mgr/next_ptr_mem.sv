// Next-Pointer SRAM: 16K x 14b, simple_dual, sync_out (2-cycle read)
module NextPtrMem #(
  parameter int DEPTH = 16384,
  parameter int DATA_WIDTH = 14
) (
  input logic clk,
  input logic rd_port_en,
  input logic [14-1:0] rd_port_addr,
  output logic [DATA_WIDTH-1:0] rd_port_data,
  input logic wr_port_en,
  input logic [14-1:0] wr_port_addr,
  input logic [DATA_WIDTH-1:0] wr_port_data
);

  logic [DATA_WIDTH-1:0] mem [0:DEPTH-1];
  logic [DATA_WIDTH-1:0] rd_port_data_r;
  
  always_ff @(posedge clk) begin
    if (wr_port_en)
      mem[wr_port_addr] <= wr_port_data;
    if (rd_port_en)
      rd_port_data_r <= mem[rd_port_addr];
  end
  logic [DATA_WIDTH-1:0] rd_port_data_r2;
  always_ff @(posedge clk) rd_port_data_r2 <= rd_port_data_r;
  assign rd_port_data = rd_port_data_r2;

endmodule

