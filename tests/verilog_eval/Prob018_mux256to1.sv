Wrote tests/verilog_eval/Prob018_mux256to1.sv

  input logic [8-1:0] sel,
  output logic out
);

  assign out = in[sel];

endmodule

