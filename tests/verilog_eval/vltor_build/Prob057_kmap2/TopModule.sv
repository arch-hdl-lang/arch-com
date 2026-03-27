module TopModule (
  input logic a,
  input logic b,
  input logic c,
  input logic d,
  output logic out
);

  assign out = ~b & ~c | ~a & b & c | ~a & ~d & b & ~c | ~a & ~d & ~b & c | a & c & d;

endmodule

