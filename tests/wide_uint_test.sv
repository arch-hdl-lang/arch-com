// Test wide UInt types: pack/unpack 512-bit cache line, equality, shifts
module WideUintTest (
  input logic clk,
  input logic rst,
  input logic [512-1:0] line_in,
  output logic [512-1:0] line_out,
  output logic [64-1:0] word0_out,
  input logic [2048-1:0] huge_in,
  output logic [2048-1:0] huge_out,
  input logic [64-1:0] w0,
  input logic [64-1:0] w1,
  input logic [64-1:0] w2,
  input logic [64-1:0] w3,
  input logic [64-1:0] w4,
  input logic [64-1:0] w5,
  input logic [64-1:0] w6,
  input logic [64-1:0] w7,
  output logic [512-1:0] packed_out,
  output logic eq_result
);

  // 512-bit cache line ports
  // Extract word 0 (bits 63:0)
  // 2048-bit max port
  // Pack 8 × 64-bit words into 512-bit line
  // Equality check
  // Registered 512-bit buffer
  logic [512-1:0] line_buf;
  always_ff @(posedge clk) begin
    if (rst) begin
      line_buf <= 0;
    end else begin
      line_buf <= line_in;
    end
  end
  assign line_out = line_buf;
  assign huge_out = huge_in;
  assign word0_out = line_buf[63:0];
  assign packed_out = {w7, w6, w5, w4, w3, w2, w1, w0};
  assign eq_result = line_buf == line_in;

endmodule

// Pass through
// Extract word 0 from registered line
// Pack 8 words into 512-bit line
// Equality
