// Integration test for generate_if/else: when condition is false, else ports/insts are active.
module GenElseTest (
  input logic clk,
  output logic [16-1:0] main_out,
  output logic [8-1:0] then_out
);

  // false → else branch: main_out exists, debug_out does not
  // true → then branch: then_out exists, skip_out does not
  assign main_out = 16'd43981;
  assign then_out = 8'd66;

endmodule

