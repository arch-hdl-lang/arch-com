module glitch_free_mux (
  input logic clk1,
  input logic clk2,
  input logic rst_n,
  input logic sel,
  output logic clkout
);

  logic en1;
  logic en2;
  always_ff @(posedge clk1 or negedge rst_n) begin
    if ((!rst_n)) begin
      en1 <= 1'b0;
    end else begin
      en1 <= ~sel & ~en2;
    end
  end
  always_ff @(posedge clk2 or negedge rst_n) begin
    if ((!rst_n)) begin
      en2 <= 1'b0;
    end else begin
      en2 <= sel & ~en1;
    end
  end
  assign clkout = clk1 & en1 | clk2 & en2;

endmodule

