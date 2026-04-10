module continuous_adder #(
  parameter int DATA_WIDTH = 32,
  parameter int THRESHOLD_VALUE = 100,
  parameter int SIGNED_INPUTS = 1
) (
  input logic clk,
  input logic reset,
  input logic [DATA_WIDTH-1:0] data_in,
  input logic data_valid,
  output logic [DATA_WIDTH-1:0] sum_out,
  output logic sum_ready
);

  logic [DATA_WIDTH-1:0] sum_accum;
  logic [DATA_WIDTH-1:0] sum_out_r;
  logic sum_ready_r;
  // Threshold as properly-sized signal
  logic [DATA_WIDTH-1:0] thresh_u;
  assign thresh_u = DATA_WIDTH'($unsigned(THRESHOLD_VALUE));
  logic signed [DATA_WIDTH-1:0] thresh_s;
  assign thresh_s = $signed(thresh_u);
  logic signed [DATA_WIDTH-1:0] neg_thresh_s;
  assign neg_thresh_s = -thresh_s;
  // Combinational next-sum and threshold detection
  logic [DATA_WIDTH-1:0] next_sum;
  assign next_sum = data_valid ? DATA_WIDTH'(sum_accum + data_in) : sum_accum;
  logic signed [DATA_WIDTH-1:0] next_sum_s;
  assign next_sum_s = $signed(next_sum);
  logic thresh_hit;
  always_comb begin
    if (SIGNED_INPUTS == 1) begin
      thresh_hit = next_sum_s >= thresh_s | neg_thresh_s >= next_sum_s;
    end else begin
      thresh_hit = next_sum >= thresh_u;
    end
  end
  always_ff @(posedge clk) begin
    if (reset) begin
      sum_accum <= 0;
      sum_out_r <= 0;
      sum_ready_r <= 0;
    end else begin
      if (data_valid) begin
        if (thresh_hit) begin
          sum_accum <= 0;
          sum_out_r <= next_sum;
          sum_ready_r <= 1'd1;
        end else begin
          sum_accum <= next_sum;
          sum_ready_r <= 1'd0;
        end
      end else begin
        sum_ready_r <= 1'd0;
      end
    end
  end
  assign sum_out = sum_out_r;
  assign sum_ready = sum_ready_r;

endmodule

