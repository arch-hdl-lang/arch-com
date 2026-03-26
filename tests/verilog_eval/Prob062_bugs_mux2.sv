Wrote tests/verilog_eval/Prob062_bugs_mux2.sv
logic [8-1:0] a,
  input logic [8-1:0] b,
  output logic [8-1:0] out
);

  assign out = sel ? a : b;

endmodule

