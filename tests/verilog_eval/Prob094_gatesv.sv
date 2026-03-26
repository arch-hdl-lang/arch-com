Wrote tests/verilog_eval/Prob094_gatesv.sv
,
  output logic [4-1:0] out_both,
  output logic [4-1:0] out_any,
  output logic [4-1:0] out_different
);

  assign out_both[0] = in[0] & in[1];
  assign out_both[1] = in[1] & in[2];
  assign out_both[2] = in[2] & in[3];
  assign out_both[3] = 0;
  assign out_any[0] = 0;
  assign out_any[1] = in[1] | in[0];
  assign out_any[2] = in[2] | in[1];
  assign out_any[3] = in[3] | in[2];
  assign out_different[0] = in[0] ^ in[1];
  assign out_different[1] = in[1] ^ in[2];
  assign out_different[2] = in[2] ^ in[3];
  assign out_different[3] = in[3] ^ in[0];

endmodule

