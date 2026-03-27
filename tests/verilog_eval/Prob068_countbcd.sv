module TopModule (
  input logic clk,
  input logic reset,
  output logic [3-1:0] ena,
  output logic [16-1:0] q
);

  logic [4-1:0] ones;
  logic [4-1:0] tens;
  logic [4-1:0] hund;
  logic [4-1:0] thou;
  always_ff @(posedge clk) begin
    if (reset) begin
      hund <= 0;
      ones <= 0;
      tens <= 0;
      thou <= 0;
    end else begin
      if (ones == 9) begin
        ones <= 0;
        if (tens == 9) begin
          tens <= 0;
          if (hund == 9) begin
            hund <= 0;
            thou <= thou == 9 ? 0 : 4'(thou + 1);
          end else begin
            hund <= 4'(hund + 1);
          end
        end else begin
          tens <= 4'(tens + 1);
        end
      end else begin
        ones <= 4'(ones + 1);
      end
    end
  end
  assign q = {thou, hund, tens, ones};
  assign ena[0] = ones == 9;
  assign ena[1] = ones == 9 & tens == 9;
  assign ena[2] = ones == 9 & tens == 9 & hund == 9;

endmodule

