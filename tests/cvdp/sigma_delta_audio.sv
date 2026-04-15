module sigma_delta_audio (
  input logic clk_sig,
  input logic clk_en_sig,
  input logic [14:0] load_data_sum,
  output logic [14:0] read_data_sum,
  output logic left_sig,
  output logic right_sig
);

  // Two independent sigma-delta accumulators: left and right channels
  // The load_data_sum is the audio data written each clock enable cycle
  // read_data_sum feeds back the current accumulator value
  // left_sig/right_sig are the 1-bit PDM outputs (MSB of accumulator)
  logic [14:0] acc_left;
  logic [14:0] acc_right;
  logic left_r;
  logic right_r;
  logic [14:0] rd_sum_r;
  // Sigma-delta: accumulate input, output MSB as bitstream
  logic [15:0] sum_left;
  assign sum_left = 16'(16'($unsigned(acc_left)) + 16'($unsigned(load_data_sum)));
  logic [15:0] sum_right;
  assign sum_right = 16'(16'($unsigned(acc_right)) + 16'($unsigned(load_data_sum)));
  assign left_sig = left_r;
  assign right_sig = right_r;
  assign read_data_sum = rd_sum_r;
  always_ff @(posedge clk_sig) begin
    if (clk_en_sig) begin
      acc_left <= 15'(sum_left);
      acc_right <= 15'(sum_right);
      left_r <= sum_left[15:15] == 1;
      right_r <= sum_right[15:15] == 1;
      rd_sum_r <= acc_left;
    end
  end

endmodule

