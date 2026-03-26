Wrote tests/verilog_eval/Prob004_vector2.sv
f 32-bit vector
module TopModule (
  input logic [32-1:0] in,
  output logic [32-1:0] out
);

  assign out = {in[7:0], in[15:8], in[23:16], in[31:24]};

endmodule

