// domain SysDomain
//   freq_mhz: 100

module IntRegs #(
  parameter int NREGS = 32,
  parameter int DATA_WIDTH = 8
) (
  input logic clk,
  input logic rst,
  input logic [5-1:0] read_addr [0:2-1],
  output logic [8-1:0] read_data [0:2-1],
  input logic write_en,
  input logic [5-1:0] write_addr,
  input logic [8-1:0] write_data
);

  logic [DATA_WIDTH-1:0] regs [0:NREGS-1];
  
  always_ff @(posedge clk) begin
    if (rst) begin
      regs[0] <= 0;
    end else begin
      if (write_en)
        regs[write_addr] <= write_data;
    end
  end
  
  always_comb begin
    for (int r = 0; r < 2; r++) begin
      if (write_en && write_addr == read_addr[r])
        read_data[r] = write_data;
      else
        read_data[r] = regs[read_addr[r]];
    end
  end

endmodule

