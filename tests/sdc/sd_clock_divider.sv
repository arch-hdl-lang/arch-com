// SD Clock Divider
// Generates SD_CLK at CLK/(2*(DIVIDER+1)) frequency.
module sd_clock_divider (
  input logic CLK,
  input logic RST,
  input logic [8-1:0] DIVIDER,
  output logic SD_CLK
);

  logic [8-1:0] clock_div;
  logic sd_clk_o;
  always_ff @(posedge CLK or posedge RST) begin
    if (RST) begin
      clock_div <= 0;
      sd_clk_o <= 1'b0;
    end else begin
      if (clock_div == DIVIDER) begin
        clock_div <= 0;
        sd_clk_o <= ~sd_clk_o;
      end else begin
        clock_div <= 8'(clock_div + 1);
      end
    end
  end
  assign SD_CLK = sd_clk_o;

endmodule

