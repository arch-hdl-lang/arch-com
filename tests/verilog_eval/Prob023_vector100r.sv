Wrote tests/verilog_eval/Prob023_vector100r.sv
  output logic [100-1:0] out
);

  assign out = {<<1{in}};

endmodule

