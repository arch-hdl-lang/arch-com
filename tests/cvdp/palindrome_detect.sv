module palindrome_detect #(
  parameter int N = 3
) (
  input logic clk,
  input logic reset,
  input logic bit_stream,
  output logic palindrome_detected
);

  logic sr0;
  logic sr1;
  logic sr2;
  always_ff @(posedge clk) begin
    if (reset) begin
      sr0 <= 0;
      sr1 <= 0;
      sr2 <= 0;
    end else begin
      sr0 <= bit_stream;
      sr1 <= sr0;
      sr2 <= sr1;
    end
  end
  assign palindrome_detected = sr0 == sr2;

endmodule

