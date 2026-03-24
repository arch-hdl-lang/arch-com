module TopModule (
  input logic c,
  input logic d,
  output logic [4-1:0] mux_in
);

  assign mux_in = {(c & d), (~d), 1'd0, (c | d)};

endmodule

