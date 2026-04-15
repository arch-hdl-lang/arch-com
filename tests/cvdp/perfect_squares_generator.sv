module perfect_squares_generator (
  input logic clk,
  input logic reset,
  output logic [31:0] sqr_o
);

  // Incremental: (n+1)^2 = n^2 + 2n + 1 = sqr + odd, odd += 2
  // Start: sqr=1, odd=3 → 4, 9, 16, ...
  logic [32:0] odd_num;
  logic saturated;
  // UInt<32> + UInt<33> → UInt<34>, need trunc
  logic [33:0] next_sqr;
  assign next_sqr = sqr_o + odd_num;
  always_ff @(posedge clk or posedge reset) begin
    if (reset) begin
      odd_num <= 3;
      saturated <= 0;
      sqr_o <= 1;
    end else begin
      if (saturated) begin
        sqr_o <= 32'd4294967295;
      end else if (next_sqr[32]) begin
        sqr_o <= 32'd4294967295;
        saturated <= 1'd1;
      end else begin
        sqr_o <= 32'(next_sqr);
      end
      odd_num <= 33'(odd_num + 33'd2);
    end
  end

endmodule

