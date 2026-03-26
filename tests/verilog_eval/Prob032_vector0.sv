Wrote tests/verilog_eval/Prob032_vector0.sv
,
  output logic [3-1:0] outv,
  output logic o2,
  output logic o1,
  output logic o0
);

  assign outv = vec;
  assign o0 = vec[0];
  assign o1 = vec[1];
  assign o2 = vec[2];

endmodule

