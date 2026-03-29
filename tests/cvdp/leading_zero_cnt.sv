module leading_zero_cnt #(
  parameter int DATA_WIDTH = 32,
  parameter int REVERSE = 0,
  parameter int OUT_WIDTH = $clog2(DATA_WIDTH)
) (
  input logic [DATA_WIDTH-1:0] data,
  output logic [OUT_WIDTH-1:0] leading_zeros,
  output logic all_zeros
);

  logic [OUT_WIDTH-1:0] result;
  logic found;
  always_comb begin
    // Find first set bit from LSB (trailing zero count for REVERSE=1)
    result = 0;
    found = 1'b0;
    for (int i = 0; i <= DATA_WIDTH - 1; i++) begin
      if (~found & data[i +: 1]) begin
        result = OUT_WIDTH'(i);
        found = 1'b1;
      end
    end
    leading_zeros = result;
    all_zeros = data == 0;
  end

endmodule

