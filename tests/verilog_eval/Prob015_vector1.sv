Wrote tests/verilog_eval/Prob015_vector1.sv
/lo bytes
module TopModule (
  input logic [16-1:0] in,
  output logic [8-1:0] out_hi,
  output logic [8-1:0] out_lo
);

  assign out_lo = in[7:0];
  assign out_hi = in[15:8];

endmodule

