Wrote tests/verilog_eval/Prob022_mux2to1.sv
logic b,
  input logic sel,
  output logic out
);

  assign out = sel == 1'd1 ? b : a;

endmodule

