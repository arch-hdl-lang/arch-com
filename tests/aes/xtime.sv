// GF(2^8) multiply by 2
// xtime(a) = (a << 1) ^ (0x1b if a[7] else 0x00)
module Xtime (
  input logic [8-1:0] a,
  output logic [8-1:0] y
);

  always_comb begin
    case ((a & 'h80))
      'h80: y = (8'((a << 1)) ^ 'h1B);
      default: y = 8'((a << 1));
    endcase
  end

endmodule

