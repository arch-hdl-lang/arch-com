Wrote tests/verilog_eval/Prob042_vector4.sv

  output logic [32-1:0] out
);

  assign out = {{24{in[7]}}, in};

endmodule

