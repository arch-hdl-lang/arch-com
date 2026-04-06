// ROM lookup table — sine approximation loaded from hex file
module RomLutFile #(
  parameter int DEPTH = 8,
  parameter int DATA_WIDTH = 8
) (
  input logic clk,
  input logic [3-1:0] rd_addr,
  input logic rd_en,
  output logic [8-1:0] rd_data
);

  logic [DATA_WIDTH-1:0] mem [0:DEPTH-1];
  logic [DATA_WIDTH-1:0] rd_data_r;
  
  always_ff @(posedge clk) begin
    if (rd_en)
      rd_data_r <= mem[rd_addr];
  end
  assign rd_data = rd_data_r;
  
  initial $readmemh("examples/rom_lut_sine.hex", mem);

endmodule

