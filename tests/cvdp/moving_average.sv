module moving_average (
  input logic clk,
  input logic reset,
  input logic [12-1:0] data_in,
  output logic [12-1:0] data_out
);

  logic [12-1:0] mem [0:8-1];
  logic [15-1:0] sum_reg = 0;
  always_ff @(posedge clk) begin
    if (reset) begin
      for (int __ri0 = 0; __ri0 < 8; __ri0++) begin
        mem[__ri0] <= 0;
      end
      sum_reg <= 0;
    end else begin
      sum_reg <= 15'(sum_reg - 15'($unsigned(mem[7])) + 15'($unsigned(data_in)));
      for (int i = 1; i <= 7; i++) begin
        mem[i] <= mem[i - 1];
      end
      mem[0] <= data_in;
    end
  end
  assign data_out = 12'(sum_reg >> 3);

endmodule

