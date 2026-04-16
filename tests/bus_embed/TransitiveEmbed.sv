package PkgTrans;
endpackage

// Level 0: leaf bus
// Level 1: embeds leaf
// Level 2: embeds mid (transitive — should include leaf_data)
// Module using the transitively-composed bus
module TransTest (
  input logic clk,
  input logic rst,
  output logic top_port_top_flag,
  output logic top_port_mid_ctrl,
  output logic [7:0] top_port_mid_leaf_data
);

  assign top_port_top_flag = 1'b1;
  assign top_port_mid_ctrl = 1'b1;
  assign top_port_mid_leaf_data = 42;

endmodule

