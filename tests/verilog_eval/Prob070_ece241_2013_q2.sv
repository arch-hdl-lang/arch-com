module TopModule (
  input logic a,
  input logic b,
  input logic c,
  input logic d,
  output logic out_sop,
  output logic out_pos
);

  assign out_sop = ((((~a) & (~b)) & c) | (c & d));
  assign out_pos = ((c & ((~b) | d)) & ((~a) | b));

endmodule

