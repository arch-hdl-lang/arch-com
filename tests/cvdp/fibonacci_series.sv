module fibonacci_series (
  input logic clk,
  input logic rst,
  output logic [32-1:0] fib_out,
  output logic overflow_flag
);

  logic [32-1:0] RegA;
  logic [32-1:0] RegB;
  logic overflow_detected;
  logic [33-1:0] next_fib;
  assign next_fib = RegA + RegB;
  always_ff @(posedge clk or posedge rst) begin
    if (rst) begin
      RegA <= 0;
      RegB <= 1;
      fib_out <= 0;
      overflow_detected <= 0;
      overflow_flag <= 0;
    end else begin
      if (overflow_detected) begin
        overflow_flag <= 1'd1;
        fib_out <= 32'd0;
        RegA <= 32'd0;
        RegB <= 32'd1;
        overflow_detected <= 1'd0;
      end else if (next_fib[32]) begin
        overflow_detected <= 1'd1;
        fib_out <= RegB;
      end else begin
        overflow_flag <= 1'd0;
        fib_out <= RegB;
        RegA <= RegB;
        RegB <= 32'(next_fib);
      end
    end
  end

endmodule

