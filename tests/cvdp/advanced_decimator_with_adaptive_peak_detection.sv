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
  // data_vec_in as Vec for debug access (dut.data_vec_in[i])
  // MSB-first: element i at shift (N-1-i)*DATA_WIDTH
  logic signed [N-1:0] [DATA_WIDTH-1:0] data_vec_in;
  // Scalar accumulators to avoid packed-2D-array iverilog bugs
  // data_out_acc: running packed output accumulator
  // peak_acc: running max value (signed)
  logic [NUM_OUT * DATA_WIDTH-1:0] data_out_acc;
  logic signed [DATA_WIDTH-1:0] peak_cur;
  always_ff @(posedge clk or posedge reset) begin
    if (reset) begin
      data_in_reg <= 0;
      valid_in_reg <= 0;
    end else begin
      data_in_reg <= data_in;
      valid_in_reg <= valid_in;
    end
  end
  // Unpack registered data MSB-first: element i at shift (N-1-i)*DATA_WIDTH
  // pack_signal packs values[0] at MSB: values[i] at shift (N-1-i)*DATA_WIDTH
  always_comb begin
    for (int i = 0; i <= N - 1; i++) begin
      data_vec_in[i] = $signed(DATA_WIDTH'(data_in_reg >> (N - 1 - i) * DATA_WIDTH));
    end
  end
  // Decimate and pack output (MSB-first) and compute peak - scalar accumulators
  // dec_vec[j] = data_vec_in[DEC_FACTOR-1 + j*DEC_FACTOR] = sample at index DEC_FACTOR-1+j*DEC_FACTOR
  // = (data_in_reg >> ((N - 1 - (DEC_FACTOR-1 + j*DEC_FACTOR)) * DATA_WIDTH)).trunc...
  // = (data_in_reg >> ((N - DEC_FACTOR - j*DEC_FACTOR) * DATA_WIDTH)).trunc...
  // Output packing MSB-first: dec_vec[0] at bits[(NUM_OUT-1)*DW +: DW], dec_vec[j] at (NUM_OUT-1-j)*DW
  always_comb begin
    data_out_acc = 0;
    peak_cur = $signed(DATA_WIDTH'(data_in_reg >> (N - DEC_FACTOR) * DATA_WIDTH));
    for (int j = 0; j <= NUM_OUT - 1; j++) begin
      data_out_acc = data_out_acc | (NUM_OUT * DATA_WIDTH)'($unsigned(DATA_WIDTH'(data_in_reg >> (N - DEC_FACTOR - j * DEC_FACTOR) * DATA_WIDTH))) << (NUM_OUT - 1 - j) * DATA_WIDTH;
      if ($signed(DATA_WIDTH'(data_in_reg >> (N - DEC_FACTOR - j * DEC_FACTOR) * DATA_WIDTH)) > peak_cur) begin
        peak_cur = $signed(DATA_WIDTH'(data_in_reg >> (N - DEC_FACTOR - j * DEC_FACTOR) * DATA_WIDTH));
      end
    end
  end
  assign valid_out = valid_in_reg;
  assign data_out = data_out_acc;
  assign peak_value = peak_cur;

endmodule

