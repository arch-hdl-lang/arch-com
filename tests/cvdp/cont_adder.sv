module cont_adder #(
  parameter int DATA_WIDTH = 16,
  parameter int ACCUM_WIDTH = 32,
  parameter int WINDOW_SIZE = 8,
  parameter int THRESHOLD = 1000,
  parameter int WEIGHT = 1
) (
  input logic clk,
  input logic rst_n,
  input logic [DATA_WIDTH-1:0] data_in,
  input logic valid_in,
  output logic [ACCUM_WIDTH-1:0] accum_out,
  output logic [DATA_WIDTH-1:0] avg_out,
  output logic threshold_high,
  output logic threshold_low
);

  logic [ACCUM_WIDTH-1:0] accum;
  logic [8-1:0] sample_cnt;
  logic [ACCUM_WIDTH-1:0] data_ext;
  logic [ACCUM_WIDTH-1:0] weighted;
  logic cnt_nonzero;
  assign data_ext = ACCUM_WIDTH'($unsigned(data_in));
  assign weighted = ACCUM_WIDTH'(data_ext * WEIGHT);
  assign cnt_nonzero = sample_cnt != 0;
  assign accum_out = accum;
  assign threshold_high = accum >= ACCUM_WIDTH'($unsigned(THRESHOLD));
  assign threshold_low = accum == 0;
  always_comb begin
    if (cnt_nonzero) begin
      avg_out = DATA_WIDTH'(accum / ACCUM_WIDTH'($unsigned(sample_cnt)));
    end else begin
      avg_out = 0;
    end
  end
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      accum <= 0;
      sample_cnt <= 0;
    end else begin
      if (valid_in) begin
        if (sample_cnt == 8'($unsigned(WINDOW_SIZE - 1))) begin
          accum <= weighted;
          sample_cnt <= 0;
        end else begin
          accum <= ACCUM_WIDTH'(accum + weighted);
          sample_cnt <= 8'(sample_cnt + 1);
        end
      end
    end
  end

endmodule

