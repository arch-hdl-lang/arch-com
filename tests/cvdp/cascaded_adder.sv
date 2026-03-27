module cascaded_adder #(
  parameter int IN_DATA_WIDTH = 16,
  parameter int IN_DATA_NS = 4,
  parameter int OUT_WIDTH = IN_DATA_WIDTH + $clog2(IN_DATA_NS)
) (
  input logic clk,
  input logic rst_n,
  input logic i_valid,
  input logic [IN_DATA_WIDTH * IN_DATA_NS-1:0] i_data,
  output logic o_valid,
  output logic [OUT_WIDTH-1:0] o_data
);

  logic valid_d1;
  logic [OUT_WIDTH-1:0] sum_reg;
  logic [OUT_WIDTH-1:0] sum_comb;
  always_comb begin
    sum_comb = 0;
    for (int i = 0; i <= IN_DATA_NS - 1; i++) begin
      sum_comb = OUT_WIDTH'(sum_comb + OUT_WIDTH'($unsigned(i_data[i * IN_DATA_WIDTH + IN_DATA_WIDTH - 1:i * IN_DATA_WIDTH])));
    end
  end
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      o_data <= 0;
      o_valid <= 0;
      sum_reg <= 0;
      valid_d1 <= 0;
    end else begin
      valid_d1 <= i_valid;
      sum_reg <= sum_comb;
      o_valid <= valid_d1;
      o_data <= sum_reg;
    end
  end

endmodule

