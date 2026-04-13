module digital_dice_roller #(
  parameter int DICE_MAX = 6
) (
  input logic clk,
  input logic reset,
  input logic button,
  output logic [2:0] dice_value
);

  always_ff @(posedge clk or negedge reset) begin
    if ((!reset)) begin
      dice_value <= 1;
    end else begin
      if (button) begin
        if (dice_value >= DICE_MAX) begin
          dice_value <= 3'd1;
        end else begin
          dice_value <= 3'(dice_value + 3'd1);
        end
      end
    end
  end

endmodule

