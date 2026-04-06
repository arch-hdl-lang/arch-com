module pipeline_mac #(
  parameter int DWIDTH = 16,
  parameter int N = 4,
  parameter int DWIDTH_ACCUMULATOR = 34
) (
  input logic clk,
  input logic rstn,
  input logic [DWIDTH-1:0] multiplicand,
  input logic [DWIDTH-1:0] multiplier,
  input logic valid_i,
  output logic [DWIDTH_ACCUMULATOR-1:0] result,
  output logic valid_out
);

  logic [DWIDTH_ACCUMULATOR-1:0] mult_result_reg;
  logic [DWIDTH_ACCUMULATOR-1:0] accumulation_reg;
  logic [$clog2(N) + 1-1:0] counter_reg;
  logic valid_out_s1;
  logic valid_out_s2;
  logic valid_i_s1;
  logic valid_out_s0;
  assign valid_out_s0 = counter_reg == ($clog2(N) + 1)'(N - 1);
  logic count_rst;
  assign count_rst = valid_out_s1;
  logic accumulator_rst;
  assign accumulator_rst = valid_out_s1;
  assign result = accumulation_reg;
  assign valid_out = valid_out_s1 & ~valid_out_s2;
  // Extend inputs for full-width multiplication
  logic [DWIDTH_ACCUMULATOR-1:0] mcand_ext;
  assign mcand_ext = DWIDTH_ACCUMULATOR'($unsigned(multiplicand));
  logic [DWIDTH_ACCUMULATOR-1:0] mplier_ext;
  assign mplier_ext = DWIDTH_ACCUMULATOR'($unsigned(multiplier));
  logic [DWIDTH_ACCUMULATOR-1:0] mult_product;
  assign mult_product = DWIDTH_ACCUMULATOR'(mcand_ext * mplier_ext);
  // Stage 1: Multiplication
  always_ff @(posedge clk or negedge rstn) begin
    if ((!rstn)) begin
      mult_result_reg <= 0;
      valid_i_s1 <= 1'b0;
    end else begin
      if (valid_i) begin
        mult_result_reg <= mult_product;
      end
      valid_i_s1 <= valid_i;
    end
  end
  // Stage 2: Accumulation
  always_ff @(posedge clk or negedge rstn) begin
    if ((!rstn)) begin
      accumulation_reg <= 0;
    end else begin
      if (accumulator_rst) begin
        accumulation_reg <= mult_result_reg;
      end else if (valid_i_s1) begin
        accumulation_reg <= DWIDTH_ACCUMULATOR'(accumulation_reg + mult_result_reg);
      end
    end
  end
  // Counter: update on valid_i_s1 or count_rst
  always_ff @(posedge clk or negedge rstn) begin
    if ((!rstn)) begin
      counter_reg <= 0;
    end else begin
      if (count_rst) begin
        counter_reg <= ($clog2(N) + 1)'($unsigned(1));
      end else if (valid_i_s1) begin
        counter_reg <= ($clog2(N) + 1)'(counter_reg + 1);
      end
    end
  end
  // Valid output pipeline
  always_ff @(posedge clk or negedge rstn) begin
    if ((!rstn)) begin
      valid_out_s1 <= 1'b0;
      valid_out_s2 <= 1'b0;
    end else begin
      valid_out_s1 <= valid_out_s0 & valid_i_s1;
      valid_out_s2 <= valid_out_s1;
    end
  end

endmodule

