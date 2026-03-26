Wrote tests/verilog_eval/Prob070_ece241_2013_q2.sv
,
  input logic c,
  input logic d,
  output logic out_sop,
  output logic out_pos
);

  assign out_sop = ~a & ~b & c | c & d;
  assign out_pos = c & (~b | d) & (~a | b);

endmodule

