module baud_rate_generator #(
  parameter int CLOCK_FREQ = 100000000,
  parameter int BAUD_RATE = 115200,
  parameter int BAUD_ACC_WIDTH = 16,
  parameter int BAUD_TICKS = CLOCK_FREQ / BAUD_RATE
) (
  input logic clock,
  input logic reset_neg,
  input logic enable,
  output logic baud_pulse
);

  // Exact baud timing via up-counter
  logic [32-1:0] cnt;
  logic pulse_w;
  assign pulse_w = enable & cnt == 32'($unsigned(BAUD_TICKS - 1));
  always_ff @(posedge clock or negedge reset_neg) begin
    if ((!reset_neg)) begin
      cnt <= 0;
    end else begin
      if (~reset_neg) begin
        cnt <= 0;
      end else if (~enable) begin
        cnt <= 0;
      end else if (cnt == 32'($unsigned(BAUD_TICKS - 1))) begin
        cnt <= 0;
      end else begin
        cnt <= 32'(cnt + 1);
      end
    end
  end
  assign baud_pulse = pulse_w;

endmodule

