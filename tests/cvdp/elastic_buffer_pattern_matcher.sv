// Elastic buffer pattern matcher: approximate match within error tolerance
module elastic_buffer_pattern_matcher #(
  parameter int WIDTH = 16,
  parameter int ERR_TOLERANCE = 2
) (
  input logic clk,
  input logic rst,
  input logic [WIDTH-1:0] i_data,
  input logic [WIDTH-1:0] i_pattern,
  output logic o_match
);

  logic [WIDTH-1:0] xor_data;
  assign xor_data = i_data ^ i_pattern;
  // Count ones (popcount)
  logic [WIDTH-1:0] err_count;
  always_comb begin
    err_count = 0;
    for (int i = 0; i <= WIDTH - 1; i++) begin
      err_count = WIDTH'(err_count + WIDTH'($unsigned(xor_data[i +: 1])));
    end
  end
  // Register the match output (1-cycle latency)
  logic match_r;
  always_ff @(posedge clk) begin
    if (rst) begin
      match_r <= 1'b0;
    end else begin
      if (err_count < ERR_TOLERANCE) begin
        match_r <= 1'b1;
      end else begin
        match_r <= 1'b0;
      end
    end
  end
  assign o_match = match_r;

endmodule

