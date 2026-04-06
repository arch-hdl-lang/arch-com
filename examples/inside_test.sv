module InsideTest (
  input logic [8-1:0] addr,
  input logic [4-1:0] val,
  output logic hit,
  output logic in_range
);

  assign hit = addr inside {8'd16, 8'd32, 8'd48};
  assign in_range = addr inside {[8'd0:8'd15], [8'd128:8'd255]};

endmodule

