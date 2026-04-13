module advanced_decimator_with_adaptive_peak_detection #(
  parameter int N = 8,
  parameter int DATA_WIDTH = 16,
  parameter int DEC_FACTOR = 4,
  localparam int NUM_OUT = N / DEC_FACTOR
) (
  input logic clk,
  input logic reset,
  input logic [0:0] valid_in,
  input logic [N * DATA_WIDTH-1:0] data_in,
  output logic [0:0] valid_out,
  output logic [NUM_OUT * DATA_WIDTH-1:0] data_out,
  output logic signed [DATA_WIDTH-1:0] peak_value
);

  // Registered inputs
  logic [N * DATA_WIDTH-1:0] data_in_reg;
  logic [0:0] valid_in_reg;
  // Unpacked registered input as Vec for easy indexing (also exposed for debug)
  logic signed [N-1:0] [DATA_WIDTH-1:0] data_vec_in;
  // Decimated samples
  logic signed [NUM_OUT-1:0] [DATA_WIDTH-1:0] dec_vec;
  // Peak accumulator: peak_acc[i] = max of dec_vec[0..i]
  logic signed [NUM_OUT-1:0] [DATA_WIDTH-1:0] peak_acc;
  // Output packing accumulator (MSB-first: element 0 at MSB)
  logic [NUM_OUT + 1-1:0] [NUM_OUT * DATA_WIDTH-1:0] out_acc;
  always_ff @(posedge clk or posedge reset) begin
    if (reset) begin
      data_in_reg <= 0;
      valid_in_reg <= 0;
    end else begin
      data_in_reg <= data_in;
      valid_in_reg <= valid_in;
    end
  end
  // Unpack registered data (LSB-first: element 0 at low bits)
  always_comb begin
    for (int i = 0; i <= N - 1; i++) begin
      data_vec_in[i] = $signed(DATA_WIDTH'(data_in_reg >> i * DATA_WIDTH));
    end
  end
  // Decimate: keep elements at indices DEC_FACTOR-1, 2*DEC_FACTOR-1, ...
  always_comb begin
    for (int j = 0; j <= NUM_OUT - 1; j++) begin
      dec_vec[j] = data_vec_in[DEC_FACTOR - 1 + j * DEC_FACTOR];
    end
  end
  // Peak detection (signed max of decimated samples)
  // peak_acc[0] = dec_vec[0], peak_acc[j] = max(peak_acc[j-1], dec_vec[j])
  always_comb begin
    peak_acc[0] = dec_vec[0];
    for (int j = 1; j <= NUM_OUT - 1; j++) begin
      if (dec_vec[j] > peak_acc[j - 1]) begin
        peak_acc[j] = dec_vec[j];
      end else begin
        peak_acc[j] = peak_acc[j - 1];
      end
    end
  end
  // Pack output MSB-first: dec_vec[0] at highest bits
  always_comb begin
    out_acc[0] = 0;
    for (int j = 0; j <= NUM_OUT - 1; j++) begin
      out_acc[j + 1] = out_acc[j] | (NUM_OUT * DATA_WIDTH)'($unsigned(DATA_WIDTH'($unsigned(dec_vec[j])))) << (NUM_OUT - 1 - j) * DATA_WIDTH;
    end
  end
  assign valid_out = valid_in_reg;
  assign data_out = out_acc[NUM_OUT];
  assign peak_value = peak_acc[NUM_OUT - 1];

endmodule

