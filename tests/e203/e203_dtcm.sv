// E203 HBirdv2 DTCM — Data Tightly Coupled Memory
// 64KB (16384 x 32-bit) simple dual-port SRAM wrapper.
// Supports simultaneous read and write with byte-strobe write enables.
// domain SysDomain
//   freq_mhz: 100

module DtcmSram #(
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

module Dtcm (
  input logic clk,
  input logic rst_n,
  input logic rd_en,
  input logic [14-1:0] rd_addr,
  output logic [32-1:0] rd_dout,
  input logic wr_en,
  input logic [4-1:0] wr_be,
  input logic [14-1:0] wr_addr,
  input logic [32-1:0] wr_din
);

  // Read port
  // Write port
  // Byte-masked write data: each bit of wr_be controls one byte lane.
  logic [8-1:0] byte0_mask;
  assign byte0_mask = {8{wr_be[0:0]}};
  logic [8-1:0] byte1_mask;
  assign byte1_mask = {8{wr_be[1:1]}};
  logic [8-1:0] byte2_mask;
  assign byte2_mask = {8{wr_be[2:2]}};
  logic [8-1:0] byte3_mask;
  assign byte3_mask = {8{wr_be[3:3]}};
  logic [32-1:0] full_mask;
  assign full_mask = {byte3_mask, byte2_mask, byte1_mask, byte0_mask};
  logic [32-1:0] masked_wdata;
  assign masked_wdata = (wr_din & full_mask);
  DtcmSram sram (
    .clk(clk),
    .rd_port_en(rd_en),
    .rd_port_addr(rd_addr),
    .rd_port_data(rd_dout),
    .wr_port_en(wr_en),
    .wr_port_addr(wr_addr),
    .wr_port_data(masked_wdata)
  );

endmodule

