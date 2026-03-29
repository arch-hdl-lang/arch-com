module advanced_decimator_with_adaptive_peak_detection #(
  parameter int N = 8,
  parameter int DATA_WIDTH = 16,
  parameter int DEC_FACTOR = 4,
  parameter int NUM_DEC = N / DEC_FACTOR
) (
  input logic clk,
  input logic reset,
  input logic valid_in,
  input logic [DATA_WIDTH * N-1:0] data_in,
  output logic valid_out,
  output logic [DATA_WIDTH * NUM_DEC-1:0] data_out,
  output logic signed [DATA_WIDTH-1:0] peak_value
);

  // Decimation: select every DEC_FACTOR-th sample and pack output
  logic [DATA_WIDTH * NUM_DEC-1:0] dec_packed;
  always_comb begin
    for (int i = 0; i <= NUM_DEC - 1; i++) begin
      dec_packed[i * DATA_WIDTH +: DATA_WIDTH] = data_in[i * DEC_FACTOR * DATA_WIDTH +: DATA_WIDTH];
    end
  end
  // Peak detection: find max among decimated samples (signed comparison)
  logic signed [DATA_WIDTH-1:0] peak;
  always_comb begin
    peak = $signed(data_in[DATA_WIDTH - 1:0]);
    for (int i = 0; i <= NUM_DEC - 1; i++) begin
      if ($signed(dec_packed[i * DATA_WIDTH +: DATA_WIDTH]) > peak) begin
        peak = $signed(dec_packed[i * DATA_WIDTH +: DATA_WIDTH]);
      end
    end
  end
  // Register outputs (1-cycle latency)
  always_ff @(posedge clk or posedge reset) begin
    if (reset) begin
      data_out <= 0;
      peak_value <= 0;
      valid_out <= 1'b0;
    end else begin
      valid_out <= valid_in;
      data_out <= dec_packed;
      peak_value <= peak;
    end
  end

endmodule

