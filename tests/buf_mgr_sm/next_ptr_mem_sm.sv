// Next-Pointer SRAM: 16 x 4b, simple_dual, sync (1-cycle read)
module NextPtrMemSm #(
  parameter int DEPTH = 16,
  parameter int DATA_WIDTH = 4
) (
  input logic clk,
  input logic rd_port_en,
  input logic [4-1:0] rd_port_addr,
  output logic [DATA_WIDTH-1:0] rd_port_data,
  input logic wr_port_en,
  input logic [4-1:0] wr_port_addr,
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
  assign rd_port_data = rd_port_data_r;

endmodule

