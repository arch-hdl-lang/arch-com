module binary_bcd_converter_twoway #(
  parameter int BCD_DIGITS = 3,
  parameter int INPUT_WIDTH = 9,
  localparam int BCD_WIDTH = BCD_DIGITS * 4,
  localparam int TOTAL_WIDTH = INPUT_WIDTH + BCD_WIDTH
) (
  input logic [0:0] switch,
  input logic [BCD_WIDTH-1:0] bcd_in,
  input logic [INPUT_WIDTH-1:0] binary_in,
  output logic [INPUT_WIDTH-1:0] binary_out,
  output logic [BCD_WIDTH-1:0] bcd_out
);

  // Double-dabble shift register for binary-to-BCD
  logic [TOTAL_WIDTH-1:0] dd;
  // Accumulator for BCD-to-binary
  logic [INPUT_WIDTH-1:0] bin_acc;
  // Temporary digit for BCD-to-binary
  logic [3:0] digit;
  always_comb begin
    // === Binary-to-BCD (Double Dabble) ===
    dd = TOTAL_WIDTH'($unsigned(binary_in));
    for (int i = 0; i <= INPUT_WIDTH - 1; i++) begin
      for (int j = 0; j <= BCD_DIGITS - 1; j++) begin
        if (dd[INPUT_WIDTH + j * 4 +: 4] >= 5) begin
          dd[INPUT_WIDTH + j * 4 +: 4] = 4'(dd[INPUT_WIDTH + j * 4 +: 4] + 3);
        end
      end
      dd = dd << 1;
    end
    // Check each BCD digit; if >= 5, add 3
    // Shift left
    // === BCD-to-Binary ===
    bin_acc = 0;
    digit = 0;
    for (int k = 0; k <= BCD_DIGITS - 1; k++) begin
      digit = bcd_in[((BCD_DIGITS - 1) - k) * 4 +: 4];
      bin_acc = INPUT_WIDTH'(bin_acc * 10 + INPUT_WIDTH'($unsigned(digit)));
    end
    // Output selection based on switch
    if (switch == 1) begin
      bcd_out = dd[TOTAL_WIDTH - 1:INPUT_WIDTH];
      binary_out = 0;
    end else begin
      binary_out = bin_acc;
      bcd_out = 0;
    end
  end

endmodule

