module cont_adder #(
  parameter int DATA_WIDTH = 32,
  parameter int ACCUM_WIDTH = 32,
  parameter int THRESHOLD_VALUE_1 = 50,
  parameter int THRESHOLD_VALUE_2 = 100,
  parameter int WEIGHT = 1,
  parameter int ACCUM_MODE = 0,
  parameter int WINDOW_SIZE = 8
) (
  input logic clk,
  input logic rst_n,
  input logic signed [DATA_WIDTH-1:0] data_in,
  input logic data_valid,
  input logic [7:0] window_size,
  output logic signed [ACCUM_WIDTH-1:0] sum_out,
  output logic signed [DATA_WIDTH-1:0] avg_out,
  output logic threshold_1,
  output logic threshold_2,
  output logic sum_ready,
  output logic [ACCUM_WIDTH-1:0] accum_out,
  output logic threshold_high,
  output logic threshold_low,
  input logic valid_in
);

  // Backward-compatible aliases used by some older harnesses.
  logic signed [ACCUM_WIDTH-1:0] accum;
  logic [7:0] sample_cnt;
  logic use_valid;
  logic signed [ACCUM_WIDTH-1:0] data_ext;
  logic signed [ACCUM_WIDTH-1:0] weight_ext;
  logic signed [ACCUM_WIDTH-1:0] valid_weighted;
  logic signed [ACCUM_WIDTH-1:0] next_accum;
  logic thresh1_hit;
  logic thresh2_hit;
  logic [7:0] win_eff;
  logic win_done;
  logic cnt_nonzero;
  assign use_valid = data_valid | valid_in;
  assign data_ext = $signed(data_in);
  assign weight_ext = $signed(ACCUM_WIDTH'($unsigned(WEIGHT)));
  assign valid_weighted = ACCUM_WIDTH'(data_ext * weight_ext);
  assign next_accum = ACCUM_WIDTH'(accum + valid_weighted);
  assign thresh1_hit = next_accum >= $signed(ACCUM_WIDTH'($unsigned(THRESHOLD_VALUE_1)));
  assign thresh2_hit = next_accum >= $signed(ACCUM_WIDTH'($unsigned(THRESHOLD_VALUE_2)));
  assign win_eff = window_size;
  assign win_done = 8'(sample_cnt + 1) >= win_eff;
  assign cnt_nonzero = sample_cnt != 0;
  assign accum_out = ACCUM_WIDTH'($unsigned(sum_out));
  assign threshold_high = threshold_1;
  assign threshold_low = ~threshold_1 & ~threshold_2;
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      accum <= 0;
      avg_out <= 0;
      sample_cnt <= 0;
      sum_out <= 0;
      sum_ready <= 1'b0;
      threshold_1 <= 1'b0;
      threshold_2 <= 1'b0;
    end else begin
      sum_ready <= 1'b0;
      if (use_valid) begin
        if (ACCUM_MODE == 0) begin
          accum <= next_accum;
          sum_out <= next_accum;
          threshold_1 <= thresh1_hit;
          threshold_2 <= thresh2_hit;
          if (thresh1_hit | thresh2_hit) begin
            sum_ready <= 1'b1;
          end
        end else begin
          accum <= next_accum;
          sample_cnt <= 8'(sample_cnt + 1);
          if (win_done) begin
            sum_out <= next_accum;
            threshold_1 <= 1'b0;
            threshold_2 <= 1'b0;
            sum_ready <= 1'b1;
            accum <= 0;
            sample_cnt <= 0;
          end
        end
      end
      if (cnt_nonzero) begin
        avg_out <= sum_out / $signed(ACCUM_WIDTH'($unsigned(sample_cnt)));
      end else begin
        avg_out <= 0;
      end
    end
  end

endmodule

