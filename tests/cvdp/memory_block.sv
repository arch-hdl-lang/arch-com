module memory_block (
  input logic axi_clk,
  input logic ctrld_clk,
  input logic reset_in,
  input logic we,
  input logic [4-1:0] write_address,
  input logic [32-1:0] write_data,
  input logic [4-1:0] address_a,
  input logic [4-1:0] address_b,
  input logic [4-1:0] address_c,
  output logic [32-1:0] data_a,
  output logic [32-1:0] data_b,
  output logic [32-1:0] data_c,
  output logic [32-1:0] result_address
);

  logic [32-1:0] ram [16-1:0];
  logic [32-1:0] result_address_r;
  // Synchronous write on axi_clk
  always_ff @(posedge axi_clk) begin
    if (reset_in) begin
      for (int __ri0 = 0; __ri0 < 16; __ri0++) begin
        ram[__ri0] <= 0;
      end
      result_address_r <= 0;
    end else begin
      if (we) begin
        ram[write_address] <= write_data;
        if (write_address == 0) begin
          result_address_r <= write_data;
        end
      end
    end
  end
  // Asynchronous read (combinational)
  assign data_a = ram[address_a];
  assign data_b = ram[address_b];
  assign data_c = ram[address_c];
  assign result_address = result_address_r;

endmodule

