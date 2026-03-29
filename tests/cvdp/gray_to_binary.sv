module gray_to_binary #(
  parameter int WIDTH = 4,
  parameter int DEBUG_MODE = 0
) (
  input logic [WIDTH-1:0] gray_in,
  output logic [WIDTH-1:0] binary_out,
  output logic [WIDTH-1:0] debug_mask,
  output logic parity,
  output logic valid
);

  logic [WIDTH-1:0] intermediate_stage_1;
  logic [WIDTH-1:0] intermediate_stage_2;
  logic [WIDTH-1:0] masked_output;
  logic [WIDTH-1:0] final_binary;
  logic valid_stage_1;
  logic valid_stage_2;
  always_comb begin
    intermediate_stage_1[WIDTH - 1] = gray_in[WIDTH - 1];
    for (int i = 0; i <= WIDTH - 2; i++) begin
      intermediate_stage_1[WIDTH - 2 - i] = intermediate_stage_1[WIDTH - 1 - i] ^ gray_in[WIDTH - 2 - i];
    end
    valid_stage_1 = 1;
    if (DEBUG_MODE == 1) begin
      masked_output = intermediate_stage_1;
      intermediate_stage_2 = masked_output;
      debug_mask = ~intermediate_stage_2;
    end else begin
      masked_output = intermediate_stage_1;
      intermediate_stage_2 = masked_output;
      debug_mask = 0;
    end
    valid_stage_2 = valid_stage_1;
    final_binary = intermediate_stage_2;
    binary_out = final_binary;
    parity = ^final_binary;
    valid = valid_stage_2;
  end

endmodule

