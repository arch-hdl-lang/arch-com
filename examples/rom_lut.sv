// ROM lookup table — sine approximation (8 entries, 8-bit values)
module RomLut #(
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
  
  initial begin
    mem[0] = 8'h0;
    mem[1] = 8'h31;
    mem[2] = 8'h5A;
    mem[3] = 8'h76;
    mem[4] = 8'h7F;
    mem[5] = 8'h76;
    mem[6] = 8'h5A;
    mem[7] = 8'h31;
  end

endmodule

