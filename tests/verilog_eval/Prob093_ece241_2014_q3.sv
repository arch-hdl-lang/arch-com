Wrote tests/verilog_eval/Prob093_ece241_2014_q3.sv
,
  output logic [4-1:0] mux_in
);

  assign mux_in = {c & d, ~d, 1'd0, c | d};

endmodule

