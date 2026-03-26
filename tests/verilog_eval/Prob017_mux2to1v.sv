Wrote tests/verilog_eval/Prob017_mux2to1v.sv

  input logic [100-1:0] b,
  input logic sel,
  output logic [100-1:0] out
);

  assign out = sel ? b : a;

endmodule

