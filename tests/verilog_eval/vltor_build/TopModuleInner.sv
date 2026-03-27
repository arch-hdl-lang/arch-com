module TopModuleInner (
  input logic a,
  input logic b,
  input logic c,
  input logic d,
  output logic out_sig
);

  assign out_sig = (a | (c & (~b)));

endmodule

