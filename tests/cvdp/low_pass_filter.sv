module low_pass_filter #(
  parameter int DATA_WIDTH = 16,
  parameter int COEFF_WIDTH = 16,
  parameter int NUM_TAPS = 8,
  parameter int NBW_MULT = DATA_WIDTH + COEFF_WIDTH,
  parameter int OUT_WIDTH = NBW_MULT + $clog2(NUM_TAPS)
) (
  input logic clk,
  input logic reset,
  input logic [DATA_WIDTH * NUM_TAPS-1:0] data_in,
  input logic valid_in,
  input logic [COEFF_WIDTH * NUM_TAPS-1:0] coeffs,
  output logic signed [OUT_WIDTH-1:0] data_out,
  output logic valid_out
);

  logic signed [DATA_WIDTH-1:0] data_reg [NUM_TAPS-1:0];
  logic signed [COEFF_WIDTH-1:0] coeff_reg [NUM_TAPS-1:0];
  always_ff @(posedge clk) begin
    if (reset) begin
      for (int __ri0 = 0; __ri0 < NUM_TAPS; __ri0++) begin
        coeff_reg[__ri0] <= 0;
      end
      for (int __ri0 = 0; __ri0 < NUM_TAPS; __ri0++) begin
        data_reg[__ri0] <= 0;
      end
    end else begin
      if (reset) begin
        for (int i = 0; i <= NUM_TAPS - 1; i++) begin
          data_reg[i] <= 0;
          coeff_reg[i] <= 0;
        end
      end else if (valid_in) begin
        for (int i = 0; i <= NUM_TAPS - 1; i++) begin
          data_reg[i] <= $signed(data_in[i * DATA_WIDTH +: DATA_WIDTH]);
          coeff_reg[i] <= $signed(coeffs[i * COEFF_WIDTH +: COEFF_WIDTH]);
        end
      end
    end
  end
  always_ff @(posedge clk) begin
    if (reset) begin
      valid_out <= 1'b0;
    end else begin
      if (reset) begin
        valid_out <= 1'b0;
      end else begin
        valid_out <= valid_in;
      end
    end
  end
  logic signed [NBW_MULT-1:0] mult [NUM_TAPS-1:0];
  logic signed [OUT_WIDTH-1:0] acc;
  always_comb begin
    for (int i = 0; i <= NUM_TAPS - 1; i++) begin
      mult[i] = data_reg[i] * coeff_reg[NUM_TAPS - 1 - i];
    end
  end
  always_comb begin
    acc = 0;
    for (int i = 0; i <= NUM_TAPS - 1; i++) begin
      acc = OUT_WIDTH'(acc + {{(OUT_WIDTH-$bits(mult[i])){mult[i][$bits(mult[i])-1]}}, mult[i]});
    end
  end
  assign data_out = acc;

endmodule

