// Init counter for free-list bank population.
// Counts 0..16383, asserts at_max when done.
module SetupCounter #(
  parameter int MAX = 16383
) (
  input logic clk,
  input logic rst,
  input logic inc,
  output logic [14-1:0] value,
  output logic at_max
);

  logic [14-1:0] count_r;
  always_ff @(posedge clk) begin
    if (rst) count_r <= 0;
    else if (inc) begin
      if (count_r == 14'(MAX)) count_r <= 0;
      else count_r <= count_r + 1;
    end
  end
  assign value = count_r;
  assign at_max = (count_r == 14'(MAX));

endmodule

