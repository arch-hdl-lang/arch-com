module Data_Reduction #(
  parameter int REDUCTION_OP = 0,
  parameter int DATA_WIDTH = 4,
  parameter int DATA_COUNT = 4,
  parameter int TOTAL_INPUT_WIDTH = DATA_WIDTH * DATA_COUNT
) (
  input logic [TOTAL_INPUT_WIDTH-1:0] data_in,
  output logic [DATA_WIDTH-1:0] reduced_data_out
);

  // One word per input element, extracted from flat data_in
  logic [DATA_COUNT-1:0] [DATA_WIDTH-1:0] words;
  // Running reductions across all DATA_COUNT words
  logic [DATA_WIDTH-1:0] and_result;
  logic [DATA_WIDTH-1:0] or_result;
  logic [DATA_WIDTH-1:0] xor_result;
  // Unpack flat input into word array
  always_comb begin
    for (int i = 0; i <= DATA_COUNT - 1; i++) begin
      words[i] = data_in[i * DATA_WIDTH +: DATA_WIDTH];
    end
  end
  // Fold words[0..DATA_COUNT-1] with AND, OR, and XOR simultaneously
  always_comb begin
    and_result = words[0];
    or_result = words[0];
    xor_result = words[0];
    for (int i = 1; i <= DATA_COUNT - 1; i++) begin
      and_result = and_result & words[i];
      or_result = or_result | words[i];
      xor_result = xor_result ^ words[i];
    end
  end
  // Select output based on REDUCTION_OP parameter
  //   0=AND  1=OR  2=XOR  3=NAND  4=NOR  5=XNOR  6/7=AND (default)
  always_comb begin
    if (REDUCTION_OP == 1) begin
      reduced_data_out = or_result;
    end else if (REDUCTION_OP == 2) begin
      reduced_data_out = xor_result;
    end else if (REDUCTION_OP == 3) begin
      reduced_data_out = ~and_result;
    end else if (REDUCTION_OP == 4) begin
      reduced_data_out = ~or_result;
    end else if (REDUCTION_OP == 5) begin
      reduced_data_out = ~xor_result;
    end else begin
      reduced_data_out = and_result;
    end
  end

endmodule

