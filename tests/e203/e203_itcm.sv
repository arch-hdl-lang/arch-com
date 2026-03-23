// domain SysDomain
//   freq_mhz: 100

module ItcmSram #(
  parameter int DEPTH = 16384,
  parameter int DATA_WIDTH = 32
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
  assign rd_port_data = rd_port_data_r;

endmodule

module E203Itcm (
  input logic clk,
  input logic rst_n,
  input logic rd_en,
  input logic [14-1:0] rd_addr,
  output logic [32-1:0] rd_data,
  input logic wr_en,
  input logic [14-1:0] wr_addr,
  input logic [32-1:0] wr_data
);

  ItcmSram mem (
    .clk(clk),
    .rd_port_en(rd_en),
    .rd_port_addr(rd_addr),
    .rd_port_data(rd_data),
    .wr_port_en(wr_en),
    .wr_port_addr(wr_addr),
    .wr_port_data(wr_data)
  );

endmodule

