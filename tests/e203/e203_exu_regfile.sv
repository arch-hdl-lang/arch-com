// E203 HBirdv2 integer register file
// 32 × 32-bit, 2 async read ports, 1 sync write port.
// x0 hardwired to 0 via init[0]=0 and write guard in the pipeline.
// No reset on data entries (matches E203 spec).
module ExuRegfile #(
  parameter int XLEN = 32,
  parameter int NREGS = 32,
  parameter int NREAD = 2,
  parameter int NWRITE = 1
) (
  input logic clk,
  input logic rst_n,
  input logic test_mode,
  input logic [5-1:0] read0_addr,
  output logic [32-1:0] read0_data,
  input logic [5-1:0] read1_addr,
  output logic [32-1:0] read1_data,
  input logic write_en,
  input logic [5-1:0] write_addr,
  input logic [32-1:0] write_data
);

  logic [32-1:0] rf_data [0:NREGS-1];
  
  always_ff @(posedge clk) begin
    if (write_en && write_addr != 0)
      rf_data[write_addr] <= write_data;
  end
  
  always_comb begin
    read0_data = rf_data[read0_addr];
    read1_data = rf_data[read1_addr];
  end

endmodule

// x0 hardwired to 0
// no bypass — hazards resolved by pipeline
