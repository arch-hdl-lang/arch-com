// 4-tap FIR filter with 4-cycle latency
module fir_filter (
  input logic clk,
  input logic reset,
  input logic signed [15:0] input_sample,
  output logic signed [15:0] output_sample,
  input logic signed [15:0] coeff0,
  input logic signed [15:0] coeff1,
  input logic signed [15:0] coeff2,
  input logic signed [15:0] coeff3
);

  // 3-stage delay line over input_sample. Tap K = K cycles of delay
  // from the input: @0 = input_sample, @1..@2 = intermediate stages,
  // @3 = bare `sample_pipe` (final output).
  logic signed [15:0] sample_pipe_stg1;
  logic signed [15:0] sample_pipe_stg2;
  logic signed [15:0] sample_pipe;
  always_ff @(posedge clk) begin
    if (reset) begin
      sample_pipe_stg1 <= '0;
      sample_pipe_stg2 <= '0;
      sample_pipe <= '0;
    end else begin
      sample_pipe_stg1 <= input_sample;
      sample_pipe_stg2 <= sample_pipe_stg1;
      sample_pipe <= sample_pipe_stg2;
    end
  end
  logic signed [31:0] accumulator;
  logic signed [15:0] out_reg;
  logic signed [31:0] prod0;
  logic signed [31:0] prod1;
  logic signed [31:0] prod2;
  logic signed [31:0] prod3;
  logic signed [31:0] acc_sum;
  assign prod0 = input_sample * coeff0;
  assign prod1 = sample_pipe_stg1 * coeff1;
  assign prod2 = sample_pipe_stg2 * coeff2;
  assign prod3 = sample_pipe * coeff3;
  assign acc_sum = 32'({{(34-$bits(prod0)){prod0[$bits(prod0)-1]}}, prod0} + {{(34-$bits(prod1)){prod1[$bits(prod1)-1]}}, prod1} + {{(34-$bits(prod2)){prod2[$bits(prod2)-1]}}, prod2} + {{(34-$bits(prod3)){prod3[$bits(prod3)-1]}}, prod3});
  always_ff @(posedge clk or posedge reset) begin
    if (reset) begin
      accumulator <= 0;
      out_reg <= 0;
    end else begin
      accumulator <= acc_sum;
      out_reg <= 16'(accumulator);
    end
  end
  assign output_sample = out_reg;

endmodule

