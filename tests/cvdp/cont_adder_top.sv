module cont_adder_top #(
  parameter int DATA_WIDTH = 32,
  parameter int THRESHOLD_VALUE = 100,
  parameter int SIGNED_INPUTS = 1
) (
  input logic clk,
  input logic rst_n,
  input logic signed [DATA_WIDTH-1:0] data_in,
  output logic signed [DATA_WIDTH-1:0] sum_out,
  output logic threshold_reached
);

  logic signed [DATA_WIDTH-1:0] accumulator;
  logic thresh;
  logic thresh_pos;
  logic thresh_neg;
  always_comb begin
    thresh_pos = accumulator >= {{(DATA_WIDTH-$bits(THRESHOLD_VALUE)){THRESHOLD_VALUE[$bits(THRESHOLD_VALUE)-1]}}, THRESHOLD_VALUE};
    thresh_neg = accumulator < -{{(DATA_WIDTH-$bits(THRESHOLD_VALUE)){THRESHOLD_VALUE[$bits(THRESHOLD_VALUE)-1]}}, THRESHOLD_VALUE};
    if (SIGNED_INPUTS == 1) begin
      thresh = thresh_pos | thresh_neg;
    end else begin
      thresh = thresh_pos;
    end
    threshold_reached = thresh;
    sum_out = accumulator;
  end
  always_ff @(posedge clk) begin
    if ((!rst_n)) begin
      accumulator <= 0;
    end else begin
      if (thresh) begin
        accumulator <= 0;
      end else begin
        accumulator <= DATA_WIDTH'(accumulator + data_in);
      end
    end
  end

endmodule

