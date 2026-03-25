// Test: counter clocked by a divided clock
// Verifies sim eval ordering for derived clocks
module ClkDivCounter (
  input logic clk,
  input logic rst_n,
  output logic [8-1:0] count,
  output logic div_clk
);

  // Divide-by-2 instance
  logic clk_slow;
  ClkDiv2 div (
    .clk_in(clk),
    .rst_n(rst_n),
    .clk_out(clk_slow)
  );
  assign div_clk = clk_slow;
  // Counter on the divided clock
  logic [8-1:0] counter_r = 0;
  always_ff @(posedge clk_slow or negedge rst_n) begin
    if ((!rst_n)) begin
      counter_r <= 0;
    end else begin
      counter_r <= 8'((counter_r + 1));
    end
  end
  assign count = counter_r;

endmodule

