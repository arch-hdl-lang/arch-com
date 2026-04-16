package PkgRemap;
endpackage

// Parent bus uses DIFFERENT param names than the embedded bus.
// This tests that param substitution maps correctly.
// Module that uses BusWide with non-default params.
// This verifies the full chain: parent param override → embed param subst → SV type.
module ParamRemapTest #(
  parameter int W = 64
) (
  input logic clk,
  input logic rst,
  output logic bus_port_ch_valid,
  input logic bus_port_ch_ready,
  output logic [W-1:0] bus_port_ch_addr,
  output logic [7:0] bus_port_ch_id,
  output logic [7:0] bus_port_ch_len
);

  assign bus_port_ch_valid = 1'b1;
  assign bus_port_ch_addr = 0;
  assign bus_port_ch_id = 0;
  assign bus_port_ch_len = 0;

endmodule

// Minimal AXI address channel bus
// AXI R data channel bus
// AXI W data channel bus
// Composed read-only AXI bus using embed
// Composed write-only AXI bus using embed
// Composed full AXI bus: re-embeds the same primitives
