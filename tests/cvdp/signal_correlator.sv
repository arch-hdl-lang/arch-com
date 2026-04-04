module signal_correlator (
  input logic clk,
  input logic reset,
  input logic [8-1:0] input_signal,
  input logic [8-1:0] reference_signal,
  output logic [4-1:0] correlation_output
);

  logic [4-1:0] corr_r;
  // Each matching 1-bit AND pair adds 2; clamp at 15
  logic [5-1:0] match_sum;
  always_comb begin
    match_sum = 0;
    for (int i = 0; i <= 7; i++) begin
      if (input_signal[i +: 1] & reference_signal[i +: 1]) begin
        if (match_sum < 14) begin
          match_sum = 5'(match_sum + 2);
        end else begin
          match_sum = 15;
        end
      end
    end
  end
  assign correlation_output = corr_r;
  always_ff @(posedge clk or posedge reset) begin
    if (reset) begin
      corr_r <= 0;
    end else begin
      corr_r <= 4'(match_sum);
    end
  end

endmodule

