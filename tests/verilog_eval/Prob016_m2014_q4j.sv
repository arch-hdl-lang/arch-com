Wrote tests/verilog_eval/Prob016_m2014_q4j.sv
 sum
module TopModule (
  input logic [4-1:0] x,
  input logic [4-1:0] y,
  output logic [5-1:0] sum
);

  assign sum = 5'(5'($unsigned(x)) + 5'($unsigned(y)));

endmodule

