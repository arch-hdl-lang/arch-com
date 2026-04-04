module Bit_Change_Detector (
  input logic clk,
  input logic reset,
  input logic bit_in,
  output logic change_pulse
);

  logic bit_in_d;
  always_ff @(posedge clk or posedge reset) begin
    if (reset) begin
      bit_in_d <= 0;
    end else begin
      bit_in_d <= bit_in;
    end
  end
  assign change_pulse = bit_in ^ bit_in_d;

endmodule

