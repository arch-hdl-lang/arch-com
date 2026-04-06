// Clock divider using as Clock<Domain> cast
module ClkDiv2 (
  input logic clk_in,
  input logic rst_n,
  output logic clk_out
);

  logic toggle_r;
  always_ff @(posedge clk_in or negedge rst_n) begin
    if ((!rst_n)) begin
      toggle_r <= 1'b0;
    end else begin
      toggle_r <= ~toggle_r;
    end
  end
  assign clk_out = logic'(toggle_r);

endmodule

