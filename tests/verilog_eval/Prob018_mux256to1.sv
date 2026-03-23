module TopModule (
  input logic [256-1:0] in_sig,
  input logic [8-1:0] sel,
  output logic out_sig
);

  assign out_sig = in_sig[sel];

endmodule

