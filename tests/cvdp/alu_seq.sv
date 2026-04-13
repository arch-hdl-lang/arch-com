module alu_seq #(
  parameter int p_key = 170
) (
  input logic i_clk,
  input logic i_rst_b,
  input logic [3:0] i_operand_a,
  input logic [3:0] i_operand_b,
  input logic [2:0] i_opcode,
  input logic [7:0] i_key_in,
  output logic [7:0] o_result
);

  logic [7:0] result;
  logic key_match;
  assign key_match = i_key_in == p_key;
  assign o_result = result;
  always_ff @(posedge i_clk or negedge i_rst_b) begin
    if ((!i_rst_b)) begin
      result <= 0;
    end else begin
      if (key_match) begin
        if (i_opcode == 0) begin
          result <= 8'(9'($unsigned(i_operand_a)) + 9'($unsigned(i_operand_b)));
        end else if (i_opcode == 1) begin
          result <= 8'(9'($unsigned(i_operand_a)) - 9'($unsigned(i_operand_b)));
        end else if (i_opcode == 2) begin
          result <= 8'(8'($unsigned(i_operand_a)) * 8'($unsigned(i_operand_b)));
        end else if (i_opcode == 3) begin
          result <= 8'($unsigned(i_operand_a)) & 8'($unsigned(i_operand_b));
        end else if (i_opcode == 4) begin
          result <= 8'($unsigned(i_operand_a)) | 8'($unsigned(i_operand_b));
        end else if (i_opcode == 5) begin
          result <= 8'($unsigned(~i_operand_a));
        end else if (i_opcode == 6) begin
          result <= 8'($unsigned(i_operand_a)) ^ 8'($unsigned(i_operand_b));
        end else begin
          result <= 8'($unsigned(~(i_operand_a ^ i_operand_b)));
        end
      end else begin
        result <= 0;
      end
    end
  end

endmodule

