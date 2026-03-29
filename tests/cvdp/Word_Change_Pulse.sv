module Word_Change_Pulse #(
  parameter int DATA_WIDTH = 8
) (
  input logic clk,
  input logic reset,
  input logic [DATA_WIDTH-1:0] data_in,
  input logic [DATA_WIDTH-1:0] mask,
  input logic [DATA_WIDTH-1:0] match_pattern,
  input logic enable,
  input logic latch_pattern,
  output logic word_change_pulse,
  output logic pattern_match_pulse,
  output logic [DATA_WIDTH-1:0] latched_pattern
);

  logic [DATA_WIDTH-1:0] change_pulses;
  logic [DATA_WIDTH-1:0] masked_data_in_r;
  logic [DATA_WIDTH-1:0] masked_change_pulses_r;
  logic match_detected_r;
  genvar i;
  for (i = 0; i <= DATA_WIDTH - 1; i = i + 1) begin : gen_i
    Bit_Change_Detector det_i (
      .clk(clk),
      .reset(reset),
      .bit_in(data_in[i +: 1]),
      .change_pulse(change_pulses[i +: 1])
    );
  end
  always_ff @(posedge clk or posedge reset) begin
    if (reset) begin
      latched_pattern <= 0;
      masked_change_pulses_r <= 0;
      masked_data_in_r <= 0;
      match_detected_r <= 1'b0;
      pattern_match_pulse <= 1'b0;
      word_change_pulse <= 1'b0;
    end else begin
      if (enable) begin
        if (latch_pattern) begin
          latched_pattern <= match_pattern;
        end
        masked_data_in_r <= data_in & mask;
        masked_change_pulses_r <= change_pulses & mask;
        if (masked_change_pulses_r != 0) begin
          word_change_pulse <= 1'b1;
        end else begin
          word_change_pulse <= 1'b0;
        end
        if (masked_data_in_r == (latched_pattern & mask)) begin
          match_detected_r <= 1'b1;
          pattern_match_pulse <= 1'b1;
        end else begin
          match_detected_r <= 1'b0;
          pattern_match_pulse <= 1'b0;
        end
      end else begin
        word_change_pulse <= 1'b0;
        pattern_match_pulse <= 1'b0;
      end
    end
  end

endmodule

