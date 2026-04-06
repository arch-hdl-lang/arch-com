// E203 ITCM RAM Wrapper
// 8K x 64-bit single-port SRAM with byte-write mask.
// Thin wrapper with ASIC power management pins (sd/ds/ls).
// Read address is registered on cs & !we; output is combinational from stored address.
module e203_itcm_ram #(
  parameter int DEPTH = 8192
) (
  input logic clk,
  input logic rst_n,
  input logic sd,
  input logic ds,
  input logic ls,
  input logic cs,
  input logic we,
  input logic [13-1:0] addr,
  input logic [8-1:0] wem,
  input logic [64-1:0] din,
  output logic [64-1:0] dout
);

  // ASIC power management (unused in sim, present for macro compatibility)
  // SRAM interface
  logic [64-1:0] mem [8192-1:0];
  logic [13-1:0] addr_r;
  always_ff @(posedge clk) begin
    // Read: latch address when cs=1 and we=0
    if (cs & ~we) begin
      addr_r <= addr;
    end
    // Write: byte-masked when cs=1 and we=1
    if (cs & we) begin
      for (int i = 0; i <= 7; i++) begin
        if (wem[i +: 1]) begin
          mem[addr][i * 8 +: 8] <= din[i * 8 +: 8];
        end
      end
    end
  end
  // Output is combinational from registered address
  assign dout = mem[addr_r];

endmodule

