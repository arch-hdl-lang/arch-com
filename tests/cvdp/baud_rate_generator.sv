module baud_rate_generator #(
  parameter int CLOCK_FREQ = 100000000,
  parameter int BAUD_RATE = 115200,
  parameter int BAUD_ACC_WIDTH = 16
) (
  input logic clock,
  input logic reset_neg,
  input logic enable,
  output logic baud_pulse
);

  logic [BAUD_ACC_WIDTH + 1-1:0] baud_inc;
  assign baud_inc = (BAUD_ACC_WIDTH + 1)'(((BAUD_RATE << BAUD_ACC_WIDTH - 4) + (CLOCK_FREQ >> 5)) / (CLOCK_FREQ >> 4));
  logic [BAUD_ACC_WIDTH + 1-1:0] baud_acc;
  always_ff @(posedge clock or negedge reset_neg) begin
    if ((!reset_neg)) begin
      baud_acc <= 0;
    end else begin
      if (~reset_neg) begin
        baud_acc <= 0;
      end else if (enable) begin
        baud_acc <= (BAUD_ACC_WIDTH + 1)'((BAUD_ACC_WIDTH + 1)'($unsigned(baud_acc[BAUD_ACC_WIDTH - 1:0])) + baud_inc);
      end else begin
        baud_acc <= 0;
      end
    end
  end
  assign baud_pulse = baud_acc[BAUD_ACC_WIDTH];

endmodule

