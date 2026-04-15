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
  logic [31:0] cnt;
  logic pulse_w;
  assign pulse_w = enable & (cnt == 32'($unsigned(BAUD_TICKS - 1)));
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

module copilot_rs_232 #(
  parameter int CLOCK_FREQ = 100000000,
  parameter int BAUD_RATE = 115200,
  parameter int BAUD_ACC_WIDTH = 16,
  parameter int REG_INPUT = 1
) (
  input logic clock,
  input logic reset_neg,
  input logic tx_datain_ready,
  input logic Present_Processing_Completed,
  input logic [7:0] tx_datain,
  output logic tx_transmitter,
  output logic tx_transmitter_valid
);

  // Shift register: {stop, data[7:0], start} = {1, D7..D0, 0}
  logic [9:0] tx_shift;
  logic [3:0] bit_cnt;
  logic baud_pulse_w;
  logic active;
  assign active = bit_cnt != 0;
  baud_rate_generator #(.BAUD_ACC_WIDTH(BAUD_ACC_WIDTH), .CLOCK_FREQ(CLOCK_FREQ), .BAUD_RATE(BAUD_RATE)) baud_gen (
    .clock(clock),
    .reset_neg(reset_neg),
    .enable(active),
    .baud_pulse(baud_pulse_w)
  );
  // Registered valid output
  logic valid_r;
  always_ff @(posedge clock or negedge reset_neg) begin
    if ((!reset_neg)) begin
      valid_r <= 1'b0;
    end else begin
      valid_r <= active;
    end
  end
  assign tx_transmitter_valid = valid_r;
  // Load shift register and start transmission
  always_ff @(posedge clock or negedge reset_neg) begin
    if ((!reset_neg)) begin
      bit_cnt <= 0;
      tx_shift <= 'b1111111111;
    end else begin
      if (~reset_neg) begin
        tx_shift <= 'b1111111111;
        bit_cnt <= 0;
      end else if (Present_Processing_Completed) begin
        tx_shift <= 'b1111111111;
        bit_cnt <= 0;
      end else if ((bit_cnt == 0) & tx_datain_ready) begin
        tx_shift <= {1'b1, tx_datain, 1'b0};
        bit_cnt <= 10;
      end else if (baud_pulse_w & (bit_cnt != 0)) begin
        tx_shift <= {1'b1, tx_shift[9:1]};
        bit_cnt <= 4'(bit_cnt - 1);
      end
    end
  end
  // Output is LSB of shift register
  assign tx_transmitter = tx_shift[0];

endmodule

