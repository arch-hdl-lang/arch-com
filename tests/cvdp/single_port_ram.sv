module single_port_ram #(
  parameter int DATA_WIDTH = 8,
  parameter int ADDR_WIDTH = 4
) (
  input logic clk,
  input logic we,
  input logic [ADDR_WIDTH-1:0] addr,
  input logic [DATA_WIDTH-1:0] din,
  output logic [DATA_WIDTH-1:0] dout
);

  logic [15:0] [DATA_WIDTH-1:0] mem;
  always_ff @(posedge clk) begin
    if (we) begin
      mem[4'(addr)] <= din;
    end
    dout <= mem[4'(addr)];
  end

endmodule

