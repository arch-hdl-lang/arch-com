module single_port_ram #(
  parameter int DATA_WIDTH = 8,
  parameter int ADDR_WIDTH = 4
) (
  input  logic                    clk,
  input  logic                    we,
  input  logic [ADDR_WIDTH-1:0]   addr,
  input  logic [DATA_WIDTH-1:0]   din,
  output logic [DATA_WIDTH-1:0]   dout
);

  logic [DATA_WIDTH-1:0] mem [0:(1<<ADDR_WIDTH)-1];

  always_ff @(posedge clk) begin
    if (we)
      mem[addr] <= din;
    dout <= mem[addr];
  end

endmodule
