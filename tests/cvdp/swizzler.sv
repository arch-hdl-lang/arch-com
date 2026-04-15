module swizzler #(
  parameter int N = 8,
  localparam int M = $clog2(N + 1),
  localparam int LOG_N = $clog2(N)
) (
  input logic clk,
  input logic reset,
  input logic [N-1:0] data_in,
  input logic [N * M-1:0] mapping_in,
  input logic config_in,
  input logic [2:0] operation_mode,
  output logic [N-1:0] data_out,
  output logic error_flag
);

  // Pipeline registers
  logic [N-1:0] swizzle_reg;
  logic error_reg;
  logic [N-1:0] operation_reg;
  // Combinational wires
  logic temp_error_flag;
  logic [N-1:0] temp_swizzled_data;
  logic [N-1:0] processed_swizzle_data;
  logic [N-1:0] op_result;
  // Swizzle + error detection
  always_comb begin
    temp_error_flag = 0;
    temp_swizzled_data = 0;
    for (int i = 0; i <= N - 1; i++) begin
      if ((M + 1)'($unsigned(mapping_in[i * M +: M])) >= (M + 1)'($unsigned(N))) begin
        temp_error_flag = 1;
      end
    end
    if (temp_error_flag == 1) begin
      temp_swizzled_data = 0;
    end else begin
      for (int i = 0; i <= N - 1; i++) begin
        temp_swizzled_data[i] = data_in[LOG_N'(mapping_in[i * M +: M])];
      end
    end
    // Config: straight or mirror
    if (config_in == 1) begin
      processed_swizzle_data = temp_swizzled_data;
    end else begin
      for (int i = 0; i <= N - 1; i++) begin
        processed_swizzle_data[i] = temp_swizzled_data[(N - 1) - i];
      end
    end
  end
  // Stage 1: swizzle register
  always_ff @(posedge clk) begin
    if (reset) begin
      error_reg <= 0;
      swizzle_reg <= 0;
    end else begin
      swizzle_reg <= processed_swizzle_data;
      error_reg <= temp_error_flag;
    end
  end
  // Operation mode
  always_comb begin
    if (operation_mode == 3'd0) begin
      op_result = swizzle_reg;
    end else if (operation_mode == 3'd1) begin
      op_result = swizzle_reg;
    end else if (operation_mode == 3'd2) begin
      // Reverse bits
      op_result = 0;
      for (int i = 0; i <= N - 1; i++) begin
        op_result[i] = swizzle_reg[(N - 1) - i];
      end
    end else if (operation_mode == 3'd3) begin
      // Swap halves
      op_result = {swizzle_reg[N / 2 - 1:0], swizzle_reg[N - 1:N / 2]};
    end else if (operation_mode == 3'd4) begin
      // Bitwise inversion
      op_result = ~swizzle_reg;
    end else if (operation_mode == 3'd5) begin
      // Circular left shift
      op_result = {swizzle_reg[N - 2:0], swizzle_reg[N - 1 +: 1]};
    end else if (operation_mode == 3'd6) begin
      // Circular right shift
      op_result = {swizzle_reg[0:0], swizzle_reg[N - 1:1]};
    end else begin
      op_result = swizzle_reg;
    end
  end
  // Stage 2: operation register
  always_ff @(posedge clk) begin
    if (reset) begin
      operation_reg <= 0;
    end else begin
      operation_reg <= op_result;
    end
  end
  // Stage 3: final bit reversal + output
  always_ff @(posedge clk) begin
    if (reset) begin
      data_out <= 0;
      error_flag <= 0;
    end else begin
      for (int i = 0; i <= N - 1; i++) begin
        data_out[i] <= operation_reg[(N - 1) - i];
      end
      error_flag <= error_reg;
    end
  end

endmodule

