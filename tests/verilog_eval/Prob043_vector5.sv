Wrote tests/verilog_eval/Prob043_vector5.sv
logic b,
  input logic c,
  input logic d,
  input logic e,
  output logic [25-1:0] out
);

  assign out = ~{{5{a}}, {5{b}}, {5{c}}, {5{d}}, {5{e}}} ^ {5{{a, b, c, d, e}}};

endmodule

