module palindrome_detect #(
  parameter int N = 3
) (
  input logic clk,
  input logic reset,
  input logic bit_stream,
  output logic palindrome_detected
);

  logic [2:0] sr;
  logic [2:0] sr_next;
  assign sr_next = sr[1:0] << 1 | 3'($unsigned(bit_stream));
  always_ff @(posedge clk) begin
    if (reset) begin
      sr <= 0;
    end else begin
      sr <= sr_next;
    end
  end
  assign palindrome_detected = sr[2:2] == sr[0:0];

endmodule

