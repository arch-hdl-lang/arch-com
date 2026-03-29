module cvdp_leading_zero_cnt #(
  parameter int DATA_WIDTH = 32,
  parameter int REVERSE = 0
) (
  input logic [DATA_WIDTH-1:0] data,
  output logic [$clog2(DATA_WIDTH)-1:0] leading_zeros,
  output logic all_zeros
);

  logic [$clog2(DATA_WIDTH)-1:0] zcount;
  logic found;
  always_comb begin
    zcount = 0;
    found = 1'b0;
    if (REVERSE == 1) begin
      // Trailing zero count: scan from LSB
      for (int i = 0; i <= DATA_WIDTH - 1; i++) begin
        if (~found & data[i] == 1) begin
          zcount = i[$clog2(DATA_WIDTH) - 1:0];
          found = 1'b1;
        end
      end
    end else begin
      // Leading zero count: scan from MSB
      for (int i = 0; i <= DATA_WIDTH - 1; i++) begin
        if (~found & data[DATA_WIDTH - 1 - i] == 1) begin
          zcount = i[$clog2(DATA_WIDTH) - 1:0];
          found = 1'b1;
        end
      end
    end
  end
  assign leading_zeros = zcount;
  assign all_zeros = data == 0;

endmodule

